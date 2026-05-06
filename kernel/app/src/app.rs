//! [`App`] and [`AppBuilder`] — the main-loop driver.

use rge_kernel_diagnostics::{Diagnostic, DiagnosticSink};

use crate::{FixedStepAccumulator, FrameContext, FramePhase, FrameStats};

/// Default frame budget: 1/60 s ≈ 16.67 ms (60 Hz target).
const DEFAULT_FRAME_BUDGET_SEC: f64 = 1.0 / 60.0;

/// Builder for [`App`].
///
/// Start with [`AppBuilder::new`] (which sets sensible 60 Hz defaults) and
/// override individual knobs as needed.
///
/// # Example
///
/// ```rust
/// use rge_kernel_app::AppBuilder;
///
/// let app = AppBuilder::new()
///     .fixed_dt(1.0 / 120.0)
///     .max_fixed_steps(16)
///     .frame_budget(1.0 / 30.0)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct AppBuilder {
    fixed_dt: f64,
    max_fixed_steps: u32,
    frame_budget_sec: f64,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Create a builder with 60 Hz fixed-step, 8 max steps/frame, 60 Hz budget.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fixed_dt: DEFAULT_FRAME_BUDGET_SEC,
            max_fixed_steps: 8,
            frame_budget_sec: DEFAULT_FRAME_BUDGET_SEC,
        }
    }

    /// Override the fixed sim timestep (seconds per step).
    ///
    /// See [`FixedStepAccumulator::new`] for constraints.
    #[must_use]
    pub fn fixed_dt(mut self, dt: f64) -> Self {
        self.fixed_dt = dt;
        self
    }

    /// Override the maximum number of fixed steps per frame.
    #[must_use]
    pub fn max_fixed_steps(mut self, n: u32) -> Self {
        self.max_fixed_steps = n;
        self
    }

    /// Override the frame budget for diagnostic emission.
    ///
    /// When `frame_dt > frame_budget_sec` the [`App`] emits a `Warning`
    /// diagnostic.
    #[must_use]
    pub fn frame_budget(mut self, sec: f64) -> Self {
        self.frame_budget_sec = sec;
        self
    }

    /// Consume the builder and produce an [`App`].
    #[must_use]
    pub fn build(self) -> App {
        App {
            fixed_step: FixedStepAccumulator::new(self.fixed_dt, self.max_fixed_steps),
            stats: FrameStats::default(),
            frame_counter: 0,
            frame_budget_sec: self.frame_budget_sec,
        }
    }
}

/// The main-loop driver.
///
/// Call [`App::run_frame`] once per platform frame tick. The `phase_runner`
/// closure is invoked once per [`FramePhase`] in the canonical order defined
/// by [`FramePhase::ALL`].
///
/// # Allocation policy
///
/// No heap allocations occur in the hot path after construction. The ring
/// buffer in [`FrameStats`] is fixed-size; [`FrameContext`] is stack-only;
/// `phase_runner` is a generic (monomorphised) closure.
pub struct App {
    fixed_step: FixedStepAccumulator,
    stats: FrameStats,
    frame_counter: u64,
    frame_budget_sec: f64,
}

impl App {
    /// Return an [`AppBuilder`] with default settings.
    #[must_use]
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// The fixed sim timestep, in seconds.
    #[must_use]
    #[inline]
    pub fn fixed_dt(&self) -> f64 {
        self.fixed_step.fixed_dt()
    }

    /// The monotonic frame counter (incremented once per [`run_frame`][Self::run_frame] call).
    #[must_use]
    #[inline]
    pub fn frame(&self) -> u64 {
        self.frame_counter
    }

