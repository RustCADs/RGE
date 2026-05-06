//! Integration tests for `rge-kernel-app` — covers all exit-criteria checks
//! from IMPLEMENTATION.md Phase 1.4.

use rge_kernel_app::{AppBuilder, FixedStepAccumulator, FramePhase, FrameStats};
use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};

// ── 1. FramePhase ordering ────────────────────────────────────────────────────

#[test]
fn frame_phase_all_is_sorted_by_discriminant() {
    let discriminants: Vec<u8> = FramePhase::ALL.iter().map(|&p| p as u8).collect();
    let mut sorted = discriminants.clone();
    sorted.sort_unstable();
    assert_eq!(discriminants, sorted);
}

#[test]
fn frame_phase_ordering_matches_spec() {
    assert!(FramePhase::Input < FramePhase::FixedSim);
    assert!(FramePhase::FixedSim < FramePhase::Update);
    assert!(FramePhase::Update < FramePhase::LateUpdate);
    assert!(FramePhase::LateUpdate < FramePhase::StageRender);
    assert!(FramePhase::StageRender < FramePhase::EndFrame);
}

// ── 2. FixedStepAccumulator basic ─────────────────────────────────────────────

#[test]
fn fixed_step_accumulator_60hz_frame_gives_zero_or_one_step() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
    let steps = acc.advance(0.016_f64); // slightly under 1/60
    assert!(steps <= 1, "expected 0 or 1 steps, got {steps}");
}

#[test]
fn fixed_step_accumulator_double_frame_gives_two_steps() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
    // 2 × 1/60 s exactly consumed — should be 2 steps.
    let steps = acc.advance(1.0 / 60.0 * 2.0);
    assert_eq!(steps, 2);
}

#[test]
fn fixed_step_accumulator_never_exceeds_fixed_dt() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
    let _ = acc.advance(0.016);
    assert!(
        acc.accumulator() < 1.0 / 60.0,
        "accumulator ({}) should be < fixed_dt",
        acc.accumulator()
    );
}

#[test]
fn fixed_step_death_spiral_cap() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 4);
    // 10 second hitch — must clamp to 4 steps.
    let steps = acc.advance(10.0);
    assert_eq!(steps, 4, "death-spiral cap not respected");
}

// ── 3. FixedStepAccumulator alpha ─────────────────────────────────────────────

#[test]
fn alpha_in_range_after_advance() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
    let _ = acc.advance(1.0 / 60.0 * 1.5);
    let a = acc.alpha();
    assert!((0.0..1.0).contains(&a), "alpha {a} not in [0, 1)");
}

#[test]
fn alpha_is_ratio_of_accumulator_to_fixed_dt() {
    let mut acc = FixedStepAccumulator::new(1.0 / 60.0, 8);
    let _ = acc.advance(1.0 / 60.0 * 1.5);
    // Expected alpha ≈ 0.5.
    let expected = acc.accumulator() / acc.fixed_dt();
    let diff = (acc.alpha() - expected).abs();
    assert!(
        diff < 1e-12,
        "alpha mismatch: got {}, expected {}",
        acc.alpha(),
        expected
    );
}

// ── 4. App::run_frame — phases in order, context.frame matches ───────────────

#[test]
fn run_frame_invokes_runner_once_per_phase_in_order() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    let mut phases: Vec<FramePhase> = Vec::new();
    app.run_frame(1.0 / 60.0, &mut sink, |phase, _ctx, _| {
        phases.push(phase);
    });
    assert_eq!(&phases, FramePhase::ALL);
}

#[test]
fn run_frame_context_frame_is_correct() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    // Frame 0.
    app.run_frame(1.0 / 60.0, &mut sink, |phase, ctx, _| {
        if phase == FramePhase::Input {
            assert_eq!(ctx.frame, 0);
        }
    });
    // Frame 1.
    app.run_frame(1.0 / 60.0, &mut sink, |phase, ctx, _| {
        if phase == FramePhase::Input {
            assert_eq!(ctx.frame, 1);
        }
    });
    assert_eq!(app.frame(), 2);
}

#[test]
fn run_frame_increments_frame_counter_by_one() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    assert_eq!(app.frame(), 0);
    app.run_frame(1.0 / 60.0, &mut sink, |_, _, _| {});
    assert_eq!(app.frame(), 1);
    app.run_frame(1.0 / 60.0, &mut sink, |_, _, _| {});
    assert_eq!(app.frame(), 2);
}

