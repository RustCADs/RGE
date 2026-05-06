//! Integration test: replay produces a byte-identical event log.
//!
//! Records three events, captures them via `replay`, constructs a fresh ledger
//! from the captured events, and verifies that deterministic IDs, kinds, and
//! payloads all match (timestamps and seq numbers intentionally differ).

use rge_kernel_audit_ledger::{AuditLedger, EventKind};

#[test]
fn replay_produces_identical_event_log() {
    let mut a = AuditLedger::new();
    a.record(EventKind::Action, b"insert(transform=Identity)".to_vec());
    a.record(EventKind::Action, b"insert(name='player')".to_vec());
    a.record(EventKind::CadCheckpoint, b"snapshot id=0".to_vec());

    // Capture all events into a Vec via replay.
    let mut captured = Vec::new();
    let result = a.replay(|e| {
        captured.push(e.clone());
        true
    });

    assert_eq!(
        result.events_applied, 3,
        "all three events should be applied"
    );
    assert_eq!(result.events_skipped, 0);
    assert_eq!(result.stopped_at_seq, None);
    assert_eq!(captured.len(), 3);

    // Reconstruct: build a fresh ledger from the captured events.
    let mut b = AuditLedger::new();
    for e in &captured {
        b.record(e.kind.clone(), e.payload.clone());
    }

    // ID + payload + kind match (timestamps + seq differ — by design).
    for (a_evt, b_evt) in a.iter().zip(b.iter()) {
        assert_eq!(a_evt.id, b_evt.id, "deterministic ID mismatch");
        assert_eq!(a_evt.kind, b_evt.kind, "kind mismatch");
        assert_eq!(a_evt.payload, b_evt.payload, "payload mismatch");
    }
}
