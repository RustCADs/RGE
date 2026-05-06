//! [`FrameContext`] and [`FrameStats`] — per-frame data and diagnostics.

/// Per-frame info passed to systems through the `phase_runner` closure.
///
/// This struct is stack-allocated and passed by reference; no heap allocation
/// occurs in the hot path.
#[derive(Debug, Clone, Copy)]
pub struct FrameContext {
    /// Monotonic frame counter (starts at 0, increments by 1 per
    /// [`App::run_frame`][crate::App::run_frame] call).
    pub frame: u64,
    /// Wall-clock seconds elapsed since the previous frame.
    pub frame_dt: f64,
    /// Number of fixed-timestep sim steps executed this frame (0..=N, capped
    /// by [`FixedStepAccumulator::max_steps_per_frame`][crate::FixedStepAccumulator]).
    pub fixed_steps_this_frame: u32,
    /// Interpolation alpha in `[0, 1)` — fraction of a fixed step that was
    /// left over after consuming whole steps.
    pub fixed_alpha: f64,
}

/// Rolling statistics accumulated by [`App`][crate::App] for diagnostics.
///
/// Uses a fixed-size ring buffer (`[f64; 16]`) so there are no heap
/// allocations after construction.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    /// Current monotonic frame counter (same as `FrameContext::frame` on the
    /// last completed frame).
    pub frame: u64,
    /// `frame_dt` from the most recently completed frame.
    pub last_frame_dt: f64,
    /// Fixed-step count from the most recently completed frame.
    pub last_fixed_steps: u32,
    /// Ring buffer of the last 16 `frame_dt` values.
    ///
    /// Index `ring_idx` is the slot that was written most recently.
    pub p99_frame_dt_window: [f64; 16],
    /// Write cursor into `p99_frame_dt_window` (wraps modulo 16).
    pub ring_idx: usize,
}

impl FrameStats {
    /// Record one completed frame into the rolling statistics.
    pub fn record(&mut self, frame: u64, frame_dt: f64, fixed_steps: u32) {
        self.frame = frame;
        self.last_frame_dt = frame_dt;
        self.last_fixed_steps = fixed_steps;

        self.ring_idx = self.ring_idx.wrapping_add(1) % 16;
        self.p99_frame_dt_window[self.ring_idx] = frame_dt;
    }

    /// Approximate p99 frame time — returns the maximum value in the ring
    /// buffer (the window holds ≤ 16 samples, so `max` approximates the
    /// 99th percentile for a 60 Hz loop's ≈267 ms window).
    #[must_use]
    pub fn p99_frame_dt(&self) -> f64 {
        self.p99_frame_dt_window
            .iter()
            .copied()
            .fold(0.0_f64, f64::max)
    }

    /// Mean frame time over the ring buffer (non-zero slots only).
    ///
    /// Returns 0.0 when no frames have been recorded yet.
    #[must_use]
    pub fn average_frame_dt(&self) -> f64 {
        let sum: f64 = self.p99_frame_dt_window.iter().copied().sum();
        let count = self
            .p99_frame_dt_window
            .iter()
            .filter(|&&v| v > 0.0)
            .count();
        if count == 0 {
            0.0
        } else {
            // count is at most 16; cast is lossless.
            #[allow(clippy::cast_precision_loss)]
            let count_f = count as f64;
            sum / count_f
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stats_are_zero() {
        let s = FrameStats::default();
        assert_eq!(s.frame, 0);
        assert!(s.last_frame_dt < f64::EPSILON);
        assert!(s.p99_frame_dt() < f64::EPSILON);
        assert!(s.average_frame_dt() < f64::EPSILON);
    }

    #[test]
    fn record_updates_fields() {
        let mut s = FrameStats::default();
        s.record(7, 0.016, 1);
        assert_eq!(s.frame, 7);
        assert!((s.last_frame_dt - 0.016).abs() < 1e-12);
        assert_eq!(s.last_fixed_steps, 1);
    }

    #[test]
    fn p99_returns_max_of_ring() {
        let mut s = FrameStats::default();
        for i in 0..16_u64 {
            #[allow(clippy::cast_precision_loss)]
            s.record(i, 0.01 * (i + 1) as f64, 1);
        }
        // Slot written is the maximum we inserted.
        let max = 0.01 * 16.0;
        assert!((s.p99_frame_dt() - max).abs() < 1e-10);
    }

    #[test]
    fn ring_wraps_after_16_frames() {
        let mut s = FrameStats::default();
        // First 16 frames: all 0.1 s.
        for i in 0..16_u64 {
            s.record(i, 0.1, 1);
        }
        // Next 16 frames: all 0.016 s — overwrites the previous values.
        for i in 16..32_u64 {
            s.record(i, 0.016, 1);
        }
        // p99 should now reflect only the last 16 frames.
        assert!(s.p99_frame_dt() < 0.02, "ring didn't wrap correctly");
    }
}
