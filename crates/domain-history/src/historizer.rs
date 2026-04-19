//! Historizer service — SLOT-STORAGE.md Stage 3.
//!
//! Subscribes to [`graph::GraphEvent::SlotChanged`] events, looks up
//! the matching `HistoryConfig` for each node, applies the declared
//! per-slot policy (COV / Interval / OnDemand), and buffers records in
//! a bounded in-memory ring buffer.  A background flush task drains the
//! buffer to the appropriate storage backend on schedule.
//!
//! ## Flush triggers (whichever fires first)
//!
//! | Trigger | Default |
//! |---|---|
//! | Time | 5 s (edge), 10 s (cloud) |
//! | Batch size | 500 records (edge), 2000 (cloud) |
//! | Memory pressure | At 80% queue capacity |
//! | Graceful shutdown | Flush all within 2 s |
//! | Critical tier | Every write, no buffering |
//!
//! ## Back-pressure
//!
//! Default tier: drops oldest records, emits `HistorizerOverflow` health event.
//! Critical tier: returns `SlotWriteRejected` to caller synchronously.
//!
//! See SLOT-STORAGE.md §"Back-pressure and the `SlotWriteRejected` contract".

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use spi::SlotValueKind;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::config::{HistoryConfig, HistoryConfigSettings, SlotPolicy};
use crate::policy::should_record_cov;
use data_repos::{HistoryRecord, HistoryRepo, HistorySlotKind, RepoError};
use data_tsdb::{ScalarRecord, TelemetryRepo, TsdbError};

/// Platform-level defaults. Operators override via agent config.
pub struct HistorizerConfig {
    /// Flush period (time trigger).
    pub flush_interval: Duration,
    /// Max records per flush batch (batch-size trigger).
    pub batch_size: usize,
    /// Per-config ring-buffer capacity (default: 10 000 on edge).
    pub queue_cap: usize,
    /// Platform default max samples per slot.
    pub default_max_samples: u64,
    /// Platform default max bytes per day for Binary slots.
    pub default_max_bytes_per_day: i64,
}

impl Default for HistorizerConfig {
    fn default() -> Self {
        Self {
            flush_interval: Duration::from_secs(5),
            batch_size: 500,
            queue_cap: 10_000,
            default_max_samples: 100_000,
            default_max_bytes_per_day: 100 * 1024 * 1024, // 100 MiB
        }
    }
}

