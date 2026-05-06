# `rge-script-bench` methodology

> Companion to [BASELINE.md](BASELINE.md). Defines what each workload measures,
> how it is constructed, and how to reproduce the published numbers. Per
> [PLAN.md §14](../../plans/PLAN.md), the published methodology is the
> defence against cherry-picked counter-benchmarks against the "fastest
> script engine" pillar (PLAN.md §5.6).

## Versioning

This document is versioned implicitly by [`src/workloads.rs`](src/workloads.rs)
constants and [`src/output.rs::SCHEMA_VERSION`](src/output.rs). Bumping a
workload constant (iteration count, entity count, seed, `FIXED_DT`) requires
a methodology bump and a `BASELINE.md` reset on every supported host.

| key                              | value                  | source                                      |
| -------------------------------- | ---------------------- | ------------------------------------------- |
| Schema version                   | 1                      | `output::SCHEMA_VERSION`                    |
| Iteration count (W1)             | 1,000,000              | `workloads::SCRIPT_TICK_ITERATIONS`         |
| Entity count (W2)                | 10,000                 | `workloads::PER_FRAME_ENTITY_COUNT`         |
| Hot-reload cycles (W4)           | 100                    | `workloads::HOT_RELOAD_CYCLES`              |
| Fixed timestep                   | 1 / 60 s               | `workloads::FIXED_DT`                       |
| Entity-buffer seed               | 0x5247_452D_5732_3031  | `workloads::ENTITY_SEED`                    |

## Workload construction

### W1 — `script_tick_1m_iters`

**Kernel.** `Transform.translation += dt * Transform.velocity`, written as
`Transform::integrate(&mut self, dt: f32)`.

**Inputs.** A single `Transform` with `translation = (0, 0, 0)` and
`velocity = (1, 2, 3)`. Constants chosen so the loop body cannot be
constant-folded (compiler cannot prove the result without iterating).

**Loop.** Executed `SCRIPT_TICK_ITERATIONS = 1_000_000` times.

**What is measured.** Wall-clock nanoseconds for the full 1M-iteration loop
returned through `criterion::black_box` so the optimiser cannot eliminate
work.

**What it tells us.** Per-iteration arithmetic throughput in tightest-possible
form. An engine candidate with a per-tick guard, type-check, or sandbox
boundary will lose here proportionally to its overhead.

### W2 — `per_frame_tick_10k_entities`

**Kernel.** Same as W1.

**Inputs.** A `Vec<Transform>` of length `PER_FRAME_ENTITY_COUNT = 10_000`,
populated deterministically by `workloads::generate_entities(count, seed)`
using SplitMix64 with `seed = ENTITY_SEED`. Identical bytes on every host.

**Loop.** One pass over the buffer applying the integration kernel once per
entry. The buffer is owned by the bench harness and `clone`d into each
sample via `criterion::iter_batched(BatchSize::SmallInput)` so the work is
"one frame" each time.

**What is measured.** Wall-clock nanoseconds for one frame's traversal
(per-op throughput is then `time / 10_000`).

**What it tells us.** Realistic-shaped per-frame work for a typical scripted
component update. Engine FFI call cost shows up here.

### W3 — `cold_start`

**Native baseline.** Native Rust has no module-load step; the kernel is
`Instant::now() / black_box(()) / .elapsed()`. The published number is the
floor of measurement noise — the unavoidable overhead an engine cannot drop
below.

**Engine version (post-W04).** Construct a fresh
`runtime_wasmtime_engine::Engine`, instantiate the canonical "tick"
module, run one tick, stop the timer.

**What is measured.** Wall-clock nanoseconds from "no engine yet" to "first
tick has returned". Single-shot per sample (`sample_size = 50`).

**Target.** < 50 ms (PLAN.md §5.6).

### W4 — `hot_reload_swap`

**Native baseline.** `Box<dyn Fn(&mut Transform)>` replaced with another
boxed closure, repeated `HOT_RELOAD_CYCLES = 100` times. The two closures
differ only in `dt` to defeat compiler dead-code elimination.

**Engine version (post-W04).** Build module v1 and module v2 ahead of the
timer. In each cycle: instantiate v2, swap the live module pointer,
quiesce v1, drop v1. Repeat 100 times.

