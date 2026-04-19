//! Slot writes — `POST /api/v1/slots`.

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::error::ClientError;
use crate::http::HttpClient;
use crate::types::WriteSlotResponse;

#[derive(Serialize)]
struct WriteSlotReq<'a> {
    path: &'a str,
    slot: &'a str,
    value: &'a JsonValue,
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
                &WriteSlotReq { path, slot, value },
            )
            .await?;
        Ok(resp.generation)
    }
}
