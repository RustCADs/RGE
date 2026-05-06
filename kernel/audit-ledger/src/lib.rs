//! Failure class: kernel-fatal
//!
//! Append-only audit ledger per IMPLEMENTATION.md Phase 2.3 / PLAN §6.16.6.
//!
//! Per PLAN.md §1.13 (line 573): an audit-ledger checksum failure is
//! kernel-fatal — if the audit log itself can't be trusted, snapshot-restore
//! recovery (which depends on replaying the log) cannot be trusted either.
//! API-level errors (cursor out of range, hash collision on benign duplicates)
//! surface as `LedgerError` and are recoverable for the caller; the
//! kernel-fatal class applies to integrity-violation paths.
//!
//! # Overview
//!
//! The audit ledger records every [`Event`] that flows through the engine in
//! append order, assigns a monotonic sequence number, and computes a
//! deterministic [`EventId`] (BLAKE3 over `(kind_tag, payload)`) that is
//! stable across machines for identical input.
//!
//! # Recovery model
//!
//! Ledger corruption (hash collision, cursor out of range) is recoverable via
//! snapshot restore — the ledger replays from the last good snapshot.  The
//! `snapshot-recoverable` failure class reflects this: continuing without the
//! ledger would silently lose the undo/redo audit trail, which is unsafe, while
//! a full snapshot restore restores it correctly.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod event;
pub mod ledger;
pub mod replay;

pub use event::{Event, EventId, EventKind};
pub use ledger::{AuditLedger, LedgerError};
pub use replay::ReplayResult;