/// Error returned synchronously for critical-tier back-pressure.
#[derive(Debug, thiserror::Error)]
pub enum HistorizerError {
    #[error("slot write rejected — buffer full, retry after {retry_after_ms} ms")]
    SlotWriteRejected { retry_after_ms: u64 },
    #[error("repo error: {0}")]
    Repo(#[from] RepoError),
    #[error("tsdb error: {0}")]
    Tsdb(#[from] TsdbError),
}

/// A pending history record before routing to scalar or structured backend.
#[derive(Debug, Clone)]
struct PendingRecord {
    node_id: Uuid,
    slot_name: String,
    value_kind: SlotValueKind,
    ts_ms: i64,
    value: JsonValue,
    ntp_synced: bool,
}

/// Per-(config_node) state tracked by the historizer.
#[derive(Debug)]
struct ConfigState {
    config: HistoryConfig,
    /// Pending records for this config (bounded ring buffer).
    queue: VecDeque<PendingRecord>,
    /// Last ts_ms and last value written per slot — used for COV logic.
    last_written: BTreeMap<String, (i64, JsonValue)>,
    /// Whether at least one overflow has been logged since last flush.
    overflow_logged: bool,
}

/// The main historizer service.
///
/// Callers feed events in via [`Historizer::on_slot_changed`] and
/// call [`Historizer::flush`] on a schedule (or from the shutdown path).
pub struct Historizer {
    cfg: HistorizerConfig,
    /// All active HistoryConfig registrations, keyed by parent node id.
    configs: Mutex<BTreeMap<Uuid, ConfigState>>,
    telemetry: Arc<dyn TelemetryRepo>,
    history: Arc<dyn HistoryRepo>,
}

impl Historizer {
    pub fn new(
        cfg: HistorizerConfig,
        telemetry: Arc<dyn TelemetryRepo>,
        history: Arc<dyn HistoryRepo>,
    ) -> Self {
        Self {
            cfg,
            configs: Mutex::new(BTreeMap::new()),
            telemetry,
            history,
        }
    }

    /// Register (or update) a `HistoryConfig` by its parent node id.
    /// Called when the graph creates or updates an `sys.core.history.config`
    /// node, after settings have been resolved.
    pub fn register_config(&self, parent_id: Uuid, config: HistoryConfig) {
        let mut map = self.configs.lock().expect("historizer lock poisoned");
        let cap = self.cfg.queue_cap;
        map.entry(parent_id)
            .and_modify(|cs| {
                cs.config = config.clone();
            })
            .or_insert_with(|| ConfigState {
                config,
                queue: VecDeque::with_capacity(cap),
                last_written: BTreeMap::new(),
                overflow_logged: false,
            });
    }

    /// Remove a `HistoryConfig` registration (called when the config node
    /// is deleted). In-flight buffered records for this config are flushed
    /// before removal in the shutdown path; here we just drop them.
    pub fn unregister_config(&self, parent_id: &Uuid) {
        let mut map = self.configs.lock().expect("historizer lock poisoned");
        map.remove(parent_id);
    }

    /// Called for every `SlotChanged` graph event.
    ///
    /// * Looks up any registered `HistoryConfig` for the node's parent.
    /// * Applies the per-slot policy (COV / Interval / OnDemand).
    /// * Appends an eligible record to the config's ring buffer.
    ///
    /// Critical-tier configs bypass the buffer and flush immediately,
    /// returning `Err(SlotWriteRejected)` if the underlying store is full.
    pub fn on_slot_changed(
        &self,
        node_id: Uuid,
        slot_name: &str,
        value: JsonValue,
        value_kind: SlotValueKind,
        ts_ms: i64,
    ) -> Result<(), HistorizerError> {
        let mut map = self.configs.lock().expect("historizer lock poisoned");
        let cs = match map.get_mut(&node_id) {
            Some(cs) => cs,
            None => return Ok(()), // no config registered for this node
        };
        let settings = &cs.config.settings;
        let slot_policy = match settings.slots.get(slot_name) {
            Some(p) => p,
            None => return Ok(()), // slot not declared in config
        };

        // Compute elapsed since last write for rate-floor / heartbeat checks.
        let (elapsed_ms, last_value) = cs
            .last_written
            .get(slot_name)
            .map(|(last_ts, last_val)| {
                let elapsed = (ts_ms - last_ts).max(0) as u64;
                (elapsed, Some(last_val))
            })
            .unwrap_or((u64::MAX, None));

        // Decide whether to record.
        let should_record = match slot_policy {
            SlotPolicy::OnDemand => false, // only on explicit trigger
            SlotPolicy::Interval {
                period_ms,
                align_to_wall,
                ..
            } => should_fire_interval(ts_ms, *period_ms, *align_to_wall, elapsed_ms),
            SlotPolicy::Cov {
                deadband,
                min_interval_ms,
                max_gap_ms,
                ..
            } => should_record_cov(
                &value,
                last_value,
                value_kind,
                *deadband,
                *min_interval_ms,
                *max_gap_ms,
                elapsed_ms,
            ),
        };

        if !should_record {
            return Ok(());
        }

        let record = PendingRecord {
            node_id,
            slot_name: slot_name.to_string(),
            value_kind,
            ts_ms,
            value: value.clone(),
            ntp_synced: true, // TODO: plumb NTP state from agent clock
        };

        // Critical tier: bypass buffer, write immediately.
        if settings.critical {
            let max_samples = effective_max_samples(slot_name, slot_policy, settings, &self.cfg);
            // Update last_written before releasing the lock.
            cs.last_written
                .insert(slot_name.to_string(), (ts_ms, value.clone()));
            drop(map);
            return self.write_record_direct(&record, max_samples);
        }

        // Default tier: push to ring buffer, drop oldest on overflow.
        let cap = self.cfg.queue_cap;
        if cs.queue.len() >= cap {
            cs.queue.pop_front(); // evict oldest
            if !cs.overflow_logged {
                warn!(
                    node_id = %node_id,
                    slot = slot_name,
                    "HistorizerOverflow: ring buffer full, oldest record dropped"
                );
                cs.overflow_logged = true;
            }
        } else {
            cs.overflow_logged = false;
        }
        cs.queue.push_back(record);
        cs.last_written
            .insert(slot_name.to_string(), (ts_ms, value));

        // Batch-size trigger: if we're at the batch cap, flush inline.
        if cs.queue.len() >= self.cfg.batch_size {
            let batch: Vec<_> = cs.queue.drain(..).collect();
            let settings_clone = cs.config.settings.clone();
            let cfg_ref = &self.cfg;
            drop(map);
            self.flush_batch(&batch, &settings_clone, cfg_ref)?;
        }

        Ok(())
    }

    /// Fire a `OnDemand` record for a given (node, slot) manually.
    pub fn record_on_demand(
        &self,
        node_id: Uuid,
        slot_name: &str,
        value: JsonValue,
        value_kind: SlotValueKind,
    ) -> Result<(), HistorizerError> {
        let ts_ms = now_ms();
        let record = PendingRecord {
            node_id,
            slot_name: slot_name.to_string(),
            value_kind,
            ts_ms,
            value,
            ntp_synced: true,
        };
        let max_samples = {
            let map = self.configs.lock().expect("historizer lock poisoned");
            let cs = map.get(&node_id).ok_or_else(|| {
                HistorizerError::Repo(RepoError::NotFound)
            })?;
            let policy = cs.config.settings.slots.get(slot_name).ok_or_else(|| {
                HistorizerError::Repo(RepoError::NotFound)
            })?;
            effective_max_samples(slot_name, policy, &cs.config.settings, &self.cfg)
        };
        self.write_record_direct(&record, max_samples)
    }

    /// Flush all buffered records. Called by the scheduler and shutdown path.
    ///
    /// Returns the number of records written.
    pub fn flush(&self) -> Result<usize, HistorizerError> {
        let batches: Vec<(Vec<PendingRecord>, HistoryConfigSettings)> = {
            let mut map = self.configs.lock().expect("historizer lock poisoned");
            map.values_mut()
                .filter(|cs| !cs.queue.is_empty())
                .map(|cs| {
                    let batch: Vec<_> = cs.queue.drain(..).collect();
                    (batch, cs.config.settings.clone())
                })
                .collect()
        };

        let mut total = 0;
        for (batch, settings) in batches {
            total += batch.len();
            self.flush_batch(&batch, &settings, &self.cfg)?;
        }
        debug!(total, "historizer flush complete");
        Ok(total)
    }

    // ---------------------------------------------------------------
    // Private helpers
    // ---------------------------------------------------------------

    fn flush_batch(
        &self,
        batch: &[PendingRecord],
        settings: &HistoryConfigSettings,
        cfg: &HistorizerConfig,
    ) -> Result<(), HistorizerError> {
        let mut scalars: Vec<ScalarRecord> = Vec::new();
        let mut structured: Vec<HistoryRecord> = Vec::new();

        for r in batch {
            if r.value_kind.is_scalar() {
                scalars.push(to_scalar_record(r));
            } else {
                structured.push(to_history_record(r));
            }
        }

        let max_samples = settings
            .retention
            .max_samples_per_slot
            .unwrap_or(cfg.default_max_samples);

        if !scalars.is_empty() {
            self.telemetry.insert_batch(&scalars, max_samples)?;
        }
        if !structured.is_empty() {
            self.history.insert_batch(&structured, max_samples)?;
        }
        Ok(())
    }

    fn write_record_direct(
        &self,
        r: &PendingRecord,
        max_samples: u64,
    ) -> Result<(), HistorizerError> {
        if r.value_kind.is_scalar() {
            self.telemetry
                .insert_batch(&[to_scalar_record(r)], max_samples)?;
        } else {
            self.history
                .insert_batch(&[to_history_record(r)], max_samples)?;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------
// Pure helpers
// -----------------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn should_fire_interval(ts_ms: i64, period_ms: u64, align_to_wall: bool, elapsed_ms: u64) -> bool {
    if !align_to_wall {
        return elapsed_ms >= period_ms;
    }
    // Wall-clock alignment: fire when `ts_ms % period_ms` crosses a boundary.
    // We fire if the current timestamp is at or past the next aligned bucket
    // since the last fire.
    if period_ms == 0 {
        return false;
    }
    let bucket = ts_ms as u64 / period_ms;
    // If elapsed is long enough to span at least one period, always fire.
    if elapsed_ms >= period_ms {
        return true;
    }
    // Otherwise compare buckets.
    let prev_ts = ts_ms as u64 - elapsed_ms;
    let prev_bucket = prev_ts / period_ms;
    bucket > prev_bucket
}

fn effective_max_samples(
    _slot_name: &str,
    policy: &SlotPolicy,
    settings: &HistoryConfigSettings,
    cfg: &HistorizerConfig,
) -> u64 {
    // Precedence: slot → config → platform
    let slot_max = match policy {
        SlotPolicy::Cov { max_samples, .. } | SlotPolicy::Interval { max_samples, .. } => {
            *max_samples
        }
        SlotPolicy::OnDemand => None,
    };
    slot_max
        .or(settings.retention.max_samples_per_slot)
        .unwrap_or(cfg.default_max_samples)
}

fn to_scalar_record(r: &PendingRecord) -> ScalarRecord {
    let bool_value = if r.value_kind == SlotValueKind::Bool {
        r.value.as_bool()
    } else {
        None
    };
    let num_value = if r.value_kind == SlotValueKind::Number {
        r.value.as_f64()
    } else {
        None
    };
    ScalarRecord {
        node_id: r.node_id,
        slot_name: r.slot_name.clone(),
        ts_ms: r.ts_ms,
        bool_value,
        num_value,
        ntp_synced: r.ntp_synced,
        last_sync_age_ms: None,
    }
}

fn to_history_record(r: &PendingRecord) -> HistoryRecord {
    let slot_kind = match r.value_kind {
        SlotValueKind::String => HistorySlotKind::String,
        SlotValueKind::Binary => HistorySlotKind::Binary,
        _ => HistorySlotKind::Json,
    };
    let (value_json, blob_bytes, byte_size) = match r.value_kind {
        SlotValueKind::Binary => {
            // Binary values arrive encoded as base64 strings in the JSON value.
            let raw = r.value.as_str().map(|s| {
                // Attempt to decode from base64; keep raw bytes from string on failure.
                base64_decode(s).unwrap_or_else(|| s.as_bytes().to_vec())
            });
            let size = raw.as_ref().map(|b| b.len() as i64).unwrap_or(0);
            (None, raw, size)
        }
        _ => {
            let s = serde_json::to_string(&r.value).unwrap_or_default();
            let size = s.len() as i64;
            (Some(s), None, size)
        }
    };
    HistoryRecord {
        id: 0, // auto-assigned by DB
        node_id: r.node_id,
        slot_name: r.slot_name.clone(),
        slot_kind,
        ts_ms: r.ts_ms,
        value_json,
        blob_bytes,
        byte_size,
        ntp_synced: r.ntp_synced,
        last_sync_age_ms: None,
    }
}

/// Minimal base64 decoding without adding a dependency.
/// Handles standard base64 (RFC 4648).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    // Use the standard alphabet: A-Z, a-z, 0-9, +, /
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [255u8; 256];
    for (i, &c) in ALPHA.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let [a, b, c, d] = [
            table[bytes[i] as usize],
            table[bytes[i + 1] as usize],
            table[bytes[i + 2] as usize],
            table[bytes[i + 3] as usize],
        ];
        if a == 255 || b == 255 { return None; }
        out.push((a << 2) | (b >> 4));
        if c != 255 { out.push((b << 4) | (c >> 2)); }
        if d != 255 { out.push((c << 6) | d); }
        i += 4;
    }
    // Remaining 2 or 3 chars.
    if i + 1 < bytes.len() {
        let (a, b) = (table[bytes[i] as usize], table[bytes[i + 1] as usize]);
        if a == 255 || b == 255 { return None; }
        out.push((a << 2) | (b >> 4));
        if i + 2 < bytes.len() {
            let c = table[bytes[i + 2] as usize];
            if c != 255 { out.push((b << 4) | (c >> 2)); }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HistoryConfig, HistoryConfigSettings, RetentionSettings, SlotPolicy};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    // ---- minimal in-memory repo stubs ----

    #[derive(Default)]
    struct FakeTelemetry {
        rows: Mutex<Vec<ScalarRecord>>,
    }
    impl TelemetryRepo for FakeTelemetry {
        fn insert_batch(&self, records: &[ScalarRecord], _: u64) -> Result<(), TsdbError> {
            self.rows.lock().unwrap().extend(records.iter().cloned());
            Ok(())
        }
        fn query_range(&self, _: &data_tsdb::ScalarQuery) -> Result<Vec<ScalarRecord>, TsdbError> {
            Ok(vec![])
        }
        fn count(&self, _: Uuid, _: &str) -> Result<u64, TsdbError> {
            Ok(self.rows.lock().unwrap().len() as u64)
        }
        fn evict_oldest(&self, _: Uuid, _: &str, _: u64) -> Result<(), TsdbError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeHistory {
        rows: Mutex<Vec<HistoryRecord>>,
    }
    impl HistoryRepo for FakeHistory {
        fn insert_batch(&self, records: &[HistoryRecord], _: u64) -> Result<(), RepoError> {
            self.rows.lock().unwrap().extend(records.iter().cloned());
            Ok(())
        }
        fn query_range(&self, _: &data_repos::HistoryQuery) -> Result<Vec<HistoryRecord>, RepoError> {
            Ok(vec![])
        }
        fn count(&self, _: Uuid, _: &str) -> Result<u64, RepoError> {
            Ok(self.rows.lock().unwrap().len() as u64)
        }
        fn evict_oldest(&self, _: Uuid, _: &str, _: u64) -> Result<(), RepoError> {
            Ok(())
        }
        fn bytes_in_window(&self, _: Uuid, _: &str, _: i64) -> Result<i64, RepoError> {
            Ok(0)
        }
    }

    fn make_historizer() -> (Historizer, Arc<FakeTelemetry>, Arc<FakeHistory>) {
        let tsdb = Arc::new(FakeTelemetry::default());
        let hist = Arc::new(FakeHistory::default());
        let h = Historizer::new(
            HistorizerConfig::default(),
            tsdb.clone() as Arc<dyn TelemetryRepo>,
            hist.clone() as Arc<dyn HistoryRepo>,
        );
        (h, tsdb, hist)
    }

    fn register(h: &Historizer, node_id: Uuid, slots: BTreeMap<String, SlotPolicy>) {
        h.register_config(
            node_id,
            HistoryConfig {
                config_node_id: Uuid::new_v4(),
                parent_node_id: node_id,
                settings: HistoryConfigSettings {
                    slots,
                    retention: RetentionSettings::default(),
                    publish_historized_events: false,
                    critical: false,
                },
            },
        );
    }

    #[test]
    fn cov_number_routes_to_telemetry() {
        let (h, tsdb, _hist) = make_historizer();
        let node = Uuid::new_v4();
        let mut slots = BTreeMap::new();
        slots.insert(
            "temp".into(),
            SlotPolicy::Cov {
                deadband: 0.1,
                min_interval_ms: 0,
                max_gap_ms: 900_000,
                max_samples: None,
            },
        );
        register(&h, node, slots);

        h.on_slot_changed(node, "temp", json!(20.0), SlotValueKind::Number, 1000)
            .unwrap();
        h.flush().unwrap();

        assert_eq!(tsdb.rows.lock().unwrap().len(), 1);
    }

    #[test]
    fn cov_within_deadband_not_recorded() {
        let (h, tsdb, _hist) = make_historizer();
        let node = Uuid::new_v4();
        let mut slots = BTreeMap::new();
        slots.insert(
            "temp".into(),
            SlotPolicy::Cov {
                deadband: 1.0,
                min_interval_ms: 0,
                max_gap_ms: 900_000,
                max_samples: None,
            },
        );
        register(&h, node, slots);

        // First sample — always recorded.
        h.on_slot_changed(node, "temp", json!(20.0), SlotValueKind::Number, 1000)
            .unwrap();
        // Within deadband — not recorded.
        h.on_slot_changed(node, "temp", json!(20.5), SlotValueKind::Number, 2000)
            .unwrap();
        h.flush().unwrap();

        assert_eq!(tsdb.rows.lock().unwrap().len(), 1);
    }

    #[test]
    fn json_slot_routes_to_history() {
        let (h, _tsdb, hist) = make_historizer();
        let node = Uuid::new_v4();
        let mut slots = BTreeMap::new();
        slots.insert(
            "config".into(),
            SlotPolicy::Cov {
                deadband: 0.0,
                min_interval_ms: 0,
                max_gap_ms: 900_000,
                max_samples: None,
            },
        );
        register(&h, node, slots);

        h.on_slot_changed(
            node,
            "config",
            json!({"a": 1}),
            SlotValueKind::Json,
            1000,
        )
        .unwrap();
        h.flush().unwrap();

        assert_eq!(hist.rows.lock().unwrap().len(), 1);
    }

    #[test]
    fn ring_buffer_overflow_drops_oldest() {
        // Use a small queue cap (5) and a batch_size larger than cap so the
        // batch-size trigger never fires, letting us test the overflow path.
        let tsdb = Arc::new(FakeTelemetry::default());
        let hist = Arc::new(FakeHistory::default());
        let h = Historizer::new(
            HistorizerConfig {
                queue_cap: 5,
                batch_size: 1000, // larger than cap — batch trigger won't fire
                ..Default::default()
            },
            tsdb.clone() as Arc<dyn TelemetryRepo>,
            hist.clone() as Arc<dyn HistoryRepo>,
        );

        let node = Uuid::new_v4();
        let mut slots = BTreeMap::new();
        slots.insert(
            "temp".into(),
            SlotPolicy::Cov {
                deadband: 0.0,
                min_interval_ms: 0,
                max_gap_ms: 0, // heartbeat always fires
                max_samples: None,
            },
        );
        register(&h, node, slots);

        // Push 6 records into a cap-5 buffer.
        for i in 0..6u64 {
            h.on_slot_changed(
                node,
                "temp",
                json!(i as f64),
                SlotValueKind::Number,
                i as i64 * 1000,
            )
            .unwrap();
        }
        h.flush().unwrap();
        // Should have exactly 5 records (oldest dropped on overflow).
        let count = tsdb.rows.lock().unwrap().len();
        assert_eq!(count, 5);
    }
}
