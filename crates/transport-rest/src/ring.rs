//! Bounded event ring buffer.
//!
//! The agent retains the last `capacity` sequenced events so that
//! reconnecting SSE clients can replay what they missed.  When the
//! ring is full the oldest event is evicted (FIFO).
//!
//! `since(seq)` covers two cases:
//!
//! * **Cursor in range** — returns every event with `seq > since`,
//!   or an empty Vec if the client is already up-to-date.
//! * **Cursor too old** — returns `Err(available_from)` when the gap
//!   between `since+1` and the ring's oldest retained event means we
//!   cannot guarantee a gapless replay.  The HTTP handler converts
//!   this to `409 Conflict { error: "cursor_too_old", available_from }`.
//!
//! All operations are O(n) in the worst case on `VecDeque`.  This is
//! acceptable: N is bounded (default 1024) and the replay path is cold
//! (reconnects only).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::event::SequencedEvent;

/// Default ring capacity if not overridden by config.
pub const DEFAULT_RING_CAPACITY: usize = 1024;

#[derive(Clone, Debug)]
pub struct EventRing {
    inner: Arc<Mutex<RingInner>>,
    capacity: usize,
}

#[derive(Debug)]
struct RingInner {
    events: VecDeque<SequencedEvent>,
}

impl EventRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RingInner {
                events: VecDeque::with_capacity(capacity.min(4096)),
            })),
            capacity,
        }
    }

    /// Push a new event into the ring.  Evicts the oldest if at capacity.
    pub fn push(&self, event: SequencedEvent) {
        let mut inner = self.inner.lock().expect("ring lock poisoned");
        if inner.events.len() >= self.capacity {
            inner.events.pop_front();
        }
        inner.events.push_back(event);
    }

    /// The seq of the most-recently pushed event (0 when the ring is empty).
    pub fn current_seq(&self) -> u64 {
        self.inner
            .lock()
            .expect("ring lock poisoned")
            .events
            .back()
            .map(|e| e.seq)
            .unwrap_or(0)
    }

    /// The lowest seq retained in the ring (0 when empty).
    pub fn available_from(&self) -> u64 {
        self.inner
            .lock()
            .expect("ring lock poisoned")
            .events
            .front()
            .map(|e| e.seq)
            .unwrap_or(0)
    }

    /// Replay events with `seq > since`.
    ///
    /// Returns `Ok(events)` when the replay is complete, even if
    /// `events` is empty (client is already up-to-date).
    ///
    /// Returns `Err(available_from)` when `since + 1 < min_seq` —
    /// there is a gap and the client must do a full refetch.
    pub fn since(&self, since: u64) -> Result<Vec<SequencedEvent>, u64> {
        let inner = self.inner.lock().expect("ring lock poisoned");
        if inner.events.is_empty() {
            return Ok(Vec::new());
        }
        let min_seq = inner.events.front().unwrap().seq;
        // Gap check: if the cursor's next expected event (since+1) is
        // below the ring floor, events were evicted and we can't replay.
        if since + 1 < min_seq {
            return Err(min_seq);
        }
        let events = inner
            .events
            .iter()
            .filter(|e| e.seq > since)
            .cloned()
            .collect();
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use graph::GraphEvent;
    use spi::{KindId, NodeId, NodePath};

    use super::*;
    use crate::event::SequencedEvent;

    fn make_event(seq: u64) -> SequencedEvent {
        SequencedEvent {
            seq,
            ts: seq * 1000,
            event: GraphEvent::NodeCreated {
                id: NodeId(uuid::Uuid::nil()),
                kind: KindId::new("test"),
                path: NodePath::root(),
            },
        }
    }

    #[test]
    fn empty_ring_returns_empty_ok() {
        let ring = EventRing::new(4);
        assert!(ring.since(0).unwrap().is_empty());
        assert_eq!(ring.current_seq(), 0);
        assert_eq!(ring.available_from(), 0);
    }

    #[test]
    fn replays_events_since_cursor() {
        let ring = EventRing::new(8);
        for i in 1..=5 {
            ring.push(make_event(i));
        }
        let replayed = ring.since(2).unwrap();
        let seqs: Vec<u64> = replayed.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn up_to_date_cursor_returns_empty() {
        let ring = EventRing::new(8);
        for i in 1..=3 {
            ring.push(make_event(i));
        }
        assert!(ring.since(3).unwrap().is_empty());
    }

    #[test]
    fn cursor_too_old_returns_available_from() {
        let ring = EventRing::new(4);
        // Push 5 events into a capacity-4 ring → seq 1 is evicted.
        for i in 1..=5 {
            ring.push(make_event(i));
        }
        // Ring now holds [2, 3, 4, 5].  Cursor 0 → gap (1 was evicted).
        match ring.since(0) {
            Ok(_) => panic!("expected Err"),
            Err(available_from) => assert_eq!(available_from, 2),
        }
    }

    #[test]
    fn exactly_at_floor_is_ok() {
        let ring = EventRing::new(4);
        for i in 1..=5 {
            ring.push(make_event(i));
        }
        // Ring holds [2, 3, 4, 5].  since(1) means "give me seq > 1"
        // → seq 1+1=2 = min_seq, so NOT a gap.
        let replayed = ring.since(1).unwrap();
        let seqs: Vec<u64> = replayed.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![2, 3, 4, 5]);
    }

    #[test]
    fn monotonic_seq_and_current_seq() {
        let ring = EventRing::new(8);
        for i in 1..=10 {
            ring.push(make_event(i));
        }
        assert_eq!(ring.current_seq(), 10);
        assert_eq!(ring.available_from(), 3); // capacity 8, so [3..10]
    }
}
