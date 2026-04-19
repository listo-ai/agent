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
}

#[derive(Serialize)]
struct RecordReq<'a> {
    path: &'a str,
    slot: &'a str,
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
    pub async fn record(
        &self,
        path: &str,
        slot: &str,
    ) -> Result<RecordedResponse, ClientError> {
        self.http
            .post(
                &format!("{}/history/record", self.base),
                &RecordReq { path, slot },
            )
            .await
    }
}
