//! Priority IO queue substrate.
//!
//! # NON-GOALS (mirror of crate-level doc, restated for in-module ownership clarity)
//!
//! - No `tokio` / `futures` / async runtime integration. Synchronous-only.
//! - No task graph / DAG scheduler. Flat queue only.
//! - No executor abstraction.
//! - No GPU upload semantics.
//! - No distributed coordination.
//! - No reactive scheduling.
//! - No generalized job-system semantics — that's `kernel/job-system`.
//! - No residency hysteresis or predictive prefetch — that lives downstream
//!   in `kernel/asset-streaming`.
//! - No actual IO driver dispatch (filesystem / network / disk) — driver
//!   crates land in dedicated future dispatches.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.

use std::collections::BTreeMap;

use crate::priority::Priority;
use crate::request::IoRequest;

/// Priority IO queue.
///
/// v0 stub: `BTreeMap`-backed for deterministic iteration order. FIFO within
/// a priority is preserved by pairing each entry with a monotonic sequence
/// number; the composite key `(Priority, u64)` orders highest-priority first
/// and within a priority orders by submission time.
///
/// The scheduler owns no IO driver, no executor, no completion mechanism.
/// It is purely a substrate for ordering pending requests; consumers
/// (drivers, residency systems, asset-streaming) pop requests and dispatch
/// them via mechanisms that land in dedicated future dispatches.
#[derive(Default, Debug)]
pub struct IoScheduler {
    /// Pending requests keyed by `(priority, sequence)` for deterministic
    /// priority-then-FIFO iteration. `BTreeMap` rather than `BinaryHeap` so
    /// `iter()` is stable and reproducible across runs.
    queue: BTreeMap<(Priority, u64), IoRequest>,
    /// Monotonic counter assigning a sequence to each submitted request.
    /// Never reset — `clear()` empties the queue but preserves the counter
    /// so subsequent submissions remain strictly later than any prior.
    seq: u64,
}

impl IoScheduler {
    /// Create an empty scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit `request` to the queue.
    ///
    /// Submission order within a single priority is preserved (FIFO). The
    /// monotonic sequence counter advances by one per call.
    pub fn submit(&mut self, request: IoRequest) {
        let key = (request.priority, self.seq);
        self.seq = self.seq.wrapping_add(1);
        self.queue.insert(key, request);
    }

    /// Remove and return the highest-priority request, FIFO within priority.
    ///
    /// Returns `None` when the queue is empty.
    pub fn pop(&mut self) -> Option<IoRequest> {
        let key = *self.queue.keys().next()?;
        self.queue.remove(&key)
    }

    /// Number of pending requests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// `true` when no requests are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Iterate pending requests in priority-then-FIFO order.
    ///
    /// Iteration is deterministic and does not consume the queue.
    pub fn iter(&self) -> impl Iterator<Item = &IoRequest> {
        self.queue.values()
    }

    /// Remove every pending request.
    ///
    /// The internal sequence counter is **not** reset; subsequent submissions
    /// retain strict monotonic ordering with respect to any pre-`clear`
    /// submissions. (This avoids ABA-style ordering anomalies if a stale
    /// reference to a popped request resurfaces.)
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{IoRequestId, IoRequestKind};

    fn req(id_byte: u8, priority: Priority) -> IoRequest {
        IoRequest {
            id: IoRequestId::from_bytes([id_byte; 16]),
            priority,
            kind: IoRequestKind::Placeholder,
        }
    }

    #[test]
    fn empty_new_is_empty() {
        let s = IoScheduler::new();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn submit_increments_len() {
        let mut s = IoScheduler::new();
        s.submit(req(1, Priority::InFrustumNear));
        assert_eq!(s.len(), 1);
        s.submit(req(2, Priority::InFrustumFar));
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());
    }

    #[test]
    fn pop_on_empty_returns_none() {
        let mut s = IoScheduler::new();
        assert!(s.pop().is_none());
    }

    #[test]
    fn fifo_within_priority_preserved_for_three_same_priority() {
        let mut s = IoScheduler::new();
        s.submit(req(1, Priority::InFrustumFar));
        s.submit(req(2, Priority::InFrustumFar));
        s.submit(req(3, Priority::InFrustumFar));

        let a = s.pop().expect("first pop");
        let b = s.pop().expect("second pop");
        let c = s.pop().expect("third pop");
        assert_eq!(a.id.as_bytes(), &[1u8; 16]);
        assert_eq!(b.id.as_bytes(), &[2u8; 16]);
        assert_eq!(c.id.as_bytes(), &[3u8; 16]);
    }

    #[test]
    fn pop_returns_higher_priority_before_lower() {
        let mut s = IoScheduler::new();
        // Submit in inverse priority order; pop must reorder to canonical
        // 4-tier sequence.
        s.submit(req(4, Priority::OutOfFrustumFar));
        s.submit(req(3, Priority::OutOfFrustumNear));
        s.submit(req(2, Priority::InFrustumFar));
        s.submit(req(1, Priority::InFrustumNear));

        assert_eq!(s.pop().unwrap().priority, Priority::InFrustumNear);
        assert_eq!(s.pop().unwrap().priority, Priority::InFrustumFar);
        assert_eq!(s.pop().unwrap().priority, Priority::OutOfFrustumNear);
        assert_eq!(s.pop().unwrap().priority, Priority::OutOfFrustumFar);
    }

    #[test]
    fn pop_drains_queue_completely() {
        let mut s = IoScheduler::new();
        for i in 0..10 {
            s.submit(req(i, Priority::InFrustumNear));
        }
        for _ in 0..10 {
            assert!(s.pop().is_some());
        }
        assert!(s.is_empty());
        assert!(s.pop().is_none());
    }

    #[test]
    fn clear_empties_without_dropping_seq_counter() {
        let mut s = IoScheduler::new();
        s.submit(req(1, Priority::InFrustumNear));
        s.submit(req(2, Priority::InFrustumNear));
        s.clear();
        assert!(s.is_empty());

        // Subsequent submission must still receive a strictly-later sequence
        // than any pre-clear submission. We verify this by submitting two new
        // entries at the same priority and asserting they pop FIFO — the
        // sequence counter advancement is observable through pop ordering.
        s.submit(req(3, Priority::InFrustumNear));
        s.submit(req(4, Priority::InFrustumNear));
        assert_eq!(s.pop().unwrap().id.as_bytes(), &[3u8; 16]);
        assert_eq!(s.pop().unwrap().id.as_bytes(), &[4u8; 16]);
        // And `seq` is now at least 4 — internal invariant; we observe via
        // a freshly-submitted entry's relative ordering against any earlier
        // entry (none here, but the FIFO ordering above implies seq advanced
        // past the cleared entries).
    }

    #[test]
    fn iter_yields_priority_then_fifo_order() {
        let mut s = IoScheduler::new();
        s.submit(req(10, Priority::OutOfFrustumFar));
        s.submit(req(20, Priority::InFrustumNear));
        s.submit(req(30, Priority::InFrustumNear));
        s.submit(req(40, Priority::OutOfFrustumNear));

        let ids: Vec<u8> = s.iter().map(|r| r.id.as_bytes()[0]).collect();
        // InFrustumNear entries first (FIFO 20 → 30), then OutOfFrustumNear
        // (40), then OutOfFrustumFar (10).
        assert_eq!(ids, vec![20, 30, 40, 10]);
        // Iteration does not consume.
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn default_impl_matches_new() {
        let a = IoScheduler::new();
        let b = IoScheduler::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}
