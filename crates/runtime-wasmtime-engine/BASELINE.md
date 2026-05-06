# W04 — runtime-wasmtime-engine baseline timings

**Wave:** W04 (Phase 3 — CRITICAL — the constitutional WASM bet)
**Date:** 2026-05-05
**Spec:** [W04 dispatch package](../../tasks/W04/PLAN.md), [PLAN.md §5.1](../../plans/PLAN.md#51-one-runtime-wasmtime), [PLAN.md §5.6](../../plans/PLAN.md#56-fastest-benchmark-suite-cratesscript-bench)

## Hello-world fixture

The `tests/fixtures/hello_tick.wat` module exports `tick(dt: f32) -> ()` and
imports `host.host_record_tick(dt: f32)` from the host. Every tick increments
a counter on the `HostState`. The test asserts that two `tick(0.016)` calls
yield `tick_count == 2`.

The module declares `<computes>` in its `rcad-effects` custom section, so it
instantiates against a `CapSet::from_one(Capability::ComputeExec)` ticket.

## Build context

- Toolchain: rustc stable (CI executes against the workspace pin 1.78; tests
  also run cleanly under stable 1.94 in the isolated buildtest).
- Profile: `release` (`cargo test --release`).
- Wasmtime version: 23.0.3 with features `cranelift, runtime, std`.
- Cache: `wasmtime-cache` deliberately disabled for W04 — version 23.0.3 has
  a windows-sys 0.52 incompatibility unrelated to this wave's scope.
- Host: Windows 11, x86_64.

## Observed numbers (5 runs, release)

| Metric | r1 | r2 | r3 | r4 | r5 | median |
|---|---:|---:|---:|---:|---:|---:|
| `Engine::new()` | 60.7 us | 70.4 us | 62.6 us | 69.6 us | 60.5 us | **62.6 us** |
| `Engine::instantiate(...)` (compile + bind + instantiate) | 870.2 us | 1029.9 us | 904.3 us | 1054.8 us | 888.7 us | **904.3 us** |
| Per-tick overhead (avg over 10 000 ticks) | 125 ns | 144 ns | 126 ns | 114 ns | 108 ns | **125 ns** |
| Memory footprint per instance | 64 KiB | 64 KiB | 64 KiB | 64 KiB | 64 KiB | **64 KiB** |

**Memory footprint** is the size of the single exported `(memory 1)` page (1
wasm page = 65 536 bytes = 64 KiB). This is the minimum a wasm module can
declare; production modules will allocate more.

## Spec budget cross-reference

PLAN.md §5.6 spec: cold-start <50 ms · hot-reload p95 <100 ms · per-module <1 MB.

| Spec metric | Spec budget | W04 observation | Headroom |
|---|---|---|---|
| Cold-start (engine + compile + instantiate) | <50 ms | ~967 us | **~52x under** |
| Per-instance memory | <1 MB | 64 KiB | **~16x under** |
| Hot-reload p95 | <100 ms | not validated this wave | landed in W20 |
| Per-tick (host-call + return) | not budgeted here | ~125 ns | bench in W20 |

## What W04 did **not** measure

- **Hot-reload p95** — that's W20 (`script-bench`) coupled with `script-host`
  (Phase 3.3). If hot-reload p95 > 500 ms after Phase 3.3 optimization,
  ADR-077's escape clause activates per PLAN.md §1.4.
- **Cap-gate overhead at host-function call sites** — measured incidentally
  by the per-tick number above, but not isolated.
- **Trap recovery cost** — the panic registry path is exercised in
  `panic_recovery::tests::registry_buffers_reports` but not microbenchmarked.

## Test coverage

```
cargo test -p rge-runtime-wasmtime-engine --features engine_wasmtime
```

- `tests::version_matches_cargo_pkg_version` — the version-pin sanity check.
- `tests::engine_constructs_with_default_config` — Engine::new() does not panic.
- `panic_recovery::tests::registry_buffers_reports` — panic registry round-trip.
- `hello_world::hello_world_two_ticks_increment_counter_to_two` — the
  primary W04 deliverable: load + tick + tick → counter == 2.
- `hello_world::cap_gate_module_without_network_cap_fails_at_instantiate` —
  the W04 cap-gate deliverable: a module importing `wasi:sockets/tcp.connect`
  without declaring `<network>` fails at link time.
- `hello_world::cap_gate_phi_plugin_fails_against_compute_only_grant` —
  Path B runtime gate rejects an under-granted `<reads-phi>` plugin **before**
  any wasmtime compile work.
- `hello_world::loader_rejects_unknown_effect_tag` — `<undeclared>` in the
  rcad-effects manifest fails at load time, not runtime.
- `hello_world::instantiate_quarantine_unaffected_when_no_trap` — clean
  ticks leave the instance non-quarantined.
- `hello_world::baseline_timings_are_within_spec` — sanity bounds on the
  numbers above.

## Constitutional check

- **One runtime** (PLAN.md §5.1): wasmtime + Cranelift JIT, no sibling.
- **Cap-gate as the only authority** (PLAN.md §1.4): host-function set is
  gated by the plugin's declared effect mask; no per-call permission map.
- **Engine-independent cap-gate API** (W04 spec, ABI promise): the
  `runtime-wasmtime` cap-gate API is unchanged by this activation. Both
  Path A (compile-time const-generic gate) and Path B (runtime gate) keep
  the same predicate.

## Cross-reference: rustforge

Stolen from:
- `rustforge/crates/runtime-wasmtime/src/effect_specifier.rs` — direct copy
  of the const-generic typestate; trimmed to remove medical-domain copy in docs.
- `rustforge/crates/runtime-wasmtime/src/host.rs` — direct copy plus
  `tick_counter` and `last_dt` fields added for the W04 hello-world test.
- `rustforge/crates/runtime-wasmtime/src/runtime.rs` — direct copy of the
  hand-rolled `.wasm` header validator + `WasmRuntime` struct, with the
  `LoadedPlugin` extended to keep the original bytes (so the engine sibling
  can pass them to wasmtime without re-reading from disk).
- `rustforge/crates/runtime-wasmtime/tests/example_plugin.rs` — adapted into
  `tests/hello_world.rs`; the rcad-property/pybridge sections were dropped
  (those are W02 and outside W04 scope).

The `rustforge::runtime-wasmtime-engine` crate was an empty placeholder; W04
is the wave that activates it. The `engine.rs`, `instance.rs`, and
`panic_recovery.rs` modules in this crate are new for RGE.
