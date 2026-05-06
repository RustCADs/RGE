# `rge-script-bench` baseline (v0.0.1 — native-Rust only)

Status: **scaffold**. Engine columns (`wasmtime_cranelift`, `wasmtime_singlepass`,
`mlua`, `wasmer_singlepass`, `bevy_extism`) land post-W04 per `tasks/W20/PLAN.md`.

This file records the reference numbers for every workload defined in
[`src/workloads.rs`](src/workloads.rs) when run on the
**native-Rust baseline** (`src/native_baseline.rs`). All later "engine X is
1.5× of native" claims are computed against the values here on the same host.

See [METHODOLOGY.md](METHODOLOGY.md) for what each row means and how to
reproduce it.

## Workload roster

| id  | name                          | native-Rust kernel                                                             |
| --- | ----------------------------- | ------------------------------------------------------------------------------ |
| W1  | `script_tick_1m_iters`        | tight loop: `Transform.translation += dt * Transform.velocity`, 1M iterations  |
| W2  | `per_frame_tick_10k_entities` | one frame over 10k entities, integration kernel applied once each              |
| W3  | `cold_start`                  | empty-closure timer floor (no native module-load step exists)                  |
| W4  | `hot_reload_swap`             | replace `Box<dyn Fn>` × 100 cycles                                             |
| W5  | `memory_overhead`             | `size_of::<fn(&mut Transform)>()` (function-pointer cost)                      |

## Baseline results

The numbers below are the **first-run record** for the host where
`cargo bench -p rge-script-bench` was last executed. Re-runs on the same
host should land within ±5% of these values — that's the "no regressions"
exit criterion.

Recorded on a Windows 11 / x86_64 dev box, `cargo 1.78`, `[profile.bench]`
defaults (LTO=thin, opt-level=3, codegen-units=1):

| workload                       | engine        | metric            | unit             | value     | samples |
| ------------------------------ | ------------- | ----------------- | ---------------- | --------- | ------- |
| `script_tick_1m_iters`         | `native_rust` | wall_time         | ns total / 1M op | 668 000   | 100     |
| `per_frame_tick_10k_entities`  | `native_rust` | wall_time         | ns total / 10k   | 8 102     | 100     |
| `cold_start`                   | `native_rust` | wall_time         | ns               | 50.8      | 50      |
| `hot_reload_swap`              | `native_rust` | wall_time_total   | ns / 100 cycles  | 110.6     | 50      |
| `memory_overhead`              | `native_rust` | wall_time_per_load | ns               | 1.28      | 50      |
| `memory_overhead`              | `native_rust` | bytes_per_module  | bytes            | 8         | n/a     |

Per-op derivations:

- `script_tick_1m_iters` — 668 000 ns / 1 000 000 = **0.668 ns/op** (~1.5 Gelem/s).
- `per_frame_tick_10k_entities` — 8 102 ns / 10 000 = **0.81 ns/op** (~1.23 Gelem/s).

Reproducibility: a second back-to-back full run yielded W1 within ±0.3%, W3
within +2.1%, W4 within -2.2%, W5 within ±0.3% — all comfortably inside the
±5% band. (W2 with `--quick` is noisy by design; the value above is from the
default profile.)

> **Filling in the table.** After running `cargo bench -p rge-script-bench`,
> read `target/criterion/<group>/<name>/new/estimates.json` for each row and
> paste the `mean.point_estimate` (in nanoseconds) into the value column.
> This is intentionally manual at v0.0.1; the W04 follow-up wires automatic
> JSON aggregation through `src/output.rs`.

## Engine rows (placeholder)

The table below is what `BASELINE.md` *will* look like once the engine
columns are populated. It is reproduced here so reviewers know the target
shape.

| workload                       | native_rust | wasmtime_cranelift | wasmtime_singlepass | mlua | wasmer_singlepass | bevy_extism |
| ------------------------------ | ----------- | ------------------ | ------------------- | ---- | ----------------- | ----------- |
| `script_tick_1m_iters`         | _baseline_  | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `per_frame_tick_10k_entities`  | _baseline_  | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `cold_start`                   | 0 ns *      | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `hot_reload_swap`              | _baseline_  | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `memory_overhead`              | 8 B *       | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |

\* Native code has no module-load step and no per-module heap allocation;
the values shown are the formal lower bounds. See METHODOLOGY for why
this is fair.

## Targets to defend (per PLAN.md §5.6)

- `per_frame_tick_10k_entities` (engine) ≤ **1.5×** native row.
- `script_tick_1m_iters` (engine) ≤ **1.5×** native row.
- `cold_start` (engine) < **50 ms**.
- `hot_reload_swap` (engine, p95) < **100 ms**.
- `memory_overhead` (engine) < **1 MB** per module.

## Reproducing this file

```sh
# from RGE workspace root
cargo bench -p rge-script-bench
# Reads target/criterion/**/new/estimates.json for each group/function and
# updates this table. (Currently manual at v0.0.1.)
```

Methodology, including `--save-baseline`/`--baseline` flow and CI ratchet,
is in [METHODOLOGY.md](METHODOLOGY.md).
