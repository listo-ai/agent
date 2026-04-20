//! Slot operations — `POST /api/v1/slots`, `/api/v1/history`, `/api/v1/telemetry`.

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::{HistoryResponse, RecordedResponse, TelemetryResponse, WriteSlotResponse};

#[derive(Serialize)]
struct WriteSlotReq<'a> {
    path: &'a str,
    slot: &'a str,
    value: &'a JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_generation: Option<u64>,
}

#[derive(Serialize)]
struct RecordReq<'a> {
    path: &'a str,
    slot: &'a str,
}

fn parse_generation_mismatch(raw: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.get("code").and_then(|c| c.as_str()) != Some("generation_mismatch") {
        return None;
    }
    v.get("current_generation").and_then(|n| n.as_u64())
}

pub struct Slots<'c> {
    http: &'c HttpClient,
    base: String,
}

impl<'c> Slots<'c> {
    pub(crate) fn new(http: &'c HttpClient, api_version: u32) -> Self {
        Self {
            http,
            base: format!("/api/v{api_version}"),
        }
    }

    /// Write a slot value. Returns the new generation number.
    pub async fn write(
        &self,
        path: &str,
        slot: &str,
        value: &JsonValue,
    ) -> Result<u64, ClientError> {
        let resp: WriteSlotResponse = self
            .http
            .post(
                &format!("{}/slots", self.base),
                &WriteSlotReq {
                    path,
                    slot,
                    value,
                    expected_generation: None,
                },
            )
            .await?;
        Ok(resp.generation)
    }

    /// Write a slot value with an OCC guard. The server compares
    /// `expected` against the slot's current generation and returns
    /// [`ClientError::GenerationMismatch`] on conflict.
    pub async fn write_with_generation(
        &self,
        path: &str,
        slot: &str,
        value: &JsonValue,
        expected: u64,
    ) -> Result<u64, ClientError> {
        let req = WriteSlotReq {
            path,
            slot,
            value,
            expected_generation: Some(expected),
        };
        match self
            .http
            .post::<WriteSlotResponse, _>(&format!("{}/slots", self.base), &req)
            .await
        {
            Ok(resp) => Ok(resp.generation),
            Err(ClientError::Http {
                status: 409,
                message,
            }) => {
                // Server sent `{"code":"generation_mismatch","current_generation":N}`;
                // the HTTP layer stringifies anything without an `error` key into
                // `message`. Parse it back to the typed error.
                if let Some(current) = parse_generation_mismatch(&message) {
                    Err(ClientError::GenerationMismatch { current })
                } else {
                    Err(ClientError::Http {
                        status: 409,
                        message,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Query structured history (String / Json / Binary slots).
    ///
    /// `from_ms` and `to_ms` are Unix timestamps in milliseconds;
    /// pass `None` for the defaults (0 and now, respectively).
    pub async fn history_range(
        &self,
        path: &str,
        slot: &str,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
        limit: Option<u32>,
    ) -> Result<HistoryResponse, ClientError> {
        let mut url = format!("{}/history?path={}&slot={}", self.base, path, slot);
        if let Some(f) = from_ms {
            url.push_str(&format!("&from={f}"));
        }
        if let Some(t) = to_ms {
            url.push_str(&format!("&to={t}"));
        }
        if let Some(l) = limit {
            url.push_str(&format!("&limit={l}"));
        }
        self.http.get(&url).await
    }

    /// Query scalar telemetry (Bool / Number slots).
    pub async fn telemetry_range(
        &self,
        path: &str,
        slot: &str,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
        limit: Option<u32>,
    ) -> Result<TelemetryResponse, ClientError> {
        let mut url = format!("{}/telemetry?path={}&slot={}", self.base, path, slot);
        if let Some(f) = from_ms {
            url.push_str(&format!("&from={f}"));
        }
        if let Some(t) = to_ms {
            url.push_str(&format!("&to={t}"));
        }
        if let Some(l) = limit {
            url.push_str(&format!("&limit={l}"));
        }
        self.http.get(&url).await
    }

    /// On-demand record of the slot's current live value.
    ///
    /// Routes to the telemetry store for Bool/Number, structured history
    /// store for String/Json. Returns the kind that was recorded.
    pub async fn record(&self, path: &str, slot: &str) -> Result<RecordedResponse, ClientError> {
        self.http
            .post(
                &format!("{}/history/record", self.base),
                &RecordReq { path, slot },
            )
            .await
    }
}
