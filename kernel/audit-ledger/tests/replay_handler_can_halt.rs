//! Integration test: replay handler can halt early.
//!
//! Records 10 events.  The handler returns `false` after seeing `seq == 4`
//! (the 5th event, 0-indexed).  The spec says returning `false` consumes
//! the current event then halts, so `events_applied == 5`, `events_skipped ==
//! 5`, and `stopped_at_seq == Some(4)`.

use rge_kernel_audit_ledger::{AuditLedger, EventKind};

#[test]
fn replay_handler_halts_after_seq_4() {
    let mut ledger = AuditLedger::new();
    for i in 0..10_u64 {
        ledger.record(EventKind::Action, format!("event-{i}").into_bytes());
    }
    assert_eq!(ledger.len(), 10);

    // The handler stops after consuming the event at seq == 4 (the 5th event).
    let result = ledger.replay(|e| {
        // Return false on seq 4 — still counts as applied.
        e.seq != 4
    });

    assert_eq!(result.events_applied, 5, "events 0-4 are applied");
    assert_eq!(result.events_skipped, 5, "events 5-9 are skipped");
    assert_eq!(result.stopped_at_seq, Some(4));
}
