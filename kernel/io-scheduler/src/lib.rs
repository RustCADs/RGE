//! `rge-kernel-io-scheduler` — priority I/O queue substrate.
//!
//! Failure class: recoverable
//!
//! Implements the priority IO queue described in PLAN.md §7 (Async / resource
//! streaming) and listed at §10.1 alongside `kernel/job-system`,
//! `kernel/asset-streaming`, and `kernel/asset-view`. The 4-tier streaming
//! priority taxonomy (in-frustum-near / in-frustum-far / out-of-frustum-near
//! / out-of-frustum-far) is captured by [`Priority`]; the queue is captured
//! by [`IoScheduler`]; in-flight requests are carried by [`IoRequest`] +
//! [`IoRequestId`] + [`IoRequestKind`].
//!
//! # NON-GOALS
//!
//! v0 establishes vocabulary and ownership boundaries; it deliberately does
//! NOT establish behaviour richness. The strongest part of this crate's v0
//! is the list of what it intentionally is **not**:
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
//!
//! # What this crate is
//!
//! Vocabulary, ownership boundaries, and future-safe seams. Future
//! dispatches extend this substrate incrementally without undoing the
//! foundational choices made here: the priority enum is `#[non_exhaustive]`
//! so new tiers may be added; the request kind is `#[non_exhaustive]` so
//! driver-specific variants may be added; the scheduler is `BTreeMap`-backed
//! so iteration is deterministic and reproducible.

pub mod priority;
pub mod request;
pub mod scheduler;

pub use priority::Priority;
pub use request::{IoRequest, IoRequestId, IoRequestKind};
pub use scheduler::IoScheduler;

#[cfg(test)]
mod smoke {
    use super::*;

    /// End-to-end: construct scheduler, submit two requests at different
    /// priorities, assert pop returns higher priority first.
    #[test]
    fn scheduler_pops_higher_priority_before_lower() {
        let mut s = IoScheduler::new();
        s.submit(IoRequest {
            id: IoRequestId::from_bytes([0xaa; 16]),
            priority: Priority::OutOfFrustumFar,
            kind: IoRequestKind::Placeholder,
        });
        s.submit(IoRequest {
            id: IoRequestId::from_bytes([0xbb; 16]),
            priority: Priority::InFrustumNear,
            kind: IoRequestKind::Placeholder,
        });

        let first = s.pop().expect("first pop");
        assert_eq!(first.priority, Priority::InFrustumNear);
        assert_eq!(first.id.as_bytes(), &[0xbb; 16]);

        let second = s.pop().expect("second pop");
        assert_eq!(second.priority, Priority::OutOfFrustumFar);
        assert_eq!(second.id.as_bytes(), &[0xaa; 16]);

        assert!(s.is_empty());
    }
}
