# Wave W20 — script-bench

> Self-contained agent dispatch. Phase 3.4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §5.6 ("fastest" benchmark suite).

## Goal

Scaffold the benchmark suite. Harness, native-Rust baseline, output format. Real workloads land later (post-W04 + script-host).

## Crate owned

`crates/script-bench`.

## Files this wave touches

```
crates/script-bench/src/{lib.rs, native_baseline.rs, workloads.rs, output.rs}
crates/script-bench/benches/{script_tick_1m.rs, cold_start.rs, memory_overhead.rs, hot_reload_swap.rs}
crates/script-bench/BASELINE.md
crates/script-bench/METHODOLOGY.md
```

## Stubs needed

- `criterion` workspace dep.
- `wasmtime` workspace dep.
- `runtime-wasmtime-engine` (W04) for engine instantiation in benchmarks — local stub.

## Implementation order

1. `workloads.rs` — workload definitions per PLAN.md §5.6:
   - **W1** `script_tick_1m_iters` — tight loop: `Transform.translation += dt * v`, 1M iterations.
   - **W2** `per_frame_tick_10k_entities` — iterate 10k entities; mutate component; compare to native Rust baseline.
   - **W3** `cold_start` — module load + ready-to-tick latency.
   - **W4** `hot_reload_swap` — measure swap latency over 100 cycles.
   - **W5** `memory_overhead` — resident memory per loaded script module.
2. `native_baseline.rs` — pure-native-Rust reference for each workload (the "1.5× of native" target denominator).
3. `output.rs` — JSON output format for CI pipeline ingestion + Markdown summary for `BASELINE.md`.
4. Harness: criterion benches that record results to `BASELINE.md`.
5. `METHODOLOGY.md` — how each workload is constructed, what's being measured, reproducer instructions.
6. **No comparison-vs-others** at v0.0.1. That's integration-phase work after W04 produces real wasmtime runs.

## Rustforge prior art (steal-and-adapt)

(none specific — rustforge has no scripting benchmark suite). Greenfield.

## Exit criteria

- `cargo bench -p rge-script-bench` runs and produces a number per workload.
- `BASELINE.md` has the baseline results (native-Rust only at this stage).
- `METHODOLOGY.md` documents reproducer instructions.
- No regressions: re-running benchmarks produces the same number ± 5% on the same machine.
- Output JSON consumable by CI gate per §13.3.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single benchmark suite; methodology + reproducer published to defend the "fastest" claim against cherry-picked counter-benches (per §14 risk mitigation).

## Handoff

After merge: integration phase wires real wasmtime/Cranelift workloads (post-W04). Post-Phase-3, comparison vs Lua/mlua/Wasmer-singlepass/Bevy-extism added per PLAN.md §5.6.

## Critical context

This wave produces the **infrastructure** for the "fastest script engine" pillar's verification. **The actual numbers come from Phase 3 hot-reload validation** (after `script-host` exists and W04 is real). If those numbers don't meet PLAN.md §5.6 targets, ADR-077 escape clause activates. This wave just builds the harness.
