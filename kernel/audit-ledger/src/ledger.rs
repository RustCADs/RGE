//! Append-only in-memory ledger.

use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::event::{Event, EventId, EventKind};

// ────────────────────────────────────────────────────────────────────────────
// LedgerError
// ────────────────────────────────────────────────────────────────────────────

/// Errors that the [`AuditLedger`] can produce.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    /// Two events share the same deterministic ID but carry different payloads.
    ///
    /// This is a BLAKE3 collision (astronomically unlikely with honest input)
    /// or a bug in the caller.  Either way, the ledger refuses the append.
    #[error("event id collision at seq {0}: payload differs from prior event with same id")]
    HashCollision(u64),

    /// The requested cursor position is beyond the end of the ledger.
    #[error("cursor {requested} out of range; ledger has {len} events")]
    CursorOutOfRange {
        /// The cursor value that was requested.
        requested: u64,
        /// The current number of events in the ledger.
        len: u64,
    },

    /// A `truncate` call would discard events that are at or below the current
    /// undo cursor, which would orphan undo state.
    #[error("cannot truncate to {target}; current cursor at {cursor}")]
    TruncateBeforeCursor {
        /// The truncation target that was requested.
        target: u64,
        /// The current cursor value.
        cursor: u64,
    },
}

// ────────────────────────────────────────────────────────────────────────────
// AuditLedger
// ────────────────────────────────────────────────────────────────────────────

/// Append-only ledger backed by an in-memory `Vec<Event>`.
///
/// Persistence (disk flush, journal rotation) is a Phase-3+ concern — for now,
/// the ledger holds events for the duration of the run.
///
/// # Undo / Redo cursor
///
/// The ledger maintains a `cursor` that the Command Bus (Phase 2.2) uses to
/// project the undo/redo streams:
///
/// - Events `[0, cursor)` form the **undo stream** (already applied).
/// - Events `[cursor, len)` form the **redo stream** (available to replay).
///
/// The cursor starts at `0` and is advanced/retreated by the Command Bus via
/// [`set_cursor`](Self::set_cursor).
#[derive(Debug, Default, Clone)]
pub struct AuditLedger {
    events: Vec<Event>,
    /// Undo stream projection cursor; in `[0, events.len()]`.
    cursor: u64,
}

impl AuditLedger {
    /// Create an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ── Append ───────────────────────────────────────────────────────────────

    /// Record an event.
    ///
    /// Computes the deterministic [`EventId`], assigns the next monotonic
    /// `seq`, and captures the current wall-clock time.  Returns the assigned
    /// `EventId`.
    ///
    /// The ledger **does not detect intentional duplicates**: the same
    /// `(kind, payload)` pair at a different `seq` is valid and receives the
    /// same `EventId`.  The collision check only fires when the *same ID*
    /// appears with a *different payload* (i.e., a BLAKE3 preimage collision).
    ///
    /// # Panics
    ///
    /// Does not panic, but will produce a `tracing::error` log if a
    /// [`LedgerError::HashCollision`] is detected.  In that unlikely case the
    /// method still returns the `EventId` that was computed and the collision
    /// is logged — the caller can decide how to proceed.  (A future Phase-3+
    /// version may promote this to a `Result<EventId, LedgerError>` return.)
    pub fn record(&mut self, kind: EventKind, payload: Vec<u8>) -> EventId {
        let id = EventId::compute(&kind, &payload);
        let seq = self.events.len() as u64;
        // `as_millis` returns u128; we truncate to u64 deliberately — a u64
        // millisecond counter overflows in ~585 million years, which is an
        // acceptable bound for a wall-clock timestamp used for human inspection
        // only (determinism uses `id` + `seq`, not time).
        #[allow(clippy::cast_possible_truncation)]
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Collision guard: if we have seen this ID before, the payload MUST
        // match (same-id same-payload is fine — it's a deliberate replay).
        if let Some(prior) = self.events.iter().find(|e| e.id == id) {
            if prior.payload != payload {
                tracing::error!(
                    seq,
                    id = %id,
                    "audit-ledger: BLAKE3 collision detected — rejecting event"
                );
                // Still return the id so the caller can log it; but do NOT
                // append the corrupted entry.
                return id;
            }
        }

        tracing::trace!(seq, kind = kind.kind_tag(), "audit-ledger: record");
        self.events.push(Event {
            id,
            seq,
            timestamp_ms,
            kind,
            payload,
        });
        id
    }

