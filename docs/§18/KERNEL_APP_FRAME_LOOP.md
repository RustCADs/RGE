# KERNEL_APP_FRAME_LOOP

| Companion to | PLAN.md §6 (frame loop / runtime heartbeat) + PLAN.md §1.5.2 (render-tier separation, snapshot-driven render thread) + IMPLEMENTATION.md Phase 1.4 (kernel-app exit criteria) |
|---|---|
| Status | Stable v1; 44 tests passing (7 app + 11 fixed-step + 4 frame + 4 phase unit tests + 18 integration tests in `kernel/app/tests/main_loop_test.rs`, one of which is `#[ignore]`-gated for fast-hardware-only); allocation-free hot path verified by inspection |
| Audience | Every Tier-2 author needing frame-phase ordering + sim/render separation + every plugin-host orchestrator + future winit / wgpu integrators threading window events into the loop |
| Sibling doc | `KERNEL_PLUGIN_HOST_LIFECYCLE.md` — host orchestration runs on top of `App`; `init_all` typically fires before the first `run_frame`, `tick_all` typically fires inside the orchestrator's `phase_runner` closure during the `Update` phase, `shutdown_all` typically fires after the loop terminates; `KERNEL_DIAGNOSTICS.md` — frame-budget overrun emits a `Warning` `Diagnostic` per §8 |
| Reference impls | `kernel/app/src/{lib,app,fixed_step,frame,phase}.rs` (substrate) · `kernel/app/tests/main_loop_test.rs` (integration) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the main-loop driver substrate; subsystem-specific phase ordering (e.g. cad-projection's lazy-recompute scheduling, gfx's render-snapshot staging timing) belongs in their sibling §18 docs.

## 1. Why a substrate

Without one, every subsystem-with-an-update-loop would re-invent the same pieces: a fixed-timestep accumulator (Fiedler), a frame counter, a phase-ordering mechanism, a budget-overrun diagnostic emit. Three months in, the workspace would have N parallel loops with subtly different fixed-dt semantics — a sim running at 60 Hz here, 59.94 there, 60.001 in a third place — and "the engine is at frame 1234" would mean different things in different code paths.

PLAN §6 commits to **one canonical `App` + frame loop** as the runtime heartbeat: ordered phases, fixed-timestep sim, variable-rate update, diagnostic hooks for budget overruns, deterministic frame counter. Render and sim threads must agree on frame boundaries (cross-ref `PIE_SNAPSHOT.md` for the snapshot-staging downstream — `StageRender` is the phase that captures the immutable render-side payload — and `GFX_RENDER_TIER.md` for the render-thread consumer).

The substrate's three load-bearing properties:

- **Allocation-free steady state.** No `Vec` / `String` / `Box` in the hot path after construction. Ring buffers, fixed slices, generic (monomorphised) closures. PLAN §6's 16.67 ms / frame budget at 60 Hz cannot tolerate per-frame allocator jitter.
- **No `tokio`, no `winit`.** This crate is the loop *driver*, not the platform binding. Window events flow in via a callback boundary the owner supplies. This decoupling lets tests run the loop without a window and lets the future winit binding sit on top without restructuring.
- **Deterministic per-frame state.** `FrameContext::frame` is monotonic; `FixedStepAccumulator` produces a deterministic `(steps, alpha)` pair given a deterministic `frame_dt` sequence; `FrameStats` ring-buffer is purely additive. Replay-Stable v1.0 (PLAN §1.6.8) builds on this property.

## 2. `FramePhase` enum

Lives at `kernel/app/src/phase.rs`. Six ordered phases:

```rust
#[repr(u8)]
pub enum FramePhase {
    Input        = 0,    // drain input + queued events
    FixedSim     = 1,    // fixed-timestep sim (0..N times per frame)
    Update       = 2,    // variable-rate update (per-frame logic, animation interp)
    LateUpdate   = 3,    // late update (camera follow, post-physics anchoring)
    StageRender  = 4,    // render-snapshot staging (Phase-5 placeholder)
    EndFrame     = 5,    // diagnostics flush, frame counter advance
}
```

