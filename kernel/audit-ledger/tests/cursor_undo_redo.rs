//! Integration test: cursor, undo stream, redo stream, and truncation.

use rge_kernel_audit_ledger::{AuditLedger, EventKind, LedgerError};

#[test]
fn cursor_undo_redo_full_scenario() {
    let mut ledger = AuditLedger::new();
    for i in 0..5_u64 {
        ledger.record(EventKind::Action, format!("event-{i}").into_bytes());
    }
    assert_eq!(ledger.len(), 5);
    assert_eq!(ledger.cursor(), 0);

    // ── set_cursor(5): cursor at end ─────────────────────────────────────────
    ledger.set_cursor(5).expect("cursor at len is valid");
    assert_eq!(ledger.cursor(), 5);

    let redo: Vec<u64> = ledger.redo_stream().map(|e| e.seq).collect();
    assert!(
        redo.is_empty(),
        "redo stream must be empty when cursor == len"
    );

    let undo: Vec<u64> = ledger.undo_stream().map(|e| e.seq).collect();
    assert_eq!(
        undo,
        vec![4, 3, 2, 1, 0],
        "undo stream must be all 5 events in reverse"
    );

    // ── set_cursor(2): partial undo ──────────────────────────────────────────
    ledger.set_cursor(2).expect("cursor=2 is valid");
    assert_eq!(ledger.cursor(), 2);

    let redo: Vec<u64> = ledger.redo_stream().map(|e| e.seq).collect();
    assert_eq!(redo, vec![2, 3, 4], "redo stream has 3 events");

    let undo: Vec<u64> = ledger.undo_stream().map(|e| e.seq).collect();
    assert_eq!(undo, vec![1, 0], "undo stream has 2 events in reverse");

    // ── truncate(3) while cursor=2: succeeds ─────────────────────────────────
    ledger
        .truncate(3)
        .expect("truncate(3) with cursor=2 must succeed");
    assert_eq!(ledger.len(), 3);
    assert_eq!(ledger.cursor(), 2, "cursor unchanged after truncate");

    // ── truncate(1) while cursor=2: errors ───────────────────────────────────
    let err = ledger
        .truncate(1)
        .expect_err("truncate below cursor must fail");
    assert_eq!(
        err,
        LedgerError::TruncateBeforeCursor {
            target: 1,
            cursor: 2,
        }
    );
    // Ledger unchanged after failed truncate.
    assert_eq!(ledger.len(), 3);
}
