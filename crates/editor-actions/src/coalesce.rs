//! [`CoalesceWindow`] — time-window helper for same-target action coalescing.

use crate::action::ActionId;

// ---------------------------------------------------------------------------
// CoalesceWindow
// ---------------------------------------------------------------------------

/// Time-window helper for same-target [`crate::Action`] coalescing.
///
/// Two consecutive actions with the same [`ActionId`] submitted within
/// `window_ms` milliseconds are eligible for merging via
/// [`crate::Action::merge`].
///
/// Default window per PLAN §6.16.7: 500 ms.
#[derive(Debug, Clone)]
pub struct CoalesceWindow {
    /// The coalesce window in milliseconds.
    window_ms: u64,
    /// Wall-clock milliseconds when the last action was recorded.
    last_recorded_at: Option<u64>,
    /// The id of the most recently recorded action.
    last_id: Option<ActionId>,
}

impl CoalesceWindow {
    /// Create a new [`CoalesceWindow`] with the given `window_ms`.
    #[must_use]
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            last_recorded_at: None,
            last_id: None,
        }
    }

    /// Create a new [`CoalesceWindow`] with the default 500 ms window (PLAN §6.16.7).
    #[must_use]
    pub fn default_500ms() -> Self {
        Self::new(500)
    }

    /// Returns `true` when `next` should coalesce with the most recent action.
    ///
    /// Conditions (both must hold):
    /// 1. `next == last_id`
    /// 2. `now_ms - last_recorded_at <= window_ms`
    #[must_use]
    pub fn should_coalesce(&self, next: &ActionId, now_ms: u64) -> bool {
        match (&self.last_id, self.last_recorded_at) {
            (Some(last), Some(at)) => *next == *last && now_ms.saturating_sub(at) <= self.window_ms,
            _ => false,
        }
    }

    /// Update the window state after recording an action.
    pub fn note_recorded(&mut self, id: ActionId, now_ms: u64) {
        self.last_id = Some(id);
        self.last_recorded_at = Some(now_ms);
    }

    /// Reset the window (e.g. after an explicit save mark).
    pub fn reset(&mut self) {
        self.last_id = None;
        self.last_recorded_at = None;
    }
}

impl Default for CoalesceWindow {
    fn default() -> Self {
        Self::default_500ms()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> ActionId {
        ActionId::new(s)
    }

    #[test]
    fn no_coalesce_before_first_record() {
        let w = CoalesceWindow::default_500ms();
        assert!(!w.should_coalesce(&id("foo"), 1000));
    }

    #[test]
    fn coalesces_same_id_within_window() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        // 300 ms later, same id → should coalesce
        assert!(w.should_coalesce(&id("foo"), 1300));
    }

    #[test]
    fn does_not_coalesce_same_id_outside_window() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        // 600 ms later → outside window
        assert!(!w.should_coalesce(&id("foo"), 1600));
    }

    #[test]
    fn does_not_coalesce_different_id() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        // Different id, same time
        assert!(!w.should_coalesce(&id("bar"), 1000));
    }

    #[test]
    fn coalesces_exactly_at_boundary() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        // Exactly 500 ms later → within window (<=)
        assert!(w.should_coalesce(&id("foo"), 1500));
    }

    #[test]
    fn does_not_coalesce_one_ms_past_boundary() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        // 501 ms later → outside window
        assert!(!w.should_coalesce(&id("foo"), 1501));
    }

    #[test]
    fn reset_clears_state() {
        let mut w = CoalesceWindow::default_500ms();
        w.note_recorded(id("foo"), 1000);
        w.reset();
        assert!(!w.should_coalesce(&id("foo"), 1200));
    }
}