> **Source-truth flag:** the dispatch spec speculatively listed phases as `PreFixed / Fixed / PostFixed / Render / FrameEnd`. Source-truth: `Input / FixedSim / Update / LateUpdate / StageRender / EndFrame` (six phases, not five; the conceptual mapping is Input → "PreFixed", FixedSim → "Fixed", Update + LateUpdate → "PostFixed", StageRender → "Render", EndFrame → "FrameEnd"). This doc reflects the actual surface.

Discriminant values are stable; do NOT renumber them. New phases must be inserted with a fresh, strictly increasing discriminant and added to `FramePhase::ALL` in the correct position. The four phase-tests (`all_is_sorted_by_discriminant`, `ordering_matches_spec`, `label_is_nonempty`, `all_contains_six_phases`) regression-pin the canonical ordering.

### Iteration via `ALL`

```rust
impl FramePhase {
    pub const ALL: &'static [Self] = &[
        Self::Input, Self::FixedSim, Self::Update,
        Self::LateUpdate, Self::StageRender, Self::EndFrame,
    ];
    pub const fn label(self) -> &'static str;
}
```

Callers MUST iterate `FramePhase::ALL` (not a manually-constructed list) so a future phase insertion is automatically picked up everywhere. `App::run_frame` invokes the user-supplied `phase_runner` closure once per phase in `ALL` order.

## 3. `FixedStepAccumulator` (Fiedler pattern)

Lives at `kernel/app/src/fixed_step.rs`. Implements Glenn Fiedler's "Fix Your Timestep!" pattern (https://gafferongames.com/post/fix_your_timestep/):

```rust
pub struct FixedStepAccumulator {
    fixed_dt: f64,
    accumulator: f64,
    max_steps_per_frame: u32,
}
```

### Algorithm

Each frame: `advance(frame_dt)` adds `frame_dt` to the accumulator, then extracts as many whole `fixed_dt` steps as fit:

```rust
self.accumulator += frame_dt;             // (clamped: > 0)
// Death-spiral cap: clamp accumulator to max_steps * fixed_dt
let max_consume = self.fixed_dt * f64::from(self.max_steps_per_frame);
if self.accumulator > max_consume { self.accumulator = max_consume; }
let steps = (self.accumulator / self.fixed_dt).floor() as u32;
let steps = steps.min(self.max_steps_per_frame);
self.accumulator -= f64::from(steps) * self.fixed_dt;
steps
```

The leftover (`< fixed_dt`) carries to the next frame. `alpha()` returns the leftover-as-fraction-of-fixed_dt, in `[0, 1)`, for sim-state interpolation between the most recent two simulated states.

### Death-spiral cap

The accumulator is clamped to `max_steps_per_frame * fixed_dt` BEFORE step extraction. This prevents the spiral-of-death where a heavy frame produces N steps, each step takes longer than `fixed_dt`, the accumulator accrues faster than it drains, and the next frame produces N+k steps. With the cap, a 10-second frame hitch consumes at most `max_steps_per_frame` steps; the residual time is **discarded** (sim falls behind wall clock, but the loop survives). The `death_spiral_cap_respected` regression test pins this with `advance(10.0)` → 4 steps under `max_steps_per_frame = 4`.

### Defaults

`AppBuilder::new()` defaults: `fixed_dt = 1.0/60.0`, `max_fixed_steps = 8`, `frame_budget_sec = 1.0/60.0`. `new` panics on `fixed_dt <= 0.0`, `fixed_dt >= 1.0`, or `max_steps_per_frame == 0` (the three `should_panic` regression tests pin the bounds).

## 4. `App` + `AppBuilder`

