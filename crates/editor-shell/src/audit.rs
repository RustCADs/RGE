//! Local **stub** for `kernel/audit-ledger`.
//!
//! Per PLAN.md §6.16.6: the Command Bus emits events into the audit ledger;
//! PIE Play/Stop transitions are one such event class (PLAN.md §6.13). The
//! ledger crate is stubbed at the workspace level; W03 stands up a
//! ring-buffer-backed in-memory ledger here so the lifecycle code that
//! *would* call `audit_ledger.record(...)` can be wired without a
//! cross-crate dependency.
//!
//! When `kernel/audit-ledger` lands, the call sites continue to use
//! [`AuditLedger::record`] and the field type swaps to the real ledger
//! handle.

use std::collections::VecDeque;

/// What kind of event the editor recorded. Closed enum so the audit-log
/// projection (undo, replay, diagnostics) doesn't have to carry a `dyn`.
///
/// `Eq` is intentionally **not** derived because `TimeScaleChanged` carries
/// `f32` (which lacks `Eq`); event-equality in tests goes through `tag()`
/// or a custom comparator. Real `kernel/audit-ledger` will likely store
/// events as serialized bytes, sidestepping this entirely.
#[derive(Debug, Clone, PartialEq)]
pub enum AuditEvent {
    /// `[Play]` pressed.
    PlayPressed {
        /// Label of the prior `PlayState` (for replay diagnostics).
        before_state: &'static str,
    },
    /// `[Pause]` pressed.
    PausePressed,
    /// `[Stop]` pressed — round-trip restored the snapshot.
    StopPressed,
    /// `[Step]` pressed — single-tick advance.
    StepPressed,
    /// `[FrameStep]` — single-frame advance (no scaled tick; one render only).
    FrameStepPressed,
    /// Time-scale slider changed.
    TimeScaleChanged {
        /// Previous scale value.
        from: f32,
        /// New scale value (already clamped to `[MIN, MAX]`).
        to: f32,
    },
    /// PIE snapshot captured (entity count + serialized byte length).
    SnapshotCaptured {
        /// Number of entities in the captured world.
        entity_count: usize,
        /// Size of the serialized byte stream.
        bytes: usize,
    },
    /// PIE snapshot restored (entity count + serialized byte length).
    SnapshotRestored {
        /// Number of entities in the restored world.
        entity_count: usize,
        /// Size of the serialized byte stream.
        bytes: usize,
    },
}

impl AuditEvent {
    /// Stable string tag for diagnostics / log lines.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::PlayPressed { .. } => "PlayPressed",
            Self::PausePressed => "PausePressed",
            Self::StopPressed => "StopPressed",
            Self::StepPressed => "StepPressed",
            Self::FrameStepPressed => "FrameStepPressed",
            Self::TimeScaleChanged { .. } => "TimeScaleChanged",
            Self::SnapshotCaptured { .. } => "SnapshotCaptured",
            Self::SnapshotRestored { .. } => "SnapshotRestored",
        }
    }
}

/// In-memory ring-buffer audit ledger. Real `kernel/audit-ledger` will be
/// content-addressed + persisted; W03 only needs the call shape to validate
/// the lifecycle wiring.
///
/// Capacity defaults to 1024; on overflow the oldest event is dropped.
/// (Real ledger spills to disk; that's W22+.)
#[derive(Debug)]
pub struct AuditLedger {
    capacity: usize,
    events: VecDeque<AuditEvent>,
}

impl Default for AuditLedger {
    fn default() -> Self {
        Self::with_capacity(1024)
    }
}

impl AuditLedger {
    /// Construct a ledger with a fixed event-capacity ring buffer.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
        }
    }

    /// Append an event. If capacity is exceeded, oldest event drops.
    pub fn record(&mut self, event: AuditEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        tracing::debug!(target: "rge::editor-shell::audit", tag = event.tag(), "audit event");
        self.events.push_back(event);
    }

    /// Iterate recorded events in chronological order.
    pub fn iter(&self) -> impl Iterator<Item = &AuditEvent> + '_ {
        self.events.iter()
    }

    /// Number of events currently in the ledger.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True if the ledger has no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Most recent event, if any.
    #[must_use]
    pub fn last(&self) -> Option<&AuditEvent> {
        self.events.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_appends() {
        let mut l = AuditLedger::default();
        l.record(AuditEvent::PlayPressed {
            before_state: "Editing",
        });
        assert_eq!(l.len(), 1);
        assert_eq!(l.last().unwrap().tag(), "PlayPressed");
    }

    #[test]
    fn ring_buffer_drops_oldest() {
        let mut l = AuditLedger::with_capacity(2);
        l.record(AuditEvent::PausePressed);
        l.record(AuditEvent::StopPressed);
        l.record(AuditEvent::StepPressed);
        assert_eq!(l.len(), 2);
        let tags: Vec<_> = l.iter().map(AuditEvent::tag).collect();
        assert_eq!(tags, vec!["StopPressed", "StepPressed"]);
    }
}
