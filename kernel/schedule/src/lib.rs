//! `rge-kernel-schedule` — boring, observable, deterministic system scheduler.
//!
//! Failure class: kernel-fatal
//!
//! Per PLAN.md §1.13 (line 572): a deadlock detected by the scheduler is
//! kernel-fatal — the engine cannot recover and must exit. API-level errors
//! (duplicate-system registration, dependency cycle at build time, missing
//! dependency) are caught BEFORE `run()` and surface as `ScheduleError`; those
//! are recoverable for the caller. The kernel-fatal class applies to runtime
//! invariant violations during `run()` (deadlock, system panic that the
//! supervisor cannot quarantine).
//!
//! Implements Phase 1.5 of IMPLEMENTATION.md. Single-threaded synchronous
//! execution with declared async-boundary metadata for future scheduler use.
//! Determinism is guaranteed via `BTreeMap` and alphabetical tiebreaking.
//!
//! # Design
//!
//! Systems are registered with a [`Stage`] and optional dependency edges.
//! Calling [`Schedule::build`] performs topological sorting (Kahn's algorithm)
//! within each stage, with `SystemId` alphabetical tiebreaking. Calling
//! [`Schedule::run`] executes every system once in the resulting order.

pub mod schedule;
pub mod stage;
pub mod system;

pub use schedule::{Schedule, ScheduleError};
pub use stage::Stage;
pub use system::{AsyncBoundary, SystemDescriptor, SystemId};