Lives at `kernel/app/src/app.rs`. Fluent-builder pattern (NOT typestate):

```rust
let app = AppBuilder::new()
    .fixed_dt(1.0 / 120.0)
    .max_fixed_steps(16)
    .frame_budget(1.0 / 30.0)
    .build();
```

> **Source-truth flag:** the dispatch spec speculatively described `AppBuilder::add_system(phase, system)` and `App::run()`. Source-truth: there is NO `add_system` method (systems are not stored on `App`; the user supplies them via the per-frame `phase_runner` closure) and NO `run()` method (the caller drives the loop via `run_frame` / `run_frames`). This doc reflects the actual surface.

### `App` shape

```rust
pub struct App {
    fixed_step: FixedStepAccumulator,
    stats: FrameStats,
    frame_counter: u64,
    frame_budget_sec: f64,
}
```

### `App::run_frame` — the per-frame entry point

```rust
pub fn run_frame<F>(
    &mut self,
    frame_dt: f64,
    sink: &mut dyn DiagnosticSink,
    mut phase_runner: F,
) where F: FnMut(FramePhase, &FrameContext, &mut dyn DiagnosticSink);
```

The four-step flow:

1. `fixed_step.advance(frame_dt)` produces `fixed_steps_this_frame`; `fixed_step.alpha()` produces the interpolation alpha.
2. Build a stack-only `FrameContext { frame, frame_dt, fixed_steps_this_frame, fixed_alpha }`.
3. Invoke `phase_runner(phase, &ctx, sink)` for each phase in `FramePhase::ALL` order.
4. Update `FrameStats`, increment `frame_counter`, emit `Diagnostic::warning("frame budget exceeded")` if `frame_dt > frame_budget_sec`.

The user-supplied `phase_runner` is the per-system dispatch point. A typical orchestrator pattern:

```rust
app.run_frame(frame_dt, &mut sink, |phase, ctx, sink| {
    match phase {
        FramePhase::Input      => input_drain(ctx, sink),
        FramePhase::FixedSim   => for _ in 0..ctx.fixed_steps_this_frame { sim_step(sink) },
        FramePhase::Update     => update(ctx, sink),
        FramePhase::LateUpdate => late_update(ctx, sink),
        FramePhase::StageRender => stage_render(ctx, sink),
        FramePhase::EndFrame    => {} // diagnostics flush handled inside App
    }
});
```

`run_frames(n, frame_dt, sink, phase_runner)` is the synthetic-loop variant for tests + benchmarks: runs `n` frames at constant `frame_dt`. The `run_frames_60_one_second_simulation` integration test pins the property — 60 frames at 0.016 s each → frame counter at 60, fixed steps in `[55, 65]` (allowing ±5 for floating-point accumulation).

## 5. `FrameContext`

Lives at `kernel/app/src/frame.rs`. Per-frame info passed to the `phase_runner`:

```rust
#[derive(Debug, Clone, Copy)]
pub struct FrameContext {
    pub frame: u64,
    pub frame_dt: f64,
    pub fixed_steps_this_frame: u32,
    pub fixed_alpha: f64,
}
```

`Copy` so passing by reference into the closure doesn't allocate; the entire struct is stack-resident. `frame` matches `FrameStats::frame` for the most-recently-completed frame; `fixed_alpha` is in `[0, 1)`.

## 6. `FrameStats` ring buffer

```rust
pub struct FrameStats {
    pub frame: u64,
    pub last_frame_dt: f64,
    pub last_fixed_steps: u32,
    pub p99_frame_dt_window: [f64; 16],
    pub ring_idx: usize,
}
```

> **Source-truth flag:** the dispatch spec described "`[f64; 16]` (or whatever per source-truth)". Source-truth confirms `[f64; 16]` exactly. The 16-slot ring at 60 Hz spans ~267 ms — long enough to smooth out a single bad frame, short enough that the latest spike still surfaces in `p99_frame_dt()`.