    /// Read-only access to rolling frame statistics.
    #[must_use]
    #[inline]
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    /// Run a single frame.
    ///
    /// The caller provides:
    /// * `frame_dt` — wall-clock seconds elapsed since the last call (variable
    ///   rate; may be 0 on the very first frame).
    /// * `sink` — a [`DiagnosticSink`] that receives any diagnostics emitted
    ///   during this frame (e.g. budget overruns).
    /// * `phase_runner` — a closure invoked once per phase in
    ///   [`FramePhase::ALL`] order. Receives the phase, the frame context, and
    ///   the same `sink` so that systems can emit their own diagnostics.
    ///
    /// # Phase semantics
    ///
    /// 1. [`FixedStepAccumulator::advance`] is called to determine the number
    ///    of fixed steps for this frame.
    /// 2. A [`FrameContext`] is built (stack-only, no allocation).
    /// 3. Each phase in [`FramePhase::ALL`] invokes `phase_runner`.
    /// 4. [`FrameStats::record`] is updated; the frame counter increments; a
    ///    Warning is emitted if `frame_dt > frame_budget_sec`.
    pub fn run_frame<F>(
        &mut self,
        frame_dt: f64,
        sink: &mut dyn DiagnosticSink,
        mut phase_runner: F,
    ) where
        F: FnMut(FramePhase, &FrameContext, &mut dyn DiagnosticSink),
    {
        // 1. Advance the fixed-step accumulator.
        let fixed_steps_this_frame = self.fixed_step.advance(frame_dt);
        let fixed_alpha = self.fixed_step.alpha();

        // 2. Build the (stack-only) frame context.
        let ctx = FrameContext {
            frame: self.frame_counter,
            frame_dt,
            fixed_steps_this_frame,
            fixed_alpha,
        };

        // 3. Invoke the phase runner for each phase in canonical order.
        for &phase in FramePhase::ALL {
            phase_runner(phase, &ctx, sink);
        }

        // 4. Update stats, advance frame counter, emit budget-overrun diagnostic.
        self.stats
            .record(self.frame_counter, frame_dt, fixed_steps_this_frame);
        self.frame_counter += 1;

        if frame_dt > self.frame_budget_sec {
            sink.emit(
                Diagnostic::warning("frame budget exceeded")
                    .with_span(rge_kernel_diagnostics::Span::new()),
            );
        }
    }

    /// Run `n` frames at a synthetic, constant `frame_dt`.
    ///
    /// Useful in tests and benchmarks where a real platform clock is not
    /// available. The same `sink` and `phase_runner` contract as
    /// [`run_frame`][Self::run_frame] applies.
    pub fn run_frames<F>(
        &mut self,
        n: u32,
        frame_dt: f64,
        sink: &mut dyn DiagnosticSink,
        mut phase_runner: F,
    ) where
        F: FnMut(FramePhase, &FrameContext, &mut dyn DiagnosticSink),
    {
        for _ in 0..n {
            self.run_frame(frame_dt, sink, &mut phase_runner);
        }
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};

    use super::*;

    #[test]
    fn builder_default_values() {
        let app = AppBuilder::new().build();
        assert!((app.fixed_dt() - DEFAULT_FRAME_BUDGET_SEC).abs() < 1e-12);
        assert_eq!(app.frame(), 0);
    }

    #[test]
    fn frame_counter_increments() {
        let mut app = AppBuilder::new().build();
        let mut sink = ();
        app.run_frame(1.0 / 60.0, &mut sink, |_, _, _| {});
        assert_eq!(app.frame(), 1);
        app.run_frame(1.0 / 60.0, &mut sink, |_, _, _| {});
        assert_eq!(app.frame(), 2);
    }

    #[test]
    fn phase_runner_called_for_each_phase_in_order() {
        let mut app = AppBuilder::new().build();
        let mut sink = ();
        let mut phases_seen: Vec<FramePhase> = Vec::new();
        app.run_frame(1.0 / 60.0, &mut sink, |phase, _ctx, _| {
            phases_seen.push(phase);
        });
        assert_eq!(phases_seen, FramePhase::ALL);
    }

    #[test]
    fn frame_context_frame_field_matches() {
        let mut app = AppBuilder::new().build();
        let mut sink = ();
        // Run two frames; first frame has frame == 0, second has frame == 1.
        let mut frame_values: Vec<u64> = Vec::new();
        for _ in 0..2 {
            app.run_frame(1.0 / 60.0, &mut sink, |phase, ctx, _| {
                if phase == FramePhase::Input {
                    frame_values.push(ctx.frame);
                }
            });
        }
        assert_eq!(frame_values, vec![0, 1]);
    }

    #[test]
    fn budget_overrun_emits_warning() {
        let mut app = AppBuilder::new().frame_budget(0.001).build();
        let mut sink = DiagnosticAggregator::new();
        app.run_frame(0.020, &mut sink, |_, _, _| {});
        assert!(
            sink.iter().any(|d| d.severity == Severity::Warning),
            "expected a Warning diagnostic for budget overrun"
        );
    }

    #[test]
    fn no_warning_when_within_budget() {
        let mut app = AppBuilder::new().frame_budget(1.0).build();
        let mut sink = DiagnosticAggregator::new();
        app.run_frame(0.016, &mut sink, |_, _, _| {});
        assert!(!sink.has_errors());
        // The warning should NOT have been emitted.
        let warnings: Vec<_> = sink
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect();
        assert!(warnings.is_empty());
    }

    #[test]
    fn run_frames_increments_counter_correctly() {
        let mut app = AppBuilder::new().build();
        let mut sink = ();
        app.run_frames(60, 1.0 / 60.0, &mut sink, |_, _, _| {});
        assert_eq!(app.frame(), 60);
    }
}