**What is measured.** Total wall-clock nanoseconds for the 100 swap
cycles. p95 derived from criterion's per-iteration distribution.

**Target.** p95 < 100 ms (PLAN.md §5.6).

### W5 — `memory_overhead`

**Native baseline.** `size_of::<fn(&mut Transform)>()` — the smallest
"loaded module" Rust can have. Reported in bytes via
`Criterion::iter_custom`, encoded as a `Duration::from_nanos(bytes)` to
ride the criterion timer infrastructure.

**Engine version (post-W04).** Resident-set-size delta: measure RSS
before any module is loaded; load N copies of the canonical "tick"
module; measure RSS again; report `(after - before) / N`.

**What is measured.** Bytes of resident memory per loaded script module.

**Target.** < 1 MB per module (PLAN.md §5.6).

## Anti-cheat rules

These rules apply to every engine candidate row added in later waves:

1. **Identical input bytes.** The entity buffer for W2 is generated by the
   *exact* SplitMix64 in `workloads::generate_entities`. Engines may not
   substitute their own RNG.
2. **Identical kernel.** `Transform::integrate` is the only permitted
   arithmetic. No SIMD swap, no FMA reorder, no precomputed table.
3. **No `#[inline(always)]` on the kernel.** Native baseline is an honest
   denominator: only the optimiser's own choices apply.
4. **No `unsafe` in either side.** Workspace lints forbid it; the bench
   crate inherits them via `[lints] workspace = true`.
5. **Same machine.** Engine numbers must be quoted alongside native numbers
   from the same `cargo bench` invocation. Never compare an engine number
   on host A to a native number on host B.
6. **Same compiler & profile.** `[profile.bench]` in the workspace root
   pins LTO=thin, opt-level=3, codegen-units=1.

## Reproducer instructions

### One-shot run

```sh
# From the RGE workspace root.
cargo bench -p rge-script-bench
```

This runs every `[[bench]]` declared in `crates/script-bench/Cargo.toml`:

- `script_tick_1m`        — W1 + W2 (one binary, two groups)
- `cold_start`            — W3
- `memory_overhead`       — W5
- `hot_reload_swap`       — W4

Criterion writes per-bench JSON estimates to
`target/criterion/<group>/<name>/new/estimates.json`.

### Stable-comparison flow (for "no regressions ±5%")

```sh
# Record a baseline named "main" once on a clean machine.
cargo bench -p rge-script-bench -- --save-baseline main

# Later, on the same machine:
cargo bench -p rge-script-bench -- --baseline main
# Criterion prints "x.x% faster" / "x.x% slower" deltas per row.
```

The exit criterion in `tasks/W20/PLAN.md` is **±5%** between adjacent runs
on the same host. Any row outside that band is a regression and the cause
must be tracked down before the run is published to `BASELINE.md`.

### CI ingestion (post-W20)

Future work (a `cargo run -p rge-script-bench --bin bench-collect`-style
front-end) will:

1. Run `cargo bench -p rge-script-bench --no-run` to compile.
2. Execute each bench binary with `--measurement-time` and `--noplot`
   tuned for CI runners.
3. Read every `target/criterion/**/estimates.json`.
4. Build a [`BenchReport`](src/output.rs) and emit JSON to
   `target/script-bench-report.json` plus the Markdown table for
   `BASELINE.md`.
5. Fail the build if any row drifts > 5% versus a stored baseline file
   (per PLAN.md §13.3 ratchet rule).

The `BenchReport` schema is already stable at `schema_version = 1` so the
CI front-end is a pure plumbing job.

## What is *not* measured (and why)

- **Cross-language FFI raw cost.** Subsumed by W2; isolating it would
  publish a number with no policy meaning.
- **JIT warmup separately from cold-start.** The engine spec for W3 fixes
  "cold start" to mean "first tick returned"; warmup is part of cold
  start by definition. A separate "steady-state warmed" metric can be
  added in a future schema bump if `BenchReport.metric` grows a new
  variant.
- **Engine compilation cost.** Counted within W3 if it occurs before the
  first tick. AOT-cached engines should report a separate row using
  `metric: "warm_start"` once the schema accommodates it.

## Change log

- **v0.0.1** (this wave) — initial scaffold; native-Rust baseline only;
  engine rows pending W04. JSON schema v1 frozen.