`record(frame, frame_dt, fixed_steps)` advances `ring_idx` (modulo 16) and writes `frame_dt` into the slot. Two read APIs:

- `p99_frame_dt()` — returns the **maximum** value in the ring buffer (approximates the 99th percentile for a 16-sample window). Used by editor-ui for spike detection.
- `average_frame_dt()` — returns the mean over non-zero slots. Used for FPS readouts. Returns `0.0` when no frames have been recorded.

The ring is allocation-free: `[f64; 16]` is a stack-or-struct array; `record` performs only arithmetic + array writes; both readers iterate the array in-place. The `stats_ring_buffer_after_32_frames_reflects_last_16_only` regression test pins the wrap-around: after 16 × 0.1 s frames followed by 16 × 0.016 s frames, `p99_frame_dt() < 0.02` (the older 0.1 s entries are gone).

## 7. 60 Hz allocation-free steady state

Per the `kernel/app/src/lib.rs` module-doc design point: **no allocations after warmup**. Verified by inspection (the integration test `run_frame_smoke_no_panic` is a smoke check, not a mechanical allocation gate — see the comment block in `main_loop_test.rs` §7):

- `FrameContext` is `Copy`, stack-allocated.
- `FramePhase::ALL` is `&'static [Self; 6]` — no allocation, no per-frame copy.
- `FrameStats` uses `[f64; 16]` (no `Vec`).
- `phase_runner` is a generic closure (monomorphised at the `run_frame` call site, no `Box<dyn FnMut>`).
- `Diagnostic::warning(...)` allocates **once per overrun** (intentional; overruns are exceptional, not the common case).

Why this matters: PLAN §6's frame-loop budget is 16.67 ms / frame at 60 Hz. Per-frame allocator jitter (a single `malloc` can be hundreds of microseconds on a paged-out arena) destroys the budget. The substrate's commitment is "zero heap traffic in the steady-state hot path"; budget-overrun diagnostics are the only allocation, and they fire only when the budget was already missed.

## 8. Diagnostics integration

Frame-budget anomalies emit a structured `Diagnostic` via the `&mut dyn DiagnosticSink` the caller passes to `run_frame`:

```rust
if frame_dt > self.frame_budget_sec {
    sink.emit(
        Diagnostic::warning("frame budget exceeded")
            .with_span(rge_kernel_diagnostics::Span::new()),
    );
}
```

The `Severity::Warning` choice is deliberate (cross-ref `KERNEL_DIAGNOSTICS.md` §3): a single overrun frame is not a hard error — the loop carries on; the user / orchestrator can treat the warning as a hint to investigate. The `budget_overrun_emits_warning_diagnostic` and `no_diagnostic_when_within_budget` regression tests pin the condition. The `()` unit type implements `DiagnosticSink` as a no-op, so tests that don't care about diagnostics can pass `&mut ()`.

Per-frame `phase_runner` systems can emit their own diagnostics by writing to the same `sink` parameter — the closure receives `&mut dyn DiagnosticSink` so plugin-host failures, render-stage cache-misses, sim-step assertions, and physics integration errors all converge on one stream.

## 9. Failure class — recoverable

`kernel/app/src/lib.rs` line 3 declares:

```rust
//! Failure class: recoverable
```

