//! [`FixedStepAccumulator`] — Glenn Fiedler-style fixed-timestep accumulator.

/// Accumulator for fixed-timestep sim per Glenn Fiedler's "Fix Your Timestep!"
/// pattern: variable-rate frames feed delta-time in; each frame extracts `n`
/// discrete fixed steps and a leftover.
///
/// # Example
///
/// ```rust
/// use rge_kernel_app::FixedStepAccumulator;
///
/// let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
///
/// // Normal 60 Hz frame — typically 0 or 1 step.
/// let steps = acc.advance(1.0 / 60.0);
/// assert!(steps <= 1);
/// assert!(acc.alpha() < 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct FixedStepAccumulator {
    fixed_dt: f64,
    accumulator: f64,
    max_steps_per_frame: u32,
}

impl FixedStepAccumulator {
    /// Construct a new accumulator.
    ///
    /// # Parameters
    ///
    /// * `fixed_dt` — seconds per fixed step (e.g. `1.0 / 60.0` for 60 Hz).
    ///   Must be strictly positive and less than 1 second.
    /// * `max_steps_per_frame` — safety cap; prevents the death-spiral where
    ///   each frame takes longer than the previous one. Must be ≥ 1.
    ///
    /// # Panics
    ///
    /// Panics when `fixed_dt <= 0.0`, `fixed_dt >= 1.0`, or
    /// `max_steps_per_frame == 0`.
    #[must_use]
    pub fn new(fixed_dt: f64, max_steps_per_frame: u32) -> Self {
        assert!(fixed_dt > 0.0, "fixed_dt must be > 0; got {fixed_dt}");
        assert!(fixed_dt < 1.0, "fixed_dt must be < 1; got {fixed_dt}");
        assert!(max_steps_per_frame >= 1, "max_steps_per_frame must be >= 1");
        Self {
            fixed_dt,
            accumulator: 0.0,
            max_steps_per_frame,
        }
    }

    /// The fixed delta-time, in seconds.
    #[must_use]
    #[inline]
    pub fn fixed_dt(&self) -> f64 {
        self.fixed_dt
    }

    /// The current accumulator value (leftover after the last `advance` call).
    #[must_use]
    #[inline]
    pub fn accumulator(&self) -> f64 {
        self.accumulator
    }

    /// Advance with a variable-rate `frame_dt`.
    ///
    /// Returns the number of fixed steps to take this frame (capped by
    /// `max_steps_per_frame`). Negative or zero `frame_dt` produces 0 steps
    /// (no panic — callers should clamp at source, but a bad clock shouldn't
    /// crash the loop).
    #[must_use]
    pub fn advance(&mut self, frame_dt: f64) -> u32 {
        if frame_dt > 0.0 {
            self.accumulator += frame_dt;
        }

        // Cap the accumulator to prevent death-spiral: if we can't keep up,
        // we take at most `max_steps_per_frame` steps and discard the rest.
        let max_consume = self.fixed_dt * f64::from(self.max_steps_per_frame);
        if self.accumulator > max_consume {
            self.accumulator = max_consume;
        }

        // SAFETY: accumulator is capped above so ratio is in [0, max_steps].
        // floor() is non-negative; value fits in u32 by construction.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (self.accumulator / self.fixed_dt).floor() as u32;
        let steps = steps.min(self.max_steps_per_frame);

        self.accumulator -= f64::from(steps) * self.fixed_dt;
        // Clamp to zero to prevent tiny negative values from floating-point
        // subtraction errors.
        if self.accumulator < 0.0 {
            self.accumulator = 0.0;
        }

        steps
    }

    /// Interpolation alpha in `[0, 1)` for the leftover after `advance`.
    ///
    /// Use this to blend render state between two sim states:
    /// `render_state = lerp(prev_state, curr_state, alpha)`.
    #[must_use]
    #[inline]
    pub fn alpha(&self) -> f64 {
        // Safe: fixed_dt is asserted > 0 in `new`.
        (self.accumulator / self.fixed_dt).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT_60HZ: f64 = 1.0 / 60.0;

    #[test]
    fn new_starts_at_zero() {
        let acc = FixedStepAccumulator::new(DT_60HZ, 8);
        assert!(acc.accumulator() < f64::EPSILON);
        assert!(acc.alpha() < f64::EPSILON);
    }

    #[test]
    fn advance_exact_frame_yields_one_step() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let steps = acc.advance(DT_60HZ);
        assert_eq!(steps, 1, "one exact frame → one step");
        assert!(acc.accumulator() < DT_60HZ);
    }

    #[test]
    fn advance_double_frame_yields_two_steps() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let steps = acc.advance(DT_60HZ * 2.0);
        assert_eq!(steps, 2);
    }

    #[test]
    fn advance_half_frame_yields_zero_steps() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let steps = acc.advance(DT_60HZ / 2.0);
        assert_eq!(steps, 0);
        // Accumulator carries forward.
        assert!(acc.accumulator() > 0.0);
    }

    #[test]
    fn death_spiral_cap_respected() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 4);
        // Simulate a frame that took 10 seconds — must clamp to max 4 steps.
        let steps = acc.advance(10.0);
        assert_eq!(steps, 4);
    }

    #[test]
    fn alpha_in_range() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let _ = acc.advance(DT_60HZ * 1.5);
        let a = acc.alpha();
        assert!((0.0..1.0).contains(&a), "alpha {a} not in [0, 1)");
    }

    #[test]
    fn alpha_zero_after_exact_consume() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let _ = acc.advance(DT_60HZ);
        // Remainder should be ~0; alpha should be ~0.
        assert!(acc.alpha() < 1e-10);
    }

    #[test]
    fn accumulator_never_exceeds_fixed_dt_after_single_frame() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let _ = acc.advance(DT_60HZ * 0.9);
        assert!(acc.accumulator() < DT_60HZ);
    }

    #[test]
    fn negative_frame_dt_produces_zero_steps() {
        let mut acc = FixedStepAccumulator::new(DT_60HZ, 8);
        let steps = acc.advance(-0.1);
        assert_eq!(steps, 0);
        assert!(acc.accumulator() < f64::EPSILON);
    }

    #[test]
    #[should_panic(expected = "fixed_dt must be > 0")]
    fn new_panics_on_zero_dt() {
        let _ = FixedStepAccumulator::new(0.0, 8);
    }

    #[test]
    #[should_panic(expected = "fixed_dt must be < 1")]
    fn new_panics_on_dt_gte_one() {
        let _ = FixedStepAccumulator::new(1.0, 8);
    }
}
