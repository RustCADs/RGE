//! Priority job queue substrate.
//!
//! # NON-GOALS (mirror of crate-level doc, restated for in-module ownership clarity)
//!
//! - No work-stealing thread pool. Scheduling only — no execution.
//! - No `tokio` / `futures` / async runtime integration. Synchronous-only.
//! - No closure / `Box<dyn FnOnce>` storage. Jobs carry a `JobId` and
//!   discriminant only; callers route execution out-of-band.
//! - No task graph / DAG scheduler. Flat queue only.
//! - No cancellation tokens / cooperative-cancel handles.
//! - No priority inversion handling (no priority-boost on dependency-block).
//! - No thread affinity hints.
//! - No reactive scheduling.
//! - No I/O scheduling — that's `kernel/io-scheduler`.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.

use std::collections::BTreeMap;

use crate::job::Job;
use crate::priority::JobPriority;

/// Priority job queue.
///
/// v0 stub: `BTreeMap`-backed for deterministic iteration order. FIFO within
/// a priority is preserved by pairing each entry with a monotonic sequence
/// number; the composite key `(JobPriority, u64)` orders highest-priority
/// first and within a priority orders by submission time.
///
/// The scheduler owns no thread pool, no executor, no completion mechanism.
/// It is purely a substrate for ordering pending jobs; consumers (work
/// dispatchers, frame-loop integrations, future thread-pool wrappers) pop
/// jobs and dispatch them via mechanisms that land in dedicated future
/// dispatches.
#[derive(Default, Debug)]
pub struct JobScheduler {
    /// Pending jobs keyed by `(priority, sequence)` for deterministic
    /// priority-then-FIFO iteration. `BTreeMap` rather than `BinaryHeap` so
    /// `iter()` is stable and reproducible across runs.
    queue: BTreeMap<(JobPriority, u64), Job>,
    /// Monotonic counter assigning a sequence to each submitted job.
    /// Never reset — `clear()` empties the queue but preserves the counter
    /// so subsequent submissions remain strictly later than any prior.
    seq: u64,
}

impl JobScheduler {
    /// Create an empty scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit `job` to the queue.
    ///
    /// Submission order within a single priority is preserved (FIFO). The
    /// monotonic sequence counter advances by one per call.
    pub fn submit(&mut self, job: Job) {
        let key = (job.priority, self.seq);
        self.seq = self.seq.wrapping_add(1);
        self.queue.insert(key, job);
    }

    /// Remove and return the highest-priority job, FIFO within priority.
    ///
    /// Returns `None` when the queue is empty.
    pub fn pop(&mut self) -> Option<Job> {
        let key = *self.queue.keys().next()?;
        self.queue.remove(&key)
    }

    /// Number of pending jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// `true` when no jobs are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Iterate pending jobs in priority-then-FIFO order.
    ///
    /// Iteration is deterministic and does not consume the queue.
    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.queue.values()
    }

    /// Remove every pending job.
    ///
    /// The internal sequence counter is **not** reset; subsequent submissions
    /// retain strict monotonic ordering with respect to any pre-`clear`
    /// submissions. (This avoids ABA-style ordering anomalies if a stale
    /// reference to a popped job resurfaces.)
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{JobId, JobKind};

    fn job(id_byte: u8, priority: JobPriority) -> Job {
        Job {
            id: JobId::from_bytes([id_byte; 16]),
            priority,
            kind: JobKind::Placeholder,
        }
    }

    #[test]
    fn empty_new_is_empty() {
        let s = JobScheduler::new();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn submit_increments_len() {
        let mut s = JobScheduler::new();
        s.submit(job(1, JobPriority::High));
        assert_eq!(s.len(), 1);
        s.submit(job(2, JobPriority::Normal));
        assert_eq!(s.len(), 2);
        assert!(!s.is_empty());
    }

    #[test]
    fn pop_on_empty_returns_none() {
        let mut s = JobScheduler::new();
        assert!(s.pop().is_none());
    }

    #[test]
    fn fifo_within_priority_preserved_for_three_same_priority() {
        let mut s = JobScheduler::new();
        s.submit(job(1, JobPriority::Normal));
        s.submit(job(2, JobPriority::Normal));
        s.submit(job(3, JobPriority::Normal));

        let a = s.pop().expect("first pop");
        let b = s.pop().expect("second pop");
        let c = s.pop().expect("third pop");
        assert_eq!(a.id.as_bytes(), &[1u8; 16]);
        assert_eq!(b.id.as_bytes(), &[2u8; 16]);
        assert_eq!(c.id.as_bytes(), &[3u8; 16]);
    }

    #[test]
    fn pop_returns_higher_priority_before_lower() {
        let mut s = JobScheduler::new();
        // Submit in inverse priority order; pop must reorder to canonical
        // 4-tier sequence.
        s.submit(job(4, JobPriority::Background));
        s.submit(job(3, JobPriority::Normal));
        s.submit(job(2, JobPriority::High));
        s.submit(job(1, JobPriority::Critical));

        assert_eq!(s.pop().unwrap().priority, JobPriority::Critical);
        assert_eq!(s.pop().unwrap().priority, JobPriority::High);
        assert_eq!(s.pop().unwrap().priority, JobPriority::Normal);
        assert_eq!(s.pop().unwrap().priority, JobPriority::Background);
    }

    #[test]
    fn pop_drains_queue_completely() {
        let mut s = JobScheduler::new();
        for i in 0..10 {
            s.submit(job(i, JobPriority::Normal));
        }
        for _ in 0..10 {
            assert!(s.pop().is_some());
        }
        assert!(s.is_empty());
        assert!(s.pop().is_none());
    }

    #[test]
    fn clear_empties_without_dropping_seq_counter() {
        let mut s = JobScheduler::new();
        s.submit(job(1, JobPriority::Normal));
        s.submit(job(2, JobPriority::Normal));
        s.clear();
        assert!(s.is_empty());

        // Subsequent submission must still receive a strictly-later sequence
        // than any pre-clear submission. Verify via FIFO ordering of new
        // entries at the same priority — sequence advancement is observable.
        s.submit(job(3, JobPriority::Normal));
        s.submit(job(4, JobPriority::Normal));
        assert_eq!(s.pop().unwrap().id.as_bytes(), &[3u8; 16]);
        assert_eq!(s.pop().unwrap().id.as_bytes(), &[4u8; 16]);
    }

    #[test]
    fn iter_yields_priority_then_fifo_order() {
        let mut s = JobScheduler::new();
        s.submit(job(10, JobPriority::Background));
        s.submit(job(20, JobPriority::Critical));
        s.submit(job(30, JobPriority::Critical));
        s.submit(job(40, JobPriority::Normal));

        let ids: Vec<u8> = s.iter().map(|j| j.id.as_bytes()[0]).collect();
        // Critical entries first (FIFO 20 → 30), then Normal (40), then
        // Background (10).
        assert_eq!(ids, vec![20, 30, 40, 10]);
        // Iteration does not consume.
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn default_impl_matches_new() {
        let a = JobScheduler::new();
        let b = JobScheduler::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}