Per PLAN §1.13. Frame-loop hiccups recover next frame: a budget overrun warns and continues; a fixed-step death-spiral discards residual time and continues; a `phase_runner` that emits errors does not stop the loop (the orchestrator decides whether to keep going). Only deadlocks / hangs would be kernel-fatal — but those would manifest in the schedule layer (`kernel/schedule`, future) rather than in `kernel/app`, which is purely a driver.

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/app` does not appear in `tools/architecture-lints/exemptions.toml`.

## 10. Plugin-host integration pattern

`KERNEL_PLUGIN_HOST_LIFECYCLE.md` describes how plugins fit into the loop. Summary:

- `PluginHost::init_all(&mut ctx)` typically fires once, before the first `App::run_frame`. Returns an `InitReport` with success / failure parallel lists.
- `PluginHost::tick_all(&mut ctx)` typically fires inside the `phase_runner` closure during `FramePhase::Update`. Per-frame; returns a `TickReport`.
- `PluginHost::shutdown_all(&mut ctx)` typically fires once, after the loop terminates (or when the user requests engine shutdown). LIFO order; returns a `ShutdownReport`.

The `App` substrate doesn't know about plugins — it's purely a driver. The orchestrator (a future `runtime/orchestrator` crate) wires `PluginHost` into the `phase_runner` closure. That decoupling is intentional: tests for `App` don't need a plugin host; tests for `PluginHost` don't need a frame loop.

## 11. Render-thread separation

PLAN §1.5.2 commits to a render-tier separation: sim runs on the main thread; render runs on a separate thread; they communicate via an immutable `PieSnapshot` produced at `FramePhase::StageRender`. `App` is the substrate that fires the `StageRender` phase at the right moment — after `Update` / `LateUpdate` (so the snapshot reflects the latest sim state) but before `EndFrame` (so frame-counter advance happens after the snapshot is staged).

Today `StageRender` is a Phase-5 placeholder per the source's phase doc-comment. When the render-tier substrate lands, the snapshot capture happens inside the user-supplied `phase_runner` for `FramePhase::StageRender`, and the render thread reads from a snapshot ring (the `PIE_SNAPSHOT.md` and `GFX_RENDER_TIER.md` companions cover that consumer surface).

## 12. References

- **PLAN.md §6** — frame loop / runtime heartbeat (full design).
- **PLAN.md §1.5.2** — render-tier separation; the snapshot-driven render thread that `FramePhase::StageRender` produces for.
- **PLAN.md §1.6.8** — Replay-Stable v1.0; deterministic frame counter + deterministic `(steps, alpha)` from a deterministic `frame_dt` sequence.
- **PLAN.md §1.13** — failure-class taxonomy; recoverable definition.
- **IMPLEMENTATION.md Phase 1.4** — kernel-app exit criteria; the integration tests in `main_loop_test.rs` cover each criterion.
- **Glenn Fiedler, "Fix Your Timestep!"** — https://gafferongames.com/post/fix_your_timestep/ — the canonical reference for the accumulator-based fixed-step pattern `FixedStepAccumulator` implements.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; plugin-host orchestration runs on top of `App` per §10.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; `Severity::Warning` for budget overruns, `DiagnosticSink` trait the loop emits to.
- **`PIE_SNAPSHOT.md`** — sibling §18 doc; the render-snapshot the `StageRender` phase will produce when the render-tier substrate lands.
- **`GFX_RENDER_TIER.md`** — sibling §18 doc; render-thread consumer of `PieSnapshot`.
- **`kernel/app/src/lib.rs`** — module roots + failure-class declaration + design-point list.
- **`kernel/app/src/phase.rs`** — `FramePhase` enum + `ALL` slice + 4 unit tests pinning ordering / discriminant stability.
- **`kernel/app/src/fixed_step.rs`** — `FixedStepAccumulator` + 11 unit tests pinning `advance` semantics, alpha range, death-spiral cap, panic bounds.
- **`kernel/app/src/frame.rs`** — `FrameContext` + `FrameStats` + 4 unit tests pinning ring-buffer wrap.
- **`kernel/app/src/app.rs`** — `App` + `AppBuilder` + `run_frame` + `run_frames` + 7 unit tests pinning frame counter + phase order + budget overrun.
- **`kernel/app/tests/main_loop_test.rs`** — 18 integration tests covering Phase 1.4 exit criteria.
- **`kernel/diagnostics/src/sink.rs`** — `DiagnosticSink` trait + `()` no-op impl that the loop's tests rely on.
