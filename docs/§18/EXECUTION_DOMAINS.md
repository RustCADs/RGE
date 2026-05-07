# EXECUTION_DOMAINS

| Companion to | PLAN.md §0.3.1 (Execution domains) + PLAN.md §1.6 / §1.6.8 (determinism modes); ADR-099 (Execution domains naming — formal ADR pending; PLAN §0.3.1 + this doc are canonical until trigger fires, mirroring ADR-101/§1.14 + ADR-102/§1.13 deferred-pattern) |
|---|---|
| Status | Active v0; framing landed PLAN v0.7; four canonical domains with shared substrate (kernel/types reflection schema, capability model, kernel/diagnostics spans, hot-reload-watcher orchestration) and distinct backends (wasmtime ≠ wgpu ≠ inline expr-wasm) |
| Audience | Authors deciding "where does this code execute"; orchestrator authors composing per-domain hot-reload + capability gating; reviewers watching for sibling-system creep (e.g. "GPU scripting" must slot into existing GPU-compute domain, not become a new one) |
| Sibling doc | `RECOVERY_MODEL.md` — failure-class implications per domain; `KERNEL_PLUGIN_HOST_LIFECYCLE.md` — "untrusted execution domains" framing for plugin-host; `KERNEL_TYPES.md` — the shared reflection schema across domains |
| Reference impls | `kernel/types/` (shared reflection schema across all 4 domains) · `kernel/diagnostics/` (shared diagnostic spans) · `crates/runtime-wasmtime/` + `crates/runtime-wasmtime-engine/` (CPU-gameplay domain backend) · `crates/script-host/` (CPU-gameplay domain consumer) · `crates/expr-wasm/` (Expression-microcode domain) · `crates/gfx/` (GPU-shading + GPU-compute domain backend via wgpu) · `crates/hot-reload-watcher/` (per-domain orchestration) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the execution-domain framing (PLAN §0.3.1). Per-domain runtime details (wasmtime cap-gate, wgpu pipeline state, expr-wasm Pratt parser) belong in their crate-specific docs; this doc fixes the cross-cutting taxonomy.

## 1. Why a substrate

PLAN §0 commits to one runtime per execution domain (no siblings within a domain). Without explicit framing, "GPU scripting" tomorrow becomes an accidental third runtime alongside wasmtime + wgpu; the marketplace fragments into competing scripting ecosystems; users have to learn N runtimes for one engine.

The framing fixes the boundary at four canonical domains and forces every new execution context to slot into an existing one (or pass formal ADR review for a fifth). The naming is the discipline: when "GPU scripting" demand surfaces, it slots into the existing **GPU-compute** domain rather than becoming an accidental sibling. Per PLAN §0.3.1: *"This naming prevents 'GPU scripting becoming an accidental sibling ecosystem' — when GPU compute scripting demand surfaces, it slots into the existing domain. New domains (XR shaders, neural compute) require formal review and an ADR."*

## 2. The 4 canonical domains

Per PLAN §0.3.1:

| Domain | Runtime | Reflection | Capability | Hot-reload |
|---|---|---|---|---|
| **CPU gameplay** | wasmtime | shared schema via `#[rge::reflect]` | `runtime-wasmtime` cap-gate | WASM module swap |
| **GPU shading** | WGSL via wgpu | shared schema (material params) | wgpu pipeline state | shader recompile + PSO swap |
| **GPU compute** | WGSL compute via wgpu | shared schema | wgpu compute pipeline | shader recompile + dispatch swap |
| **Expression microcode** | expr-wasm (single-expr WASM) | shared schema | inline whitelist | recompile + cache invalidate |

### What each domain is

