# Wave W04 — runtime-wasmtime-engine activation

> Self-contained agent dispatch. **Phase 3 — CRITICAL.** Validates the constitutional WASM hot-reload bet.
> Cross-refs: PLAN.md §5.1, §1.4 escape clause; IMPLEMENTATION.md Phase 3.

## Goal

Flip the deferred `engine_wasmtime` feature flag, run a hello-world WASM module, establish the baseline benchmark. This is the start of Phase 3 (the highest-risk validation point in the entire plan).

## Crates owned

`crates/runtime-wasmtime`, `crates/runtime-wasmtime-engine`.

## Files this wave touches

```
crates/runtime-wasmtime/src/{lib.rs, effect_specifier.rs, host.rs, runtime.rs, cap_ticket.rs}
crates/runtime-wasmtime-engine/Cargo.toml          # flip `default = ["engine_wasmtime"]`
crates/runtime-wasmtime-engine/src/{lib.rs, engine.rs, instance.rs, panic_recovery.rs}
crates/runtime-wasmtime-engine/tests/hello_world.rs
crates/runtime-wasmtime-engine/tests/fixtures/hello_tick.wat
```

## Stubs needed

- `kernel/types::Reflect` — assume W02 has shipped trait stub.
- `kernel/diagnostics` — local stub if not yet implemented.

## Implementation order

1. **Steal** `runtime-wasmtime` cap-gate API from rustforge — already exists, mostly adapt.
2. Activate `engine_wasmtime` feature in `runtime-wasmtime-engine/Cargo.toml`. Pull `wasmtime` + `wit-bindgen` workspace deps.
3. `engine.rs`: `Engine::compile(&[u8]) -> Module`, `Engine::instantiate(Module, &Caps) -> Instance`.
4. `instance.rs`: tick invocation; memory limits; cap ticket enforcement.
5. `panic_recovery.rs`: WASM trap → diagnostic; instance quarantined, editor continues.
6. Hello-world test: a 50-line `.wat` exporting `tick(dt: f32) -> ()` that increments a counter; load + tick + read-back.
7. Document baseline: instance creation time, tick overhead, memory footprint per instance in `BASELINE.md`.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/runtime-wasmtime/` | cap-gate API + effect specifiers (already designed) | direct copy; adapt only for new errors crate |
| `rustforge/crates/runtime-wasmtime-engine/` | deferred crate (this wave activates) | flip the feature flag |
| `rustforge/crates/runtime-wasmtime/src/effect_specifier.rs` | capability-gated typestate | direct copy |
| `rustforge/crates/runtime-wasmtime/src/host.rs` | host function bindings | direct adapt |
| `rustforge/crates/runtime-wasmtime/tests/example_plugin.rs` | integration test pattern | adapt for hello_world |

Header pattern: `// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated`.

## Exit criteria

- `cargo test -p rustforge-runtime-wasmtime-engine --features engine_wasmtime` passes.
- Hello-world module: load + tick(0.016) + tick(0.016) → counter == 2.
- Cap-gate: a module without `<network>` cap that imports `wasi:sockets/tcp` fails at instantiate (test fixture).
- Baseline timings recorded in `crates/runtime-wasmtime-engine/BASELINE.md`.

## Duration estimate

1 day for activation + hello-world. Hot-reload prototype is part of W20 (script-bench) coupled with `script-host` (Phase 3.3 — separate wave or follow-on).

## Anti-pattern check

PASS — single runtime (wasmtime). Cranelift-only AOT (no LLVM sibling). Cap-gate API is the only authority for plugin permissions.

## Handoff

After merge: W19 (expr-wasm) consumes wasmtime to produce WASM modules. W20 (script-bench) benchmarks against this. The hot-reload full loop (Phase 3.3) is a Phase 3 follow-on after `script-host` exists (post-Phase 1).

## Critical context

This wave starts the Phase 3 validation. **If hot-reload swap p95 > 500ms after later optimization in Phase 3.3, ADR-077 escape clause triggers.** This wave just activates the engine; hot-reload validation comes later.
