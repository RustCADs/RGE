//! `TimeScale` — game-tick speed multiplier for PIE.
//!
//! Per W03 dispatch: 0.01–4.0× scale slider. **Game systems scale; editor
//! systems do not.** This is a constitutional invariant — editor responsiveness
//! must not depend on simulation speed (PLAN.md constitutional principle #8:
//! editor extends runtime, never replaces).
//!
//! Two timescale "classes":
//!
//! - [`TimeScaleClass::Game`] — game-side `dt` is multiplied by `value`
//! - [`TimeScaleClass::Editor`] — `dt` is passed through unchanged
//!
//! The system-registration layer (W02 `kernel/schedule`) will tag each
//! system with its class; W03 stages the type and the scale value.

/// Which side of the editor/runtime boundary a system runs on. Per
/// constitutional principle #8: editor extends runtime, but editor must
/// not be subject to game time-dilation (gizmos, panel animations,
/// hot-reload watcher, etc. always run at wall-clock speed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeScaleClass {
    /// Subject to time-scale (game systems: physics, anim, scripts, audio).
    Game,
    /// Not subject to time-scale (editor systems: gizmos, panels,
    /// hot-reload, inspector).
    Editor,
}

/// Scaling factor for game-system delta-time. Clamped to a hard range; UI
/// slider in [`crate::play_toolbar`] never lets the user enter values
/// outside the range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale {
    value: f32,
}

impl TimeScale {
    /// Minimum scale. `0.01×` (slow-motion). Lower than this and physics
    /// determinism gets ugly (sub-microsecond `dt`); higher than the max
    /// and integrators alias.
    pub const MIN: f32 = 0.01;

    /// Maximum scale. `4.0×` (fast-forward).
    pub const MAX: f32 = 4.0;

    /// Default (real-time, no dilation).
    pub const DEFAULT: f32 = 1.0;

    /// Construct with `Self::DEFAULT`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: Self::DEFAULT,
        }
    }

    /// Construct with an explicit value, clamped to `[MIN, MAX]`.
    #[must_use]
    pub fn with_value(value: f32) -> Self {
        Self {
            value: value.clamp(Self::MIN, Self::MAX),
        }
    }

    /// Current value.
    #[must_use]
    pub fn value(self) -> f32 {
        self.value
    }

    /// Set a new value, clamped. Returns the *previous* value (for audit
    /// log).
    pub fn set(&mut self, value: f32) -> f32 {
        let previous = self.value;
        self.value = value.clamp(Self::MIN, Self::MAX);
        previous
    }

    /// Apply this scale to a `dt_seconds` for a system of class `class`.
    /// Editor systems get the raw `dt`; game systems get scaled.
    #[must_use]
    pub fn apply(self, dt_seconds: f32, class: TimeScaleClass) -> f32 {
        match class {
            TimeScaleClass::Game => dt_seconds * self.value,
            TimeScaleClass::Editor => dt_seconds,
        }
    }

    /// Convenient nominal "1×" check (within FP tolerance).
    #[must_use]
    pub fn is_real_time(self) -> bool {
        (self.value - Self::DEFAULT).abs() < 1e-6
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_one() {
        let t = TimeScale::default();
        assert!(t.is_real_time());
        assert!((t.value() - 1.0_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn clamps_below_min() {
        let t = TimeScale::with_value(0.0001);
        assert!((t.value() - TimeScale::MIN).abs() < f32::EPSILON);
    }

    #[test]
    fn clamps_above_max() {
        let t = TimeScale::with_value(100.0);
        assert!((t.value() - TimeScale::MAX).abs() < f32::EPSILON);
    }

    #[test]
    fn editor_systems_unaffected() {
        let t = TimeScale::with_value(0.5);
        let dt = 0.016_f32;
        assert!((t.apply(dt, TimeScaleClass::Editor) - dt).abs() < 1e-6);
    }

    #[test]
    fn game_systems_scale() {
        let t = TimeScale::with_value(2.0);
        let dt = 0.016_f32;
        assert!((t.apply(dt, TimeScaleClass::Game) - dt * 2.0).abs() < 1e-6);
    }

    #[test]
    fn set_returns_previous() {
        let mut t = TimeScale::default();
        let prev = t.set(0.25);
        assert!((prev - 1.0_f32).abs() < f32::EPSILON);
        assert!((t.value() - 0.25_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn slow_motion_extreme() {
        let t = TimeScale::with_value(TimeScale::MIN);
        let scaled = t.apply(1.0, TimeScaleClass::Game);
        assert!((scaled - TimeScale::MIN).abs() < 1e-6);
    }
}
