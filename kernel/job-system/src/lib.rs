//! `rge-kernel-job-system` — priority job queue substrate.
//!
//! Failure class: recoverable
//!
//! Implements the priority job queue listed in PLAN.md §10.1 alongside
//! `kernel/io-scheduler`, `kernel/asset-streaming`, and `kernel/asset-view`.
//! PLAN §10.1 frames this as the eventual "work-stealing thread pool" for
//! the engine; v0 ships only the vocabulary substrate (priority taxonomy +
//! job carrier + queue ordering) without any execution mechanism.
//!
//! # NON-GOALS
//!
//! v0 establishes vocabulary and ownership boundaries; it deliberately does
//! NOT establish behaviour richness. The strongest part of this crate's v0
//! is the list of what it intentionally is **not**:
//!
//! - No work-stealing thread pool. The PLAN §10.1 listed feature is NOT
//!   implemented here; v0 is a queue, not an executor.
//! - No closure / `Box<dyn FnOnce>` storage. Jobs carry a `JobId` and
//!   discriminant only; callers route execution out-of-band.
//! - No `tokio` / `futures` / async runtime integration. Synchronous-only.
//! - No task graph / DAG scheduler. Flat queue only.
//! - No cancellation tokens / cooperative-cancel handles.
//! - No priority inversion handling.
//! - No thread affinity hints.
//! - No reactive scheduling.
//! - No I/O scheduling — that's `kernel/io-scheduler`.
//! - No actual work execution — pool implementations / driver crates land in
//!   dedicated future dispatches.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.
//!
//! # What this crate is
//!
//! Vocabulary, ownership boundaries, and future-safe seams. Future
//! dispatches extend this substrate incrementally without undoing the
//! foundational choices made here: the priority enum is `#[non_exhaustive]`
//! so new tiers may be added; the job kind is `#[non_exhaustive]` so
//! domain-specific variants may be added; the scheduler is `BTreeMap`-backed
//! so iteration is deterministic and reproducible.

pub mod job;
pub mod priority;
pub mod scheduler;

pub use job::{Job, JobId, JobKind};
pub use priority::JobPriority;
pub use scheduler::JobScheduler;

#[cfg(test)]
mod smoke {
    use super::*;

    /// End-to-end: construct scheduler, submit two jobs at different
    /// priorities, assert pop returns higher priority first.
    #[test]
    fn scheduler_pops_higher_priority_before_lower() {
        let mut s = JobScheduler::new();
        s.submit(Job {
            id: JobId::from_bytes([0xaa; 16]),
            priority: JobPriority::Background,
            kind: JobKind::Placeholder,
        });
        s.submit(Job {
            id: JobId::from_bytes([0xbb; 16]),
            priority: JobPriority::Critical,
            kind: JobKind::Placeholder,
        });

        let first = s.pop().expect("first pop");
        assert_eq!(first.priority, JobPriority::Critical);
        assert_eq!(first.id.as_bytes(), &[0xbb; 16]);

        let second = s.pop().expect("second pop");
        assert_eq!(second.priority, JobPriority::Background);
        assert_eq!(second.id.as_bytes(), &[0xaa; 16]);

        assert!(s.is_empty());
    }
}