// ── 5. run_frames(60, 0.016) — simulates ~1 s; frame == 60; steps ≈ 60 ───────

#[test]
fn run_frames_60_one_second_simulation() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    let mut total_fixed_steps: u64 = 0;

    app.run_frames(60, 0.016, &mut sink, |phase, ctx, _| {
        if phase == FramePhase::FixedSim {
            total_fixed_steps += u64::from(ctx.fixed_steps_this_frame);
        }
    });

    assert_eq!(
        app.frame(),
        60,
        "frame counter should be 60 after 60 frames"
    );
    // At 0.016 s per frame and 1/60 s fixed step, we expect ≈60 total steps.
    // Allow ±5 tolerance for floating-point accumulation.
    assert!(
        (55..=65).contains(&total_fixed_steps),
        "expected ~60 total fixed steps, got {total_fixed_steps}"
    );
}

// ── 6. Diagnostics on frame budget overrun ────────────────────────────────────

#[test]
fn budget_overrun_emits_warning_diagnostic() {
    let mut app = AppBuilder::new().frame_budget(0.001).build();
    let mut sink = DiagnosticAggregator::new();
    app.run_frame(0.020, &mut sink, |_, _, _| {});
    let warnings: Vec<_> = sink
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one Warning diagnostic; got: {warnings:?}"
    );
}

#[test]
fn no_diagnostic_when_within_budget() {
    let mut app = AppBuilder::new().frame_budget(1.0).build();
    let mut sink = DiagnosticAggregator::new();
    app.run_frame(0.016, &mut sink, |_, _, _| {});
    assert!(
        sink.is_empty(),
        "unexpected diagnostics: {:?}",
        sink.into_inner()
    );
}

// ── 7. No-allocation hot path (source-level sanity) ────────────────────────────
//
// This cannot be mechanically verified at runtime without a custom allocator,
// so we document the guarantee here and verify it by inspection:
// - FrameContext is stack-allocated (Copy, no heap).
// - FramePhase::ALL is &'static (no allocation).
// - FrameStats uses [f64; 16] (no Vec).
// - phase_runner is a generic closure (monomorphised, no Box).
// - Diagnostic::warning() allocates once per overrun (intentional; overruns
//   are exceptional, not the common case).
//
// The test below just confirms run_frame doesn't panic and returns normally,
// acting as a smoke test for the no-alloc path.

#[test]
fn run_frame_smoke_no_panic() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    for _ in 0..100 {
        app.run_frame(1.0 / 60.0, &mut sink, |_, _, _| {});
    }
    assert_eq!(app.frame(), 100);
}

// ── 8. Stats ring buffer ──────────────────────────────────────────────────────

#[test]
fn stats_ring_buffer_after_16_frames_p99_is_max() {
    let mut stats = FrameStats::default();
    for i in 0..16_u64 {
        #[allow(clippy::cast_precision_loss)]
        stats.record(i, 0.001 * (i + 1) as f64, 1);
    }
    let expected_max = 0.001 * 16.0;
    let diff = (stats.p99_frame_dt() - expected_max).abs();
    assert!(
        diff < 1e-10,
        "p99 {} ≠ expected max {}",
        stats.p99_frame_dt(),
        expected_max
    );
}

#[test]
fn stats_ring_buffer_after_32_frames_reflects_last_16_only() {
    let mut stats = FrameStats::default();
    // First 16 frames: 100ms each.
    for i in 0..16_u64 {
        stats.record(i, 0.1, 1);
    }
    // Next 16 frames: 16ms each.
    for i in 16..32_u64 {
        stats.record(i, 0.016, 1);
    }
    // The ring should now only contain 0.016 values.
    assert!(
        stats.p99_frame_dt() < 0.02,
        "ring buffer didn't overwrite old values; p99 = {}",
        stats.p99_frame_dt()
    );
}

// ── 9. Bench-like sanity: 10000 frames wall time < 100ms ─────────────────────

#[test]
#[ignore = "slow; enable manually on fast hardware"]
fn ten_thousand_frames_under_100ms() {
    let mut app = AppBuilder::new().build();
    let mut sink = ();
    let start = std::time::Instant::now();
    app.run_frames(10_000, 1.0 / 60.0, &mut sink, |_, _, _| {});
    let elapsed = start.elapsed();
    assert_eq!(app.frame(), 10_000);
    assert!(
        elapsed.as_millis() < 100,
        "10 000 frames took {}ms (budget: 100ms)",
        elapsed.as_millis()
    );
}