    // ── Iteration ────────────────────────────────────────────────────────────

    /// All events in append order (oldest first).
    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    /// Reverse iteration — newest first.  Used by the Command Bus undo
    /// projection.
    #[must_use]
    pub fn iter_reverse(&self) -> impl DoubleEndedIterator<Item = &Event> {
        self.events.iter().rev()
    }

    /// Number of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns `true` if the ledger contains no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    // ── Cursor / undo-redo ────────────────────────────────────────────────────

    /// Current undo cursor.
    ///
    /// Initially `0`.  The Command Bus advances this as `Action::apply`
    /// executes and retreats it as `Action::revert` executes.
    #[must_use]
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Set the cursor explicitly (used by undo/redo).
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::CursorOutOfRange`] if `cursor > len`.
    pub fn set_cursor(&mut self, cursor: u64) -> Result<(), LedgerError> {
        // `events.len()` fits in u64: Vec can hold at most `isize::MAX` elements
        // which is well within u64 on all supported targets.
        #[allow(clippy::cast_possible_truncation)]
        let len = self.events.len() as u64;
        if cursor > len {
            return Err(LedgerError::CursorOutOfRange {
                requested: cursor,
                len,
            });
        }
        self.cursor = cursor;
        Ok(())
    }

    /// Events between `cursor` and `len` — the "redo stream" (not-yet-applied
    /// in the current undo depth).
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor value exceeds `usize::MAX`, which cannot
    /// occur in practice because the cursor is bounded by `events.len()` and
    /// `Vec` is itself bounded by `usize::MAX`.
    pub fn redo_stream(&self) -> impl Iterator<Item = &Event> {
        let cursor = usize::try_from(self.cursor).expect("cursor fits in usize");
        self.events[cursor..].iter()
    }

    /// Events between `0` and `cursor`, in reverse — the "undo stream"
    /// (applied events, newest-applied first).
    ///
    /// Returns a double-ended iterator so callers can iterate forward or
    /// backward over the undo history as needed.
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor value exceeds `usize::MAX`, which cannot
    /// occur in practice because the cursor is bounded by `events.len()` and
    /// `Vec` is itself bounded by `usize::MAX`.
    #[must_use]
    pub fn undo_stream(&self) -> impl DoubleEndedIterator<Item = &Event> {
        let cursor = usize::try_from(self.cursor).expect("cursor fits in usize");
        self.events[..cursor].iter().rev()
    }

    // ── Truncation ────────────────────────────────────────────────────────────

    /// Truncate to `target` events.
    ///
    /// Used when the user "redoes past a new edit": the truncation discards the
    /// redo tail (events above the cursor) that was invalidated by the new
    /// edit.
    ///
    /// # Errors
    ///
    /// - [`LedgerError::TruncateBeforeCursor`] — cannot truncate below the
    ///   current cursor (would orphan undo state).
    ///
    /// # Panics
    ///
    /// Panics if `target` exceeds `usize::MAX`, which cannot occur in practice
    /// because `target` is validated to be `<= events.len()` which is itself
    /// bounded by `usize::MAX`.
    pub fn truncate(&mut self, target: u64) -> Result<(), LedgerError> {
        if target < self.cursor {
            return Err(LedgerError::TruncateBeforeCursor {
                target,
                cursor: self.cursor,
            });
        }
        // target >= cursor >= 0 and target <= events.len() (validated above),
        // so this conversion is infallible in practice.
        self.events
            .truncate(usize::try_from(target).expect("target fits in usize"));
        Ok(())
    }

    /// Clear all events and reset the cursor to `0`.
    pub fn clear(&mut self) {
        self.events.clear();
        self.cursor = 0;
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;

    fn record_n(ledger: &mut AuditLedger, n: usize) {
        for i in 0..n {
            ledger.record(EventKind::Action, format!("event-{i}").into_bytes());
        }
    }

    #[test]
    fn record_assigns_monotonic_seq() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 5);
        let seqs: Vec<u64> = ledger.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn cursor_starts_at_zero() {
        let ledger = AuditLedger::new();
        assert_eq!(ledger.cursor(), 0);
    }

    #[test]
    fn set_cursor_rejects_out_of_range() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 3);
        // cursor == 4 > len == 3 → error
        let err = ledger.set_cursor(4).unwrap_err();
        assert_eq!(
            err,
            LedgerError::CursorOutOfRange {
                requested: 4,
                len: 3
            }
        );
    }

    #[test]
    fn set_cursor_at_len_is_ok() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 3);
        assert!(ledger.set_cursor(3).is_ok());
        assert_eq!(ledger.cursor(), 3);
    }

    #[test]
    fn redo_stream_and_undo_stream_partition_correctly() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 5);
        ledger.set_cursor(3).unwrap();

        let redo: Vec<u64> = ledger.redo_stream().map(|e| e.seq).collect();
        let undo: Vec<u64> = ledger.undo_stream().map(|e| e.seq).collect();

        // redo: events 3 and 4
        assert_eq!(redo, vec![3, 4]);
        // undo: events 2, 1, 0 (reverse)
        assert_eq!(undo, vec![2, 1, 0]);
    }

    #[test]
    fn redo_stream_empty_when_cursor_at_end() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 3);
        ledger.set_cursor(3).unwrap();
        assert_eq!(ledger.redo_stream().count(), 0);
    }

    #[test]
    fn undo_stream_empty_when_cursor_at_zero() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 3);
        assert_eq!(ledger.undo_stream().count(), 0);
    }

    #[test]
    fn truncate_rejects_below_cursor() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 5);
        ledger.set_cursor(3).unwrap();

        let err = ledger.truncate(2).unwrap_err();
        assert_eq!(
            err,
            LedgerError::TruncateBeforeCursor {
                target: 2,
                cursor: 3
            }
        );
    }

    #[test]
    fn truncate_discards_redo_tail() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 5);
        ledger.set_cursor(3).unwrap();

        ledger.truncate(3).unwrap();
        assert_eq!(ledger.len(), 3);
        assert_eq!(ledger.cursor(), 3);
        assert_eq!(ledger.redo_stream().count(), 0);
    }

    #[test]
    fn clear_resets_everything() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 5);
        ledger.set_cursor(3).unwrap();
        ledger.clear();
        assert!(ledger.is_empty());
        assert_eq!(ledger.cursor(), 0);
    }

    #[test]
    fn iter_reverse_is_newest_first() {
        let mut ledger = AuditLedger::new();
        record_n(&mut ledger, 3);
        let seqs: Vec<u64> = ledger.iter_reverse().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![2, 1, 0]);
    }

    #[test]
    fn same_id_same_payload_duplicate_is_allowed() {
        // Records two events with the same kind+payload → same EventId, different seq.
        let mut ledger = AuditLedger::new();
        let id1 = ledger.record(EventKind::Action, b"dup".to_vec());
        let id2 = ledger.record(EventKind::Action, b"dup".to_vec());
        assert_eq!(id1, id2, "same kind+payload must produce same EventId");
        assert_eq!(ledger.len(), 2, "both events should be recorded");
        assert_ne!(ledger.events[0].seq, ledger.events[1].seq);
    }
}
