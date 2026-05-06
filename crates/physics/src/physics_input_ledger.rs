//! Physics-domain per-tick input ledger.
//!
//! Records [`PhysicsInput`] events (force / impulse / joint-motor) one
//! [`TickRecord`] per simulation tick so replay can reconstruct the
//! trajectory deterministically (PLAN §1.6.8 Replay-Stable v1.0).
//!
//! # Why a domain ledger, not the kernel audit ledger
//!
//! Earlier W11 wave called this `stubs::audit_ledger::AuditLedger` and
//! framed it as a "stub" of the kernel substrate at
//! [`rge_kernel_audit_ledger::AuditLedger`]. The deep audit 2026-05-09 +
//! follow-on MEDIUM-batch dispatch surfaced that the two have
//! **structurally different domains and APIs**:
//!
//! - **This ledger** is per-tick + physics-domain typed:
//!   `Vec<TickRecord>` where each record is `{ tick, inputs:
//!   Vec<PhysicsInput> }` and `PhysicsInput` enumerates only the input
//!   classes the physics step needs (`Force`, `Impulse`, `JointMotor`).
//!   The recording layer in [`crate::step`] writes
//!   `ledger.records.last_mut().expect(...).inputs.push(input)` directly.
//!
//! - **`rge_kernel_audit_ledger::AuditLedger`** is a generic event ledger:
//!   `Vec<Event>` where each event is `{ id: EventId (BLAKE3), seq,
//!   timestamp_ms, kind: EventKind { Action / CadCheckpoint /
//!   Custom(String) }, payload: Vec<u8> }` with an undo/redo cursor.
//!
//! No API-compatible swap exists. The lightest-touch close is to formalise
//! that physics has its own domain ledger separate from the engine-wide
//! event ledger — hence the rename from `AuditLedger` to
//! [`PhysicsInputLedger`]. Future work could (a) extend the kernel
//! substrate with first-class per-tick / typed-payload abstractions and
//! migrate physics onto that, or (b) keep the two ledgers separate
//! permanently (the current decision per audit-debt registry).
//!
//! Cross-ref: ADR-114 four-substrate-validation amendment table — the
//! `AuditLedger` entry refers to THIS [`PhysicsInputLedger`], NOT the
//! kernel substrate.

use serde::{Deserialize, Serialize};

/// One simulation input applied during a tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PhysicsInput {
    /// External force applied to a body for one tick.
    Force {
        /// Stable body identity (handle.0).
        body: u64,
        /// Force vector (N).
        force: [f32; 3],
    },
    /// Instantaneous impulse (units of N·s).
    Impulse {
        /// Stable body identity.
        body: u64,
        /// Impulse vector.
        impulse: [f32; 3],
    },
    /// Joint motor torque target.
    JointMotor {
        /// Joint index.
        joint: u64,
        /// Target velocity for the motor (rad/s or m/s depending on kind).
        target_vel: f32,
        /// Stiffness factor.
        factor: f32,
    },
}

/// One tick's record: tick index plus the inputs that landed on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TickRecord {
    /// Monotonic tick index from epoch 0.
    pub tick: u64,
    /// Inputs in solver-application order.
    pub inputs: Vec<PhysicsInput>,
}

/// Append-only physics-domain per-tick input ledger.
///
/// Renamed from `AuditLedger` 2026-05-09 (audit-debt MEDIUM closure) to
/// stop presenting the type as a "stub" of [`rge_kernel_audit_ledger::AuditLedger`]
/// — see module-level docs for the full rationale.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicsInputLedger {
    /// Records, ordered by tick.
    pub records: Vec<TickRecord>,
}

impl PhysicsInputLedger {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a tick. The returned mutable record accumulates inputs until
    /// the tick is ended; if you don't push any inputs we still record an
    /// empty entry so tick indices line up with replay.
    ///
    /// # Panics
    /// Will not panic — the `expect` after the push is unreachable
    /// because `Vec::push` followed by `last_mut` is always `Some`. The
    /// `expect` is there to make the invariant explicit.
    pub fn begin_tick(&mut self, tick: u64) -> &mut TickRecord {
        self.records.push(TickRecord {
            tick,
            inputs: Vec::new(),
        });
        self.records
            .last_mut()
            .expect("Vec::push leaves Vec non-empty")
    }

    /// Look up the recorded inputs for a tick (for replay).
    #[must_use]
    pub fn for_tick(&self, tick: u64) -> Option<&TickRecord> {
        self.records.iter().find(|r| r.tick == tick)
    }

    /// Total tick count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the ledger has any records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
