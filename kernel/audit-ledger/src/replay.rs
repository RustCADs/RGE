//! Replay pass over a [`crate::AuditLedger`].
//!
//! Replay iterates every event in append order and calls a user-supplied
//! handler.  The handler returns `true` to continue or `false` to stop early.
//! Replay never mutates the ledger.

use crate::{AuditLedger, Event};

// ────────────────────────────────────────────────────────────────────────────
// ReplayResult
// ────────────────────────────────────────────────────────────────────────────

/// Outcome of a [`AuditLedger::replay`] pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    /// Number of events the handler returned `true` for (i.e. consumed).
    pub events_applied: u64,
    /// Number of events skipped because the handler halted early.
    pub events_skipped: u64,
    /// The `seq` of the last event *applied* before the handler halted, or
    /// `None` if all events were applied (or the ledger was empty).
    ///
    /// When the handler halts after consuming `seq = N`, `stopped_at_seq` is
    /// `Some(N)`.  When all events are consumed, it is `None`.
    pub stopped_at_seq: Option<u64>,
}

// ────────────────────────────────────────────────────────────────────────────
// AuditLedger::replay (impl block lives here for organisation)
// ────────────────────────────────────────────────────────────────────────────

impl AuditLedger {
    /// Replay every event in append order, calling `handler(event)`.
    ///
    /// The handler returns `true` to continue or `false` to stop.  Returning
    /// `false` consumes the current event (it is counted in
    /// [`ReplayResult::events_applied`]) and then halts — subsequent events
    /// are counted in [`ReplayResult::events_skipped`].
    ///
    /// Does **not** mutate the ledger.
    pub fn replay<F>(&self, mut handler: F) -> ReplayResult
    where
        F: FnMut(&Event) -> bool,
    {
        let mut events_applied: u64 = 0;
        let mut stopped_at_seq: Option<u64> = None;

        for event in self.iter() {
            let cont = handler(event);
            events_applied += 1;

            if !cont {
                stopped_at_seq = Some(event.seq);
                break;
            }
        }

        let total = self.len() as u64;
        let events_skipped = total.saturating_sub(events_applied);

        ReplayResult {
            events_applied,
            events_skipped,
            stopped_at_seq,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::{AuditLedger, EventKind};

    fn ledger_with_n(n: usize) -> AuditLedger {
        let mut l = AuditLedger::new();
        for i in 0..n {
            l.record(EventKind::Action, format!("e{i}").into_bytes());
        }
        l
    }

    #[test]
    fn replay_all_events_returns_none_stopped_at() {
        let ledger = ledger_with_n(5);
        let result = ledger.replay(|_| true);
        assert_eq!(result.events_applied, 5);
        assert_eq!(result.events_skipped, 0);
        assert_eq!(result.stopped_at_seq, None);
    }

    #[test]
    fn replay_empty_ledger() {
        let ledger = AuditLedger::new();
        let result = ledger.replay(|_| true);
        assert_eq!(result.events_applied, 0);
        assert_eq!(result.events_skipped, 0);
        assert_eq!(result.stopped_at_seq, None);
    }

    #[test]
    fn replay_never_mutates_ledger() {
        let mut ledger = ledger_with_n(3);
        ledger.set_cursor(2).unwrap();
        let _ = ledger.replay(|_| true);
        assert_eq!(ledger.len(), 3);
        assert_eq!(ledger.cursor(), 2);
    }

    #[test]
    fn replay_handler_sees_events_in_order() {
        let ledger = ledger_with_n(4);
        let mut seqs = Vec::new();
        ledger.replay(|e| {
            seqs.push(e.seq);
            true
        });
        assert_eq!(seqs, vec![0, 1, 2, 3]);
    }
}
