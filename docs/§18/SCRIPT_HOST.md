# SCRIPT_HOST

| Companion to | PLAN.md §3 (sandboxed scripting via WASM/wasmtime) + §10 (Tier-3 plugin tier) + §1.13 line "WASM trap = plugin-fatal Tier-3 / recoverable Tier-2"; IMPLEMENTATION.md Phase 3.2 (`crates/script-host` substrate done) |
|---|---|
| Status | Substrate-done (per Status.md 2026-05-09 line 23 "done (substrate) — 4 tests"); the substrate gate is met (swap window 0.31ms vs 100ms p95 budget = 320× headroom; cold-start 9.1ms vs 50ms = 5× headroom) but the formal Phase 3 gates (1000-entity p95 / 1-hour memory / 100-cycle preservation) remain DEFERRED per Status.md "Waiting" + change.md; the runtime-wasmtime × plugin-host integration is also deferred per ADR-114 followup |
| Audience | Authors writing WASM scripts targeting RGE; reviewers verifying the call-scope `unsafe` pattern + SAFETY proofs; future Phase-4-Foundation authors graduating to a full WIT component-model bridge; orchestrator authors wiring script-host into the runtime tier |
| Sibling doc | `PLUGIN_API.md` — Tier-2 host-side `Plugin` trait surface (script-host does NOT implement; it's the host for Tier-3 wasm scripts); `PLUGIN_HOST_PATTERNS.md` — Tier-2 plugin-author guide (parallel reference for the Tier-2 path); `EXECUTION_DOMAINS.md` — Expression / Script execution domain row that script-host is the substrate for |
| Reference impls | `crates/script-host/src/lib.rs` (58L) · `crates/script-host/src/host_state.rs` (199L; the call-scope pointer pattern) · `crates/script-host/src/ecs_bridge.rs` (249L; 7 wasm host functions) · `crates/script-host/src/script_module.rs` (~234L; `ScriptModule` + `ScriptInstance`) · `crates/script-host/src/swap.rs` (171L; capture/restore for state-preserving hot-reload) · `crates/script-host/src/event_hooks.rs` (55L; advisory subscription tracker) · 3 integration test files at `crates/script-host/tests/` (4 tests) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the WASM script host substrate per IMPLEMENTATION.md Phase 3.2. The Tier-2 plugin path (different audience, different ABI) is covered by `PLUGIN_API.md` + `PLUGIN_HOST_PATTERNS.md`; the Tier-3 sandboxed-WASM path documented here will eventually reach the orchestrator via the runtime-wasmtime × plugin-host integration that ADR-114's "Tier-3 lifetimes" amendment defers.

## 1. Why a substrate

PLAN §3 commits to WASM as the sandbox boundary for user-authored scripts: hot-reloadable, tier-3-isolatable, capability-gated. Without a substrate, every script-touching subsystem (ECS access, event subscription, diagnostic emission, hot-reload state preservation) would invent its own host-function vocabulary — leading to N parallel wasmtime `Linker` setups across the workspace.

`crates/script-host` is the canonical home for the wasmtime-driven host-function vocabulary + the `Store<HostState>` data type the wasmtime guest sees + the state-preserving instance swap protocol that Phase-3 hot-reload requires.

Three load-bearing properties:

- **wasmtime `Linker`-driven host functions.** ECS access (entity_count / spawn / despawn / advance_tick / get_counter / set_counter) and diagnostic emission (`rge.diagnostic::emit`) are exposed as wasm imports under stable `(module, name)` pairs. Plus-only evolution: between Phase 3 and Phase 4, only **additions** are permitted (existing names + signatures are frozen).
- **Call-scope pointer pattern.** The wasmtime `Store<HostState>` persists across tick calls, but `&mut World` / `&mut EventBus` / `&mut DiagnosticAggregator` cannot live inside the store beyond a single tick (the borrow checker correctly forbids aliasing). `host_state.rs::with_call_scope` is the **only** function that writes raw pointers into `HostState`; an RAII guard clears them even on panic.
- **State-preserving instance swap.** `capture_state` snapshots `Counter` components from every live entity into a RON blob; `restore_state` deserialises and re-inserts after the new instance loads. The Phase-3 hot-reload contract: same scene state, same logical tick, different wasm bytes — and the substrate gate measures the swap-window duration.

## 2. The wasmtime Linker-driven host function pattern

`EcsBridge::install(&mut Linker<HostState>)` (`crates/script-host/src/ecs_bridge.rs` lines 96-247) registers all host functions on a wasmtime `Linker<HostState>`. Each function is a `func_wrap` closure that pulls `HostState` via `Caller::data_mut()` and operates on the call-scoped pointers.

The seven exposed wasm imports (per `ecs_bridge.rs` lines 12-21):

| Host function     | Wasm import                                   | Returns                            |
|-------------------|-----------------------------------------------|------------------------------------|
| `entity_count`    | `rge.ecs::entity_count() -> i64`              | `world.entity_count() as i64`     |
| `spawn`           | `rge.ecs::spawn() -> i64`                     | `entity_id_to_i64(world.spawn())` |
| `despawn`         | `rge.ecs::despawn(i64) -> i32`                | `1` on hit, `0` on miss            |
| `advance_tick`    | `rge.ecs::advance_tick() -> ()`               | side-effect only                   |
| `get_counter`     | `rge.ecs::get_counter(i64) -> i64`            | counter value, `0` if no Counter, `i64::MAX` if entity missing |
| `set_counter`     | `rge.ecs::set_counter(i64, i64) -> i32`       | `1` on success, `0` if entity missing |
| `diagnostic_emit` | `rge.diagnostic::emit(i32, i32, i32) -> ()`   | reads UTF-8 from wasm linear memory at (ptr, len), routes through severity |

`(module, name)` are stable across engine versions; appending a new function is a non-breaking change. **Renaming or removing an existing function would invalidate every existing wasm script** — the names are part of the guest-side ABI.

### `entity_id_to_i64` — low-63-bit handle encoding

Per `ecs_bridge.rs` lines 49-66:

```rust
pub fn entity_id_to_i64(id: EntityId) -> i64 {
    (id.ulid().0 & 0x7fff_ffff_ffff_ffff_u128) as i64
}
```

Mask to low 63 bits — guarantees the result is non-negative when interpreted as `i64`. Loses one bit of entity-distinguishing entropy; 2^63 distinct handles is still vastly more than any conceivable scene. The mask is **load-bearing for the wat fixtures**: per Status.md line 23, the bug fix during integration was specifically "`entity_id_to_i64` masked to low-63 bits to prevent negative-handle aliasing the wat fixture's 'uninitialized' sentinel — now stable across 5 reruns". The guest wasm uses signed comparisons (`< 0`) as an "uninitialized" sentinel; the host must never produce a negative handle.

`ENTITY_NOT_FOUND = i64::MAX` is the sentinel `get_counter` returns when the entity handle isn't found in the Counter-bearing query (vs `0` which is the legitimate value `0`).

### `find_entity_by_handle` — O(n) over Counter-bearing entities

Per `ecs_bridge.rs` lines 75-83: `world.query::<Counter>().find_map(...)` linear scan. Acceptable for prototype workloads; entities **without** Counter cannot be found by handle yet (`despawn` on a non-Counter entity returns `0`). Per Phase-4 plan: `World` gains an `entity_ids()` iterator for handle-less traversal.

## 3. State-preserving instance swap (Phase 3 hot-reload contract)

Lives at `crates/script-host/src/swap.rs`. The Phase-3 hot-reload contract:

```rust
pub fn capture_state(world: &World) -> Result<SwapPlan, SwapError>;
pub fn restore_state(world: &mut World, plan: &SwapPlan) -> Result<usize, SwapError>;

pub struct SwapPlan {
    pub captured_at_tick: u64,
    pub component_snapshot: Vec<u8>,           // RON-encoded CounterSnapshot
    pub event_subscriptions: Vec<SubscriptionId>,   // advisory; see §6
}

pub struct SwapResult {
    pub captured_at_tick: u64,
    pub restored_components: usize,
    pub swap_duration_ms: f64,
}
```

The seven-step swap window (per `tests/swap_smoke.rs` lines 1-13):

1. Compile v1 fixture → `ScriptModule`.
2. Spawn entity with `Counter { value: 0 }`.
3. Tick v1 ten times → `Counter == 10`.
4. **`capture_state(&world)` → `SwapPlan`.** ← swap window start
5. Compile v2 fixture (different bytes, different behaviour).
6. Drop old instance, instantiate v2.
7. **`restore_state(&mut world, &plan)` → `Counter == 10`.** ← swap window end
8. Tick v2 five times with the new behaviour.
9. Verify `swap_duration_ms` is recorded.

### Snapshot format — RON over Counter (prototype scope)

Per `swap.rs` lines 86-94:

```rust
struct CounterSnapshot { counters: HashMap<String, i64> }
// RON form: (counters: {"<i64-handle-as-string>": <counter-value-i64>, ...})
```

Per the module-doc lines 8-13: only `Counter` components are captured. Generalising the snapshot to every reflected type requires type-erased archetype iteration in `kernel/ecs` — a **Phase 4-Foundation** extension. The swap measurement (steps 4-7) is what matters for the p95 < 100 ms gate; the protocol is correct and minimal.

`restore_state` collects all `(handle, entity_id)` pairs from the current world first (to avoid query-then-mutate borrow conflict), then walks the snapshot's `(handle_str, value)` pairs and re-inserts. **Entities that no longer exist are silently skipped** (despawned between capture and restore is acceptable — the snapshot is best-effort, not transactional).

### Substrate gate met (per Status.md)

- Swap window: **0.31ms** measured vs **100ms p95 budget** = 320× headroom.
- Cold-start: **9.1ms** measured vs **50ms budget** = 5× headroom.

Both numbers are smoke checks (single-entity Counter; hello-world WAT fixture). The rigorous p95 criterion bench against a 1000-entity Counter fixture is Phase 3.4 work in `crates/script-bench` (deferred per Status.md "Waiting" + change.md).

## 4. The `unsafe`-allow sites in `host_state.rs`

`crates/script-host/src/host_state.rs` is the **only** module that contains `unsafe` code. The crate-level `unsafe_code = deny` (overriding the workspace `forbid` per `lib.rs` line 13) accepts each site under explicit `#[allow(unsafe_code)]` alongside a `// SAFETY:` proof comment.

### Five `unsafe`-allow sites

| Line | Construct | Purpose |
|---|---|---|
| 45 | `Drop` impl for `CallScopeGuard` | Clear pointer fields on guard drop (or panic) |
| 77 | `unsafe impl Send for HostState` | Mark the raw-pointer-bearing struct `Send` (justified by `!Send` Store containment) |
| 103 | `world()` accessor | Dereference `world_ptr` within tick scope |
| 118 | `diagnostics()` accessor | Dereference `diagnostics_ptr` within tick scope |
| 187 | `with_call_scope` write step | Install raw pointers into `HostState` before invoking the wasm guest |

Two more `#[allow(unsafe_code)]` sites in `script_module.rs` (lines 170 + 205) are **annotation-only** — they bracket the `let state_ptr: *mut HostState = self.store.data_mut();` raw-pointer extraction inside `tick` / `call_init_entity`, but the dereference happens inside `with_call_scope`. The crate-level deny treats the raw-pointer cast itself as `unsafe`-adjacent.

### Call-scope pattern — the only place raw pointers are written

Per `host_state.rs` lines 152-198:

```rust
pub(crate) fn with_call_scope<F, R>(
    state: *mut HostState,
    world: &mut World,
    diagnostics: &mut DiagnosticAggregator,
    events: &mut EventBus,
    f: F,
) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        (*state).world_ptr = Some(std::ptr::from_mut::<World>(world));
        (*state).diagnostics_ptr = Some(std::ptr::from_mut::<DiagnosticAggregator>(diagnostics));
        (*state).events_ptr = Some(std::ptr::from_mut::<EventBus>(events));
    }
    let _guard = CallScopeGuard(state);
    f()
    // _guard drops here (or on panic), clearing the pointers.
}
```

The four-clause SAFETY proof from `host_state.rs` lines 16-26:

1. Pointers are derived from `&mut T` references whose lifetimes are tied to the current stack frame of `ScriptInstance::tick`.
2. Pointers are set and cleared within `with_call_scope`, which uses an RAII `defer`-style `CallScopeGuard` to clear them even on panic / trap.
3. Wasmtime host functions run synchronously inside `func.call(...)` — no other thread holds the store or the pointed-to values during that window.
4. `HostState` is inside a wasmtime `Store`, which is `!Send` by default, preventing cross-thread pointer escape.

The fifth load-bearing constraint: `world()` / `diagnostics()` accessors **panic** when called outside an active call scope (`unwrap()` on the `Option<*mut T>`). The panic surfaces as a wasm trap, quarantining the instance per the plugin-fatal contract (§7).

## 5. `ScriptModule` and `ScriptInstance` — compiled module + live instance

Lives at `crates/script-host/src/script_module.rs`. Two-phase: compile once (cacheable across runs via BLAKE3 digest), instantiate per scene-load.

### `ScriptModule` — compiled, not yet instantiated

```rust
pub struct ScriptModule { /* private */ }

impl ScriptModule {
    pub fn from_bytes(engine: &Engine, name: impl Into<String>, bytes: &[u8]) -> Result<Self, ScriptError>;
    pub fn digest(&self) -> [u8; 32];      // BLAKE3 of original wasm bytes
    pub fn name(&self) -> &str;
}
```

The 32-byte BLAKE3 digest enables change-detection during hot-reload: same digest → skip re-instantiation. `cold_start_smoke.rs::module_digest_is_content_addressed` pins the same-bytes-same-digest / different-bytes-different-digest property.

### `ScriptInstance` — live wasmtime `Store<HostState>` + `Instance`

```rust
pub struct ScriptInstance { /* private */ }

impl ScriptInstance {
    pub fn instantiate(engine: &Engine, module: &ScriptModule) -> Result<Self, ScriptError>;
    pub fn tick(&mut self, dt: f32, world: &mut World, events: &mut EventBus, diagnostics: &mut DiagnosticAggregator) -> Result<(), ScriptError>;
    pub fn call_init_entity(&mut self, handle: i64, world: &mut World, events: &mut EventBus, diagnostics: &mut DiagnosticAggregator) -> Result<(), ScriptError>;
    pub fn raw_instance(&self) -> &WasmInstance;
    pub fn store(&self) -> &Store<HostState>;
}
```

Instantiation via `instantiate` wires the `EcsBridge` host functions onto a fresh `Linker<HostState>`, looks up the `tick(f32) -> ()` typed function, and returns the bundled `ScriptInstance`. Modules MUST export `tick(f32)` — missing-export surfaces as `ScriptError::MissingExport("tick(f32)")`.

`tick` extracts a raw `*mut HostState` pointer from the store (so it doesn't conflict with the simultaneous `&mut Store` borrow needed for `tick_fn.call`), then calls `with_call_scope` to install the per-tick borrows. `call_init_entity` is a test-support API that the WAT fixtures use to register which entity the module operates on (production scripts use a different config mechanism).

### `ScriptError` taxonomy

```rust
pub enum ScriptError {
    Compile(String),                     // wasmtime rejected the bytes
    Instantiate(String),                 // linker setup or instantiation failed
    TickTrap(String),                    // module's exported tick traps
    MissingExport(&'static str),         // module doesn't export the required symbol
}
```

`TickTrap` carries the wasmtime-rendered trap message; the host catches it via `func.call`'s `Result<_, wasmtime::Error>` (no `catch_unwind` needed — wasmtime traps are `Result`s, not panics). The `host_panic_isolation.rs` integration test pins this: a `(unreachable)` wasm guest produces `Err(ScriptError::TickTrap)` and does NOT crash the test process.

## 6. `EventHooks` — advisory subscription tracker (Phase 4 wiring deferred)

Lives at `crates/script-host/src/event_hooks.rs`. Tracks event-bus subscriptions held on behalf of a running script instance. Subscriptions are auto-cleared when `unsubscribe_all` is called or when the instance is dropped.

```rust
pub struct EventHooks { /* private */ }

impl EventHooks {
    pub fn new() -> Self;
    pub fn subscribe<E: Send + 'static>(&mut self, bus: &mut EventBus) -> SubscriptionId;
    pub fn unsubscribe_all(&mut self, bus: &mut EventBus);
    pub fn subscription_count(&self) -> usize;
}
```

The wasmtime host-function wiring that lets scripts call `rge.event.emit` / `rge.event.subscribe` directly is **deferred to Phase 4-Foundation** — the type exists today so the API shape is stable for the Phase 3.3 prototype, but the linker-side bridge doesn't yet route guest calls into `EventBus::emit`.

## 7. Test coverage breakdown — 4 tests (substrate-only)

Per Status.md line 23: "**done (substrate)** — 4 tests".

### Integration tests in `tests/` (4 total)

- `cold_start_smoke.rs` (2): `cold_start_under_50ms` (compiles + instantiates + first-tick a hello-world WAT module, asserts <50ms total, prints `cold_start_ms`); `module_digest_is_content_addressed` (same wat bytes → same BLAKE3 digest, different bytes → different digest).
- `host_panic_isolation.rs` (1): `trap_is_isolated_and_reported` — a `(unreachable)` WAT module's `tick` returns `Err(ScriptError::TickTrap)`, the test emits an `Error`-severity diagnostic into `DiagnosticAggregator`, asserts `diag.has_errors()` + `highest_severity() == Error`, then runs a SECOND tick on the trapped instance to prove the host process didn't die.
- `swap_smoke.rs` (1): `module_swap_preserves_counter_state` — the canonical 9-step swap window described in §3, asserts post-restore counter equals 10 (the pre-swap value), measures `swap_duration_ms`, and verifies v2's new increment behaviour (counter goes 10 → 20 over 5 ticks at `inc=2`).

### Unit tests in `src/` (0)

The substrate explicitly **has no `#[cfg(test)]` mod inside `src/`** — all tests are integration-tier in `tests/`. The decision is per the W11 dispatch package: src/-level tests would couple the substrate to its testing scaffolding; integration tests exercise the public API exactly as the future runtime will.

The 4 tests cover: cold-start latency budget, content-addressed module digests, trap isolation (the plugin-fatal Tier-3 contract), and the state-preserving hot-reload protocol.

## 8. Failure class — plugin-fatal

`crates/script-host/src/lib.rs` line 3 declares:

```rust
//! Failure class: plugin-fatal
```

Per PLAN §1.13 line "WASM trap = plugin-fatal Tier-3 / recoverable Tier-2": script-host's failure class scopes the **Tier-3** path (sandboxed user-authored scripts). A trapping wasm module's tick is isolated — the trap surfaces as `ScriptError::TickTrap`, the script instance is marked `Failed`, the host process continues. The `host_panic_isolation.rs` test pins this contract.

The class is enforced by the `failure-class` architecture lint (`ARCHITECTURE_LINTS.md` §3); script-host does NOT appear in `tools/architecture-lints/exemptions.toml` (its declaration was added when the substrate landed).

The Tier-2 path (a future RGE feature where engine-internal subsystems run their *own* trusted wasm code) maps to **recoverable** rather than **plugin-fatal** — same wasmtime substrate, different recovery semantics: a trusted wasm module's trap is treated as a recoverable subsystem failure, not a quarantine event. The two paths share THIS substrate but classify failures differently per orchestrator.

## 9. Phase 3 gates — STILL DEFERRED

Per Status.md "Waiting" + change.md the formal Phase 3 gates remain:

- **1000-entity p95 swap < 100ms** — currently measured at single-entity (0.31ms / 320× headroom). Requires `crates/script-bench` rewire against real script-host + 1000-entity Counter fixture. Deferred per Status.md line 201 "Phase 3.3+3.4 formal hot-reload bench gates — script-bench rewire against real script-host + 1000-entity Counter fixture".
- **1-hour memory soak** — runs the substrate for one wall-clock hour to verify no leaks across thousands of swap cycles. Deferred per Phase 3.4 plan.
- **100-cycle preservation** — runs the swap loop 100 times consecutively, asserting state preservation across every cycle (no drift, no missing components). Deferred per Phase 3.4 plan.

The substrate gate (swap window 0.31ms / cold-start 9.1ms) is met today; the formal gates are gated on Phase 3.3 + 3.4 dispatch landing. Status.md cumulative count tracking does NOT include script-host as Phase-3-formally-shipped — it's tracked under "PARTIAL" per the per-crate test count breakdown (line 71: "script-host 4").

### Cross-ref to PLUGIN_API + PLUGIN_HOST_PATTERNS — Tier-3 path is deferred

Per `PLUGIN_API.md` §1 ("Tier-3 sandboxed WASM plugins satisfy [the `Send + 'static` bound] through the WASM ABI's owned-data semantics; Tier-3 lifetimes will be discussed in the future `runtime-wasmtime` × `plugin-host` integration ADR"): the actual integration that lets a Tier-3 plugin be registered with a `PluginHost` (via `runtime-wasmtime` instead of a Tier-2 `Box<dyn Plugin>`) is **deferred per ADR-114 followup**. Today's script-host is the wasmtime substrate; tomorrow's runtime-wasmtime crate will adapt it to the `Plugin` trait surface.

## 10. Source / spec inconsistencies

- **Brief stated "3 `unsafe`-allow sites in `host_state.rs` (call-scope pointer pattern; SAFETY proofs documented inline)"**; source-truth via `grep '#\[allow(unsafe_code)\]' host_state.rs`: **5** `#[allow(unsafe_code)]` sites in `host_state.rs` (lines 45, 77, 103, 118, 187) plus 2 more annotation-only sites in `script_module.rs` (lines 170, 205). Status.md line 23 also says "3 `unsafe`-allow sites" — the brief inherited that count. The doc reflects the actual 5 in `host_state.rs` (§4 table), and notes the additional 2 annotation-only sites in `script_module.rs` for completeness.
- **Brief stated "ECS host functions (entity_count / spawn / despawn / advance_tick / get_counter / set_counter / diagnostic.emit)"** — 7 functions; source-truth confirmed: 6 under `rge.ecs::*` and 1 under `rge.diagnostic::emit` (per `ecs_bridge.rs` lines 12-21 table). The doc reflects the 7-function surface verbatim.
- **Brief stated "swap window 0.31ms vs 100ms p95 budget (320× headroom); cold-start 9.1ms vs 50ms (5× headroom)"**; source-truth: numbers come from Status.md line 23 verbatim. The actual smoke tests (`swap_smoke.rs::module_swap_preserves_counter_state` + `cold_start_smoke.rs::cold_start_under_50ms`) assert **only** that swap duration < 5000ms and cold-start < 50000ms — generous bounds for CI flakiness. The substrate-gate numbers are **measured outside CI** by the dispatch author and recorded in Status.md; the unit-test bounds are deliberately loose.
- **Brief stated "entity_id_to_i64 low-63-bit mask bug fix (per Status.md: 'masked to low-63 bits to prevent negative-handle aliasing the wat fixture's "uninitialized" sentinel')"**; source-truth confirmed at `ecs_bridge.rs` lines 49-66. The mask is `0x7fff_ffff_ffff_ffff_u128` and the comment explicitly says "Loses one bit of entity-distinguishing entropy; 2^63 distinct handles is still vastly more than any conceivable scene". Status.md line 23 confirms the bug-fix history. The doc reflects the mechanism + the rationale verbatim.
- **Brief stated "Phase 3 gates (1000-entity p95 / 1-hour memory / 100-cycle preservation) — STILL DEFERRED per Status.md 'Waiting' + change.md"**; source-truth: Status.md line 201 confirms `Phase 3.3+3.4 formal hot-reload bench gates — script-bench rewire against real script-host + 1000-entity Counter fixture`. The doc reflects this verbatim under §9.
- **Brief assumed substrate-only test count is 4**; source-truth: `tests/cold_start_smoke.rs` actually contains 2 tests (cold_start + content-addressed-digest), `host_panic_isolation.rs` has 1, `swap_smoke.rs` has 1 — total 4, matching Status.md line 23 verbatim. The doc reflects this.

## 11. References

- **PLAN.md §3** — sandboxed scripting via WASM/wasmtime; the substrate this crate implements.
- **PLAN.md §10** — Tier-3 plugin tier; the failure-class scope for sandboxed scripts.
- **PLAN.md §1.13** — failure containment model; "WASM trap = plugin-fatal Tier-3 / recoverable Tier-2".
- **IMPLEMENTATION.md Phase 3.2** — `crates/script-host` substrate done; the 4-test gate.
- **IMPLEMENTATION.md Phase 3.3 / 3.4** — formal hot-reload bench gates (1000-entity p95 / 1-hour memory / 100-cycle preservation); deferred.
- **ADR-114** — `PluginContext` owned-resources-handoff design; the Tier-3 lifetimes amendment defers the runtime-wasmtime × plugin-host integration.
- **`PLUGIN_API.md`** — sibling §18 doc; the Tier-2 host-side `Plugin` trait surface (parallel substrate; this doc is the Tier-3 substrate).
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; Tier-2 plugin-author guide (parallel reference for the Tier-2 path).
- **`EXECUTION_DOMAINS.md`** — sibling §18 doc; per-domain failure-class implications, including the Expression / Script row that script-host underlies.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; `Severity` + `DiagnosticAggregator` the wasm `rge.diagnostic::emit` host function routes through.
- **`KERNEL_ECS_WORLD.md`** — sibling §18 doc; `World::query` / `spawn` / `despawn` the ECS bridge wraps.
- **`KERNEL_EVENTS_CHANNEL.md`** — sibling §18 doc; `EventBus` + `SubscriptionId` the EventHooks tracker references.
- **`crates/script-host/src/lib.rs`** — module roots + failure-class declaration + safety policy paragraph.
- **`crates/script-host/src/host_state.rs`** — call-scope pointer pattern + the 5 SAFETY-proven `unsafe`-allow sites.
- **`crates/script-host/src/ecs_bridge.rs`** — `EcsBridge::install` wires 7 wasm host functions onto the linker; `Counter` prototype component; `entity_id_to_i64` low-63-bit mask.
- **`crates/script-host/src/script_module.rs`** — `ScriptModule` (compiled) + `ScriptInstance` (live `Store<HostState>` + `Instance`) + `ScriptError` taxonomy.
- **`crates/script-host/src/swap.rs`** — `capture_state` / `restore_state` + `SwapPlan` + `SwapResult` + `SwapError` + `CounterSnapshot` RON format.
- **`crates/script-host/src/event_hooks.rs`** — advisory subscription tracker; full host-function wiring deferred to Phase 4.
- **`crates/script-host/tests/cold_start_smoke.rs`** — cold-start budget smoke + content-addressed digest regression (2 tests).
- **`crates/script-host/tests/host_panic_isolation.rs`** — trap-isolation regression (1 test); the plugin-fatal Tier-3 contract.
- **`crates/script-host/tests/swap_smoke.rs`** — full 9-step swap window regression (1 test); the substrate-gate measurement point.
- **`crates/script-bench`** — Tier-2 follower; Phase 3.4 will rewire against real script-host for the formal 1000-entity p95 bench.
