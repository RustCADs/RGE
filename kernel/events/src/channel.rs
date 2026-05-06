//! [`EventChannel<E>`] — typed FIFO channel for one event type.

use std::collections::VecDeque;

/// Typed FIFO channel for events of a single concrete type `E`.
///
/// Events emitted via [`emit`] accumulate in a *pending* queue. When
/// [`advance_frame`] is called the pending queue is atomically swapped to the
/// *delivered* (current-frame) buffer, which consumers may iterate until the
/// next frame advance. The previous delivered buffer is discarded.
///
/// Frame semantics
/// ---------------
/// - [`emit`] → pushes into *pending*.  Never visible this frame.
/// - [`advance_frame`] → *delivered* ← *pending*; *pending* cleared; frame
///   counter incremented.
/// - [`iter_current`] → iterates *delivered* (i.e., what was pending before
///   the last advance).
///
/// [`emit`]: Self::emit
/// [`advance_frame`]: Self::advance_frame
/// [`iter_current`]: Self::iter_current
pub struct EventChannel<E> {
    /// Events queued during the current frame; not yet visible to consumers.
    pending: VecDeque<E>,
    /// Events that were pending before the last [`advance_frame`]; visible to
    /// consumers during the current frame.
    ///
    /// [`advance_frame`]: Self::advance_frame
    delivered: VecDeque<E>,
    /// Monotonically increasing frame counter. Starts at `0`; incremented by
    /// each call to [`advance_frame`].
    ///
    /// [`advance_frame`]: Self::advance_frame
    frame: u64,
}

impl<E: Clone> EventChannel<E> {
    /// Construct an empty channel at frame 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            delivered: VecDeque::new(),
            frame: 0,
        }
    }

    /// Push one event into the pending queue.
    ///
    /// The event will not be visible via [`iter_current`] until the next call
    /// to [`advance_frame`].
    ///
    /// [`iter_current`]: Self::iter_current
    pub fn emit(&mut self, event: E) {
        self.pending.push_back(event);
    }

    /// Advance to the next frame: move all pending events to the delivered
    /// buffer and increment the frame counter.
    ///
    /// The previous delivered buffer is silently dropped. Ordering is
    /// preserved: events emitted first appear first in [`iter_current`].
    ///
    /// [`iter_current`]: Self::iter_current
    pub fn advance_frame(&mut self) {
        // Replace delivered with pending in-place to reuse allocations where
        // possible, then clear pending.
        std::mem::swap(&mut self.delivered, &mut self.pending);
        self.pending.clear();
        self.frame += 1;
    }

    /// Iterate over the events delivered during this frame (i.e., those that
    /// were pending before the last [`advance_frame`]).
    ///
    /// Returns an empty iterator when no events were pending.
    ///
    /// [`advance_frame`]: Self::advance_frame
    pub fn iter_current(&self) -> impl Iterator<Item = &E> {
        self.delivered.iter()
    }

    /// Number of events available in the current-frame buffer.
    #[must_use]
    pub fn current_len(&self) -> usize {
        self.delivered.len()
    }

    /// Number of events queued for the *next* frame (not yet delivered).
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The current frame index. Starts at `0` and increments with each call to
    /// [`advance_frame`].
    ///
    /// [`advance_frame`]: Self::advance_frame
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Drop all events from both the pending and delivered buffers.
    ///
    /// The frame counter is not reset.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.delivered.clear();
    }
}

impl<E: Clone> Default for EventChannel<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty_at_frame_zero() {
        let ch: EventChannel<i32> = EventChannel::new();
        assert_eq!(ch.frame(), 0);
        assert_eq!(ch.pending_len(), 0);
        assert_eq!(ch.current_len(), 0);
        assert_eq!(ch.iter_current().count(), 0);
    }

    #[test]
    fn emit_goes_to_pending_not_delivered() {
        let mut ch: EventChannel<i32> = EventChannel::new();
        ch.emit(1);
        ch.emit(2);
        assert_eq!(ch.pending_len(), 2);
        assert_eq!(ch.current_len(), 0);
    }

    #[test]
    fn advance_frame_moves_pending_to_delivered() {
        let mut ch: EventChannel<i32> = EventChannel::new();
        ch.emit(10);
        ch.emit(20);
        ch.advance_frame();
        assert_eq!(ch.pending_len(), 0);
        assert_eq!(ch.current_len(), 2);
        let events: Vec<i32> = ch.iter_current().copied().collect();
        assert_eq!(events, [10, 20]);
    }

    #[test]
    fn frame_counter_increments() {
        let mut ch: EventChannel<u8> = EventChannel::new();
        assert_eq!(ch.frame(), 0);
        ch.advance_frame();
        assert_eq!(ch.frame(), 1);
        ch.advance_frame();
        assert_eq!(ch.frame(), 2);
    }

    #[test]
    fn iter_current_is_fifo() {
        let mut ch: EventChannel<&str> = EventChannel::new();
        ch.emit("a");
        ch.emit("b");
        ch.emit("c");
        ch.advance_frame();
        let events: Vec<&&str> = ch.iter_current().collect();
        assert_eq!(events, [&"a", &"b", &"c"]);
    }

    #[test]
    fn clear_drops_both_buffers() {
        let mut ch: EventChannel<i32> = EventChannel::new();
        ch.emit(1);
        ch.advance_frame();
        ch.emit(2);
        ch.clear();
        assert_eq!(ch.pending_len(), 0);
        assert_eq!(ch.current_len(), 0);
    }

    #[test]
    fn previous_delivered_dropped_on_next_advance() {
        let mut ch: EventChannel<i32> = EventChannel::new();
        ch.emit(1);
        ch.advance_frame(); // delivered = [1]
        ch.emit(2);
        ch.advance_frame(); // delivered = [2], old [1] dropped
        let events: Vec<i32> = ch.iter_current().copied().collect();
        assert_eq!(events, [2]);
    }

    #[test]
    fn default_impl_matches_new() {
        let ch: EventChannel<u32> = EventChannel::default();
        assert_eq!(ch.frame(), 0);
        assert_eq!(ch.pending_len(), 0);
        assert_eq!(ch.current_len(), 0);
    }
}
