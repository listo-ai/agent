//! Sequenced event envelope — wraps [`graph::GraphEvent`] with a
//! monotonic `seq` counter and a millisecond-precision `ts` timestamp.
//!
//! `seq` is the reconnect anchor: clients store their last-seen seq and
//! reconnect with `GET /api/v1/events?since=<seq>` to replay any events
//! missed during a network interruption (Stage 1c).
//!
//! `ts` is wall-clock time at emission — used by Studio for "changed 3 s
//! ago" indicators and as an audit anchor.  Stored as milliseconds since
//! Unix epoch to avoid pulling in `chrono`; the frontend converts with
//! `new Date(ts)`.

use graph::GraphEvent;
use serde::{Deserialize, Serialize};

/// A [`GraphEvent`] annotated with a monotonic sequence number and a
/// wall-clock timestamp.  This is the wire shape sent over SSE and
/// stored in the ring buffer.  `seq` and `ts` appear at the top level
/// alongside the event's own fields thanks to `#[serde(flatten)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedEvent {
    /// Monotonic counter, incremented once per event emitted by this
    /// agent instance.  Resets to 1 on agent restart.
    pub seq: u64,
    /// Wall-clock time of emission in milliseconds since Unix epoch.
    pub ts: u64,
    #[serde(flatten)]
    pub event: GraphEvent,
}

/// The initial `hello` frame sent on every new SSE connection.
/// Carries the current sequence number so the client knows where
/// "now" is without having consumed any events yet.
#[derive(Debug, Clone, Serialize)]
pub struct HelloFrame {
    pub event: &'static str,
    pub seq: u64,
}

impl HelloFrame {
    pub fn new(seq: u64) -> Self {
        Self { event: "hello", seq }
    }
}
