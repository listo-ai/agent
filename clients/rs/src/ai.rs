//! AI runner operations — `GET /api/v1/ai/providers`, `POST /api/v1/ai/run`,
//! `POST /api/v1/ai/stream`.

use std::pin::Pin;

use futures_util::{Stream, StreamExt};

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{AiProviderStatus, AiRunRequest, AiRunResponse, AiStreamEvent};

pub struct Ai<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Ai<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// List registered AI providers with availability flags.
    pub async fn providers(&self) -> Result<Vec<AiProviderStatus>, ClientError> {
        self.http
            .get::<Vec<AiProviderStatus>>(&format!("{}/ai/providers", self.base))
            .await
    }

    /// Run a one-shot prompt through the shared registry (non-streaming).
    pub async fn run(&self, req: &AiRunRequest) -> Result<AiRunResponse, ClientError> {
        self.http
            .post(&format!("{}/ai/run", self.base), req)
            .await
    }

    /// Stream a prompt as a sequence of [`AiStreamEvent`]s. The final
    /// event is always [`AiStreamEvent::Result`] (or a preceding
    /// `Error` + `Result`). Errors from the HTTP layer surface as
    /// `ClientError`; malformed frames are dropped with a log.
    pub async fn stream(
        &self,
        req: &AiRunRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AiStreamEvent, ClientError>> + Send>>,
        ClientError,
    > {
        let res = self
            .http
            .post_raw(&format!("{}/ai/stream", self.base), req)
            .await?;
        let bytes = res.bytes_stream();
        Ok(Box::pin(sse::parse(bytes)))
    }
}

mod sse {
    use super::*;
    use reqwest::Error as ReqwestError;

    /// Parse an SSE byte stream into a stream of `AiStreamEvent`.
    ///
    /// Minimal parser — we only look at `data:` lines and join multi-line
    /// payloads with `\n` per the SSE spec. `event:`, `id:`, `retry:`
    /// lines are ignored; the event name is carried inside the JSON
    /// payload's `type` discriminator.
    pub fn parse<S, B>(bytes: S) -> impl Stream<Item = Result<AiStreamEvent, ClientError>>
    where
        S: Stream<Item = Result<B, ReqwestError>> + Unpin,
        B: AsRef<[u8]>,
    {
        let buf = String::new();
        futures_util::stream::unfold((bytes, buf), |(mut bytes, mut buf)| async move {
            loop {
                // Emit any completed frames already in the buffer.
                if let Some((frame, rest)) = pop_frame(&buf) {
                    buf = rest;
                    match decode(&frame) {
                        Some(ev) => return Some((ev, (bytes, buf))),
                        None => continue,
                    }
                }
                // Read more bytes.
                match bytes.next().await {
                    Some(Ok(chunk)) => {
                        // Silently drop non-UTF-8 bytes — SSE is text/event-stream.
                        match std::str::from_utf8(chunk.as_ref()) {
                            Ok(s) => buf.push_str(s),
                            Err(_) => continue,
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(ClientError::Http {
                                status: 0,
                                message: format!("stream: {e}"),
                            }),
                            (bytes, buf),
                        ));
                    }
                    None => return None,
                }
            }
        })
    }

    /// Pop the next complete `data:` payload from `buf`. Returns
    /// `(payload, remainder)`. An SSE event terminates at a blank line
    /// (`\n\n` or `\r\n\r\n`).
    fn pop_frame(buf: &str) -> Option<(String, String)> {
        let sep = buf
            .find("\n\n")
            .map(|i| (i, 2))
            .or_else(|| buf.find("\r\n\r\n").map(|i| (i, 4)))?;
        let (i, len) = sep;
        let head = &buf[..i];
        let rest = buf[i + len..].to_string();
        // Collect data: lines, joining with \n.
        let data: Vec<&str> = head
            .split('\n')
            .filter_map(|line| {
                let line = line.trim_end_matches('\r');
                line.strip_prefix("data:")
                    .map(|v| v.strip_prefix(' ').unwrap_or(v))
            })
            .collect();
        if data.is_empty() {
            // Non-data-bearing frame (event:/id:/retry:-only) — treat as pending.
            return Some((String::new(), rest));
        }
        Some((data.join("\n"), rest))
    }

    fn decode(frame: &str) -> Option<Result<AiStreamEvent, ClientError>> {
        if frame.is_empty() {
            return None;
        }
        match serde_json::from_str::<AiStreamEvent>(frame) {
            Ok(ev) => Some(Ok(ev)),
            // Unknown frames are ignored rather than killing the stream;
            // forward compatibility with server-side additions.
            Err(_) => None,
        }
    }
}