- **CPU gameplay** — ECS systems and fixed-step physics + game logic. Ships as Rust→WASM (Tier-3 sandboxed plugins per PLAN §10.3) or as native Tier-2 plugin code through the `Plugin` trait. The wasmtime engine is canonical; capability gating is enforced by `runtime-wasmtime` (compile-time const-generic typestate for in-process plugins; runtime grant-check at `WasmRuntime::instantiate` for dynamic .wasm blobs). See `crates/runtime-wasmtime/src/lib.rs` for the two-path cap-gate API.
- **GPU shading** — vertex + fragment WGSL shaders bound to a wgpu pipeline (PSO). Material-graph emits WGSL; `gfx` owns pipeline construction (`crates/gfx/src/mesh_pipeline.rs` is the canonical example: `mat4x4` uniform + per-mesh transform). Tier-2 only (no Tier-3 GPU shading until WGSL component-model lands).
- **GPU compute** — WGSL compute shaders dispatched via wgpu compute pipelines. Future home for skinning compute (Phase 4-Authoring), particle simulation, mesh-skin transforms, post-process passes. Tier-2 plugin owners construct compute pipelines through `gfx` substrate; Tier-3 sandboxed compute is a Phase 5+ deliverable (requires WIT-typed wgpu component-model).
- **Expression microcode** — single-expression WASM compiled inline via `expr-wasm`. Use cases: animation curves, material constants, inspector formula fields. Different runtime contract from full-fat scripts: the parser is ≤50 LOC Pratt; the codegen emits a single-`f32`-returning WASM function; the cache key is the source-string BLAKE3. Hot-reload is recompile + cache invalidate (no module swap; the inline expression doesn't carry persistent state across edits). See `crates/expr-wasm/src/lib.rs` for the pipeline.

The constitutional commitment per ADR-076: expression evaluation runs through **the same wasmtime engine** as full-fat scripts, no sibling interpreter. Different surface, same engine.

## 3. Why the naming distinction matters

The four-domain framing has three load-bearing consequences:

### Kernel/userspace boundary equivalent for plugin-host

`kernel/plugin-host` treats plugins as the kernel/userspace boundary (per `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §10 + the host.rs module-doc "untrusted execution domains" framing). The four domains are the four classes of "userspace" the kernel hosts:

- **CPU gameplay** → wasmtime sandbox or in-process Tier-2 plugin; `catch_unwind` shield + leak-detection diff applies (host.rs §5).
- **GPU shading / compute** → wgpu queue submission; the wgpu device handles GPU-side faults (device-lost surfaces as `wgpu::DeviceLostReason`); the host doesn't catch_unwind GPU work.
- **Expression microcode** → wasmtime instance under expr-wasm; same wasmtime backend as CPU gameplay but the cap surface is the inline whitelist (no ECS access, no I/O, no audit-ledger projection).

### Capability gating per domain

Each domain has its own capability surface:

- **CPU gameplay** — `runtime-wasmtime` const-generic `Plugin<EFFECTS>` typestate (compile-time gate for in-process Rust plugins) + `effect_specifier::grant_check` runtime gate at `WasmRuntime::instantiate` for dynamic .wasm blobs.
- **GPU shading / compute** — wgpu pipeline state object (PSO) is the gate. The `gfx::TrianglePipeline::new(gfx_ctx, target.format())` call validates the WGSL shader against the device's supported features; failure returns a `RuntimeFault` per `PLUGIN_HOST_PATTERNS.md` §4.
- **Expression microcode** — closed-set stdlib whitelist enforced at parse time; the parser rejects un-whitelisted function calls before codegen sees them. Cross-ref `crates/expr-wasm/src/parser.rs`.

### Resource ownership rules (cross-ref `kernel-isolation` lint)

The `forbidden-dep` architecture lint enforces that domain-backing crates can't reach into other domains' substrate types:

- `crates/physics` (CPU-gameplay domain consumer) MUST NOT depend on `crates/script-host` (PLAN §1.8 forbidden-dep rule: "physics cannot depend on script-host"). Cross-domain ECS-mediated communication only.
- The renderer (CPU-gameplay → GPU-shading boundary) MUST NOT depend on game-domain crates (PLAN §1.8: "Renderer cannot depend on game-domain crates"). The boundary is enforced at the dep level.
- The `kernel-isolation` lint (filename mismatch: see `tools/architecture-lints/src/kernel_isolation.rs` module-doc "Naming mismatch" subsection — it actually enforces the one-import-path-per-format rule from PLAN §1.6.4, NOT kernel isolation per se) does NOT enforce cross-domain isolation. The cross-domain isolation is enforced via the `forbidden-dep` lint.

## 4. Per-domain failure-class implications

Cross-ref `RECOVERY_MODEL.md` §9 for the per-class examples; the per-domain mapping:

| Domain | Backing crates (sample) | Worst-case class | Recovery path |
|---|---|---|---|
| CPU gameplay | `crates/script-host` | plugin-fatal | Tier-3 WASM trap → unload plugin; Tier-2 → recoverable per PLAN §1.13 row "script-host \| WASM trap \| plugin-fatal if Tier-3; recoverable if Tier-2". |
| CPU gameplay | `crates/physics` | snapshot-recoverable | Rapier internal panic → entity quarantine (recoverable); state participates in PIE (snapshot rollback canonical). See `crates/physics/src/lib.rs`. |
| CPU gameplay | `crates/audio` | recoverable | `ManagerError::UnknownClip` → log + skip; audio device loss → recoverable warning. See `crates/audio/src/lib.rs`. |
| GPU shading | `crates/gfx` | recoverable | GPU init failure or pipeline compile error → software path fallback or surface diagnostic. See `crates/gfx/src/lib.rs`. |
| GPU shading | `crates/material-graph` (PLAN §1.13) | recoverable | Shader compile hang → 30s timeout; naga validation fail → placeholder pipeline. |
| GPU compute | `crates/skinning` (Phase 4-Authoring) | recoverable | Compute-shader compile/dispatch failures → fallback to LBS CPU path. |
| Expression microcode | `crates/expr-wasm` | recoverable | Parse error / un-whitelisted-stdlib → `ExprError`; recompile-and-retry on edit. See `crates/expr-wasm/src/lib.rs`. |

The class promotes upward when the domain participates in PIE state: physics is snapshot-recoverable because its state is part of the PIE; audio is recoverable because its state is transient. The class is per-crate, not per-domain — multiple crates per domain may carry different classes.

## 5. Determinism implications

Per PLAN §1.6.8 (Replay-Stable v1.0 determinism mode, gameplay-only):

| Domain | Determinism contract | What's deterministic | What's NOT |
|---|---|---|---|
| CPU gameplay | **Replay-Stable v1.0** | ECS systems with `DeterministicSystem` marker; fixed-timestep physics; script ticks; pinned HashMap iteration | async streaming residency; file-system I/O timing |
| GPU shading | **Bounded non-determinism** | per-frame deterministic given a snapshot; same inputs produce visually-equivalent output | bit-identical output (driver / GPU / scheduling variations); cross-machine pixel identity |
| GPU compute | **Bit-deterministic with explicit barriers** | content-addressed cooks; same WGSL + same input buffer + explicit barriers → same output buffer | implicit barriers (driver-dependent); cross-vendor consistency at floating-point precision |
| Expression microcode | **Bit-deterministic** | same source string + same input bindings → same `f32` output, byte-identical across runs | none — single-expression WASM is fully deterministic given its pure-function contract |

The wasmtime engine itself supports a deterministic-config mode (Phase 4-Foundation responsibility; tracked but not yet wired). Today the determinism contract is enforced at the substrate level: wasmtime defaults are deterministic for pure-function workloads (no random sources, no I/O); the audit-ledger substrate (`KERNEL_AUDIT_LEDGER.md` §11) is the keystone for the Replay-Stable v1.0 byte-identical replay gate.

CAD-output determinism is a separate concern tracked in `RGE/CAD_DETERMINISM.md` (best-effort at v1.0; not gated). The CPU-gameplay domain's Replay-Stable contract does NOT extend to CAD operator evaluation order.

## 6. Cross-domain communication

The four domains coordinate through fixed channels:

### CPU → GPU (CPU gameplay → GPU shading / compute)

Via the wgpu Queue. Tier-2 `gfx` consumers stage vertex / index / uniform buffers via `wgpu::Queue::write_buffer`; pipelines are pre-compiled at substrate setup; per-frame the CPU records command buffers and submits to the queue. Backpressure is wgpu-managed (the device's submission queue depth); the CPU side doesn't need to spin.

### CPU → Expression (CPU gameplay → Expression microcode)

Via the `expr-wasm` script-host Linker. The host stages bindings (e.g. `time: f32`, `entity_count: i32`) into the wasmtime store; calls the compiled expression's exported `eval()` function; receives the `f32` result. Cache hit on identical source string is `~5 ns/call`; first-time compile is `~1 ms` per the expr-wasm pipeline doc.

### CPU → CPU (intra-CPU-gameplay)

Via `kernel/events::EventBus`. The bus is frame-queued: events emitted in frame N are visible to consumers in frame N+1 after `EventBus::advance_frame` is called. This keeps delivery deterministic (no callback storage, no closure-dispatch) and aligns with the Replay-Stable v1.0 determinism contract. See `KERNEL_EVENTS_CHANNEL.md` for the substrate.

### Cross-machine (post-v1; Authoritative-Server / Lockstep-Stable)

Per PLAN §6.17 (authoritative CAD serialization) and the `replication` placeholder Tier-2 crate. Multi-machine CAD edit synchronization is post-v1; the framing is preserved so cross-domain replication can layer on the existing per-domain runtimes without inventing a fifth domain.

## 7. Shared substrate (no duplication)

Per PLAN §0.3.1 "Shared across domains":

- **Reflection schema** — `kernel/types`. All four domains marshal data through `#[derive(Reflect)]` types. CPU gameplay marshals via `script-host`'s `ReflectValue`; GPU shading marshals material-parameter struct types; expr-wasm marshals input bindings. See `KERNEL_TYPES.md` §13 (consumer surface).
- **Capability model** — one taxonomy enforced per-domain. Tier-3 WASM plugins use `runtime-wasmtime` cap tickets; Tier-2 in-process plugins use the const-generic typestate; GPU shaders are gated by wgpu pipeline construction; expressions are gated by the parser whitelist.
- **Diagnostic spans** — `kernel/diagnostics`. Source-mapped diagnostics from compiled scripts use `Span::at_script_line`; WGSL compile failures use `Span::at_file("pbr.wgsl", line, col)`; expression parse errors use `Span::at_script_line`. See `KERNEL_DIAGNOSTICS.md` §5.
- **Hot-reload orchestration** — `crates/hot-reload-watcher` triggers per-domain handlers. The watcher is Tier-2 substrate; it owns the file-watch loop and dispatches to per-domain handlers (WASM module swap for CPU gameplay; PSO swap for GPU shading; cache invalidate for expressions).

## 8. Not shared (correctly distinct)

Per PLAN §0.3.1 "Not shared":

- **The execution backends** — wasmtime ≠ wgpu. Different vendor-supplied runtimes; different ABI; different failure modes. The four-domain framing is the wrapper that lets them co-exist.
- **Hot-reload mechanics per domain** — WASM module swap (CPU gameplay) is a different operation from PSO swap (GPU shading) is different from cache-invalidate (expression). The watcher dispatches; per-domain handlers do the work.
- **Profiler integrations** — wasmtime profiler integrations (Tier-3 only at v1) differ from wgpu profile-marker injection differs from expr-wasm cache hit/miss tracking. No shared substrate; per-domain best practice.

## 9. Sibling-system tripwire

The naming exists to catch sibling creep. The Rhai-test (per PLAN §0.3 "Anti-pattern audit on every PR"): every architectural change must answer *"what unified system does this overlap with?"*. For execution-domain creep:

- **"GPU scripting"** demand → slots into **GPU compute** (existing domain). Don't invent a fifth.
- **"XR shaders"** demand → currently no domain; requires formal ADR per PLAN §0.3.1 "New domains require formal review and an ADR".
- **"Neural compute"** demand → currently no domain; requires formal ADR.
- **"Asset import scripting"** → slots into **CPU gameplay** (Tier-3 sandboxed import plugins through `runtime-wasmtime`).
- **"Inline math in inspector"** → slots into **Expression microcode** (already the canonical home).

The constitutional commitment: any proposal that doesn't slot into one of the four MUST go through ADR review; the four-domain framing is otherwise binding for v1.

## 10. Source / spec inconsistencies

- **Brief stated "ADR-099 may not exist as a file"**; source-truth confirmed: `docs/adr/` contains `ADR-098`, `ADR-104`, `ADR-112`, `ADR-114`, `ADR-115` (with 2026-05-10 amendment), `ADR-116`. ADR-099 is referenced in PLAN §0.3.1 line 69 ("ADR-099. Companion: `RGE/EXECUTION_DOMAINS.md`") and PLAN §13 entry-table line 1374 / line 1308 (review entry "ADR-099 — Execution domains naming") but has not been created as a formal ADR file. This doc adopts the same pattern as `GRAPH_FOUNDATION.md` (companion to ADR-101 — pending) and `RECOVERY_MODEL.md` (companion to ADR-102 — pending). The header table reflects this. The PLAN.md pointer "Companion: `RGE/EXECUTION_DOMAINS.md`" notes the historical name (without the `docs/§18/` prefix); the doc lives at `docs/§18/EXECUTION_DOMAINS.md` per the §18 convention.
- **Brief framed Expression-domain determinism as "bit-deterministic via wasmtime determinism config"**; source-truth: `crates/expr-wasm/src/lib.rs` does NOT explicitly configure wasmtime for deterministic mode. Bit-determinism is achieved via the pure-function contract (no I/O, no random sources, single-`f32`-returning function); the wasmtime determinism-config wiring is Phase 4-Foundation responsibility. This doc reflects the current source-truth (pure-function contract is the bit-determinism mechanism, not explicit config).
- **Brief framed material-runtime as a Tier-2 GPU-shading consumer**; source-truth: `crates/material-runtime/src/lib.rs` is currently a 3-line stub crate ("`rge-material-runtime` — stub crate. Architecture frozen at v0.8; implementation pending per IMPLEMENTATION.md."). The brief's framing is correct in intent but the implementation is deferred to Phase 4-Authoring per the architecture-freeze policy. This doc reflects the future-state surface as documented in PLAN §0.3.1 (material params marshaled through shared reflection schema).

## 11. References

- **PLAN.md §0.3.1** — Execution domains (CPU gameplay / GPU shading / GPU compute / Expression); the canonical four-domain table.
- **PLAN.md §1.6 / §1.6.8** — determinism modes; Replay-Stable v1.0 (gameplay-only); CAD-output determinism (separate; not gated at v1).
- **PLAN.md §1.8** — forbidden-dep rules; cross-domain isolation enforcement (`physics` ≠ `script-host`; renderer ≠ game-domain).
- **ADR-099** (deferred) — execution-domains naming formal ADR; pending until trigger fires (mirrors ADR-101 / ADR-102 deferred-pattern).
- **ADR-076** — wasmtime canonical for expression evaluation; expr-wasm shares the engine, not a sibling interpreter.
- **`RECOVERY_MODEL.md`** — sibling §18 doc; failure-class taxonomy; per-domain recovery path mapping.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; "untrusted execution domains" framing; plugin-host catch_unwind shield.
- **`KERNEL_TYPES.md`** — sibling §18 doc; the shared reflection schema across all four domains.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; the shared diagnostic-span substrate.
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; the deterministic event-id substrate that backs Replay-Stable v1.0.
- **`KERNEL_EVENTS_CHANNEL.md`** — sibling §18 doc; the frame-queued EventBus for CPU → CPU intra-domain communication.
- **`crates/runtime-wasmtime/src/lib.rs`** — CPU-gameplay domain backend; cap-gate API (compile-time + runtime).
- **`crates/runtime-wasmtime-engine/`** — wasmtime + wat dependency carrier; W04 `engine_wasmtime` feature flag.
- **`crates/script-host/src/lib.rs`** — CPU-gameplay domain consumer; ECS bridge + event hooks + state-preserving instance swap.
- **`crates/expr-wasm/src/lib.rs`** — Expression-microcode domain; ≤50 LOC Pratt parser + WASM codegen + cache.
- **`crates/gfx/src/lib.rs`** — GPU-shading + GPU-compute domain backend (Phase 6.1 substrate).
- **`crates/hot-reload-watcher/`** — per-domain hot-reload orchestration; dispatches to handlers.
- **`tools/architecture-lints/src/forbidden_dep.rs`** — cross-domain isolation enforcement at the dep level.
