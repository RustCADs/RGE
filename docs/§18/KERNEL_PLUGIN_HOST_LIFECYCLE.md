# KERNEL_PLUGIN_HOST_LIFECYCLE

| Companion to | ADR-114 (PluginContext owned-resources-handoff design) + PLAN.md §10.4 (dogfood rule) + PLAN.md §1.13 (failure-class taxonomy) |
|---|---|
| Status | Stable v1; lifecycle hardened post-2026-05-08 audit-2 Phase 0 + LOW #5 closures (catch_unwind shield + leak-detection diff + per-phase auto-emit policy regression-tested) |
| Audience | plugin-host maintainers + advanced plugin authors who need to reason about lifecycle ordering, failure isolation, and resource-leak detection invariants |
| Sibling doc | `PLUGIN_API.md` — author-facing API surface; `PLUGIN_HOST_PATTERNS.md` — pattern-level guide for canary authors |
| Reference impls | `kernel/plugin-host/src/host.rs` (1766L; carries `// SPLIT-EXEMPTION:`) · `kernel/plugin-host/src/plugin.rs` (`Plugin` trait + `PluginError` taxonomy + `PluginPhase`) · `kernel/plugin-host/src/context.rs` (`PluginContext` registry + `snapshot_resource_ids`) · `kernel/plugin-host/src/lib.rs` (failure-class + dogfood declaration) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. For the plugin-author API surface read `PLUGIN_API.md` first; for canary authoring patterns read `PLUGIN_HOST_PATTERNS.md`. This doc covers the *host-side* lifecycle machinery — state machine, reports, leak detection, locking, the SPLIT-EXEMPTION justification.

## 1. Scope

This doc covers the host SIDE of the plugin substrate. Specifically:

- The [`PluginRecord`] state machine (5 states, transition diagram in §3).
- The [`InitReport`] / [`TickReport`] / [`ShutdownReport`] aggregator types returned by lifecycle methods.
- The `catch_unwind` shield wrapping every host → plugin call.
- The pre/post resource-snapshot diff that detects leaked resources.
- The auto-emit policy that classifies each failure path's `Diagnostic::Severity`.
- The 1766L `host.rs` SPLIT-EXEMPTION justification.
- The LIFO shutdown ordering rationale.
- The plugin-fatal isolation guarantee per PLAN §1.13.
- The "untrusted execution domains" framing.

For the plugin-author API (the `Plugin` trait surface + `PluginContext::take` / `insert` / `with_resource`) see `PLUGIN_API.md`. For canary patterns (straight-line tick, lazy-build-on-first-tick, idempotent failure put-back) see `PLUGIN_HOST_PATTERNS.md`.

## 2. The five states

[`PluginState`] (defined at `kernel/plugin-host/src/host.rs`):

```rust
pub enum PluginState {
    Pending,
    Initialized,
    Failed,
    ShuttingDown,
    Shutdown,
}
```

Semantics:

- **`Pending`** — the plugin is registered (the host knows about it) but [`Plugin::init`] has not been called. Plugins enter this state via [`PluginHost::register`].
- **`Initialized`** — the plugin's `init` returned `Ok` and reported no leak. Only `Initialized` plugins receive [`tick_all`] calls and are eligible for the LIFO `shutdown_all` walk.
- **`Failed`** — any lifecycle method returned `Err`, panicked, or returned `Ok` while leaking a resource. Failed plugins are skipped by subsequent `tick_all` and `shutdown_all` (their `shutdown` is never called twice).
- **`ShuttingDown`** — transient. A `shutdown` (host-driven `shutdown_all` or user-driven `unregister`) is currently in progress. Visible only inside the lifecycle method body; in-process consumers don't see this state from the public `state(&id)` accessor in normal use.
- **`Shutdown`** — terminal. The plugin's `shutdown` returned `Ok` cleanly. The host has dropped the plugin from the active registry, but the state is reportable via the `ShutdownReport` returned by `shutdown_all`.

## 3. State machine

```text
                    register
                       │
                       ▼
                  ┌──────────┐
                  │ Pending  │
                  └──────────┘
                       │
                  init_all
                  ┌────┴────┐
              Ok? │         │ Err / panic / leak
                  ▼         ▼
            ┌──────────┐  ┌────────┐
            │Initialized│ │ Failed │
            └──────────┘  └────────┘
                  │           │
       tick_all   │           │  (tick_all + shutdown_all skip Failed)
       ┌──────────┤
   Ok? │          │ Err / panic / leak  →  Failed
       │          ▼
       │      (stays Initialized)
       │
shutdown_all (LIFO)  /  unregister (any-order)
       │
       ▼
  ShuttingDown  ───── Ok ─────▶ Shutdown
       │
       └─── Err / panic / leak ─▶ Failed
```

Transitions are mechanical:

- **`register` → `Pending`.** [`PluginHost::register`] validates `Plugin::id() == registered_id` (rejects with `PluginHostError::IdMismatch` otherwise) + non-duplicate id (rejects with `DuplicateId`), pushes onto `insertion_order`, inserts into `BTreeMap<PluginId, PluginRecord>`. Plugin starts at `Pending`.
- **`init_all` walks `Pending` → `Initialized | Failed`.** Calls `init` once per plugin in registration order; classifies the outcome via `catch_unwind` + leak-diff (see §5).
- **First `tick_all` is the implicit "Active" milestone.** Note: there is no `Active` state in source; the dispatch spec listed one but the actual implementation uses just `Initialized` for both pre-first-tick and post-first-tick. This doc reflects source-truth: a plugin that has been initialized stays `Initialized` until it fails or shuts down.
- **`shutdown_all` walks `Initialized` → `ShuttingDown` → `Shutdown | Failed` in LIFO order.** Reverse-iterates `insertion_order` and removes from the registry as it goes; calls `shutdown` once per plugin still alive; skips `Failed` (their `shutdown` is never called twice).

> **Source-truth flag:** the dispatch spec described 5 states with `Active` as a distinct post-first-tick state. The actual surface is 5 states with `Initialized` covering both pre-first-tick and post-tick stable operation. This doc reflects the source-truth.

## 4. The lifecycle reports

Three aggregator types, returned per-call:

```rust
pub struct InitReport {
    pub initialized: Vec<PluginId>,
    pub failed: Vec<(PluginId, String)>,
}

pub struct TickReport {
    pub ticked: usize,
    pub failed: Vec<(PluginId, String)>,
}

pub struct ShutdownReport {
    pub shutdown: Vec<PluginId>,
    pub failed: Vec<(PluginId, String)>,
}
```

Each carries success + failure parallel lists. `TickReport::ticked` is a counter rather than a list because tick is per-frame and the per-call success set is large; the orchestrator typically only cares about how many succeeded vs failed. The failure lists carry `(PluginId, String)` where the `String` is the formatted plugin-error message (already routed to the diagnostic stream by the host's auto-emit policy — see §7).

> **Source-truth flag:** the dispatch spec described all three reports as carrying `succeeded: Vec<PluginId>` + `failed: Vec<(PluginId, PluginError)>`. The actual surface uses `initialized` / `shutdown` for the success list (per-report) and `String` rather than `PluginError` for the failure-side payload (because `PluginError` doesn't currently impl `Clone`, the host pre-formats the message before pushing to the report). This doc reflects the source-truth.

## 5. The catch_unwind shield

Every direct call into a plugin's lifecycle method is wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))`. From `host.rs`:

```rust
let pre_call_resources = ctx.snapshot_resource_ids();
let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    record.plugin.init(ctx) // or .tick(ctx) / .shutdown(ctx)
}));
let post_call_resources = ctx.snapshot_resource_ids();
let leaked: Vec<_> = pre_call_resources
    .difference(&post_call_resources)
    .copied()
    .collect();
```

`AssertUnwindSafe` is sound here because the surrounding scope is the panic-recovery boundary itself — the host explicitly chose this isolation point and accepts that any state inside the closure may be partially modified. Cross-ref ADR-114 §"Decision sub-decision 3" for the design rationale.

The shield's three jobs:

1. **Catch panics.** A `panic!` inside the plugin body becomes `Err(panic_payload: Box<dyn Any + Send>)` rather than unwinding through the host's stack frame.
2. **Snapshot resources before AND after.** Both `BTreeSet<TypeId>` snapshots happen regardless of `Ok` / `Err` / panic — so leak detection works on every outcome path.
3. **Route the outcome.** The match arm pattern is `Ok(Ok(()))` (success) / `Ok(Err(plugin_err))` (plugin-returned error) / `Err(panic_payload)` (caught panic). Each arm has its own diagnostic-emit + state-transition path.

## 6. Resource-leak detection invariant

The orchestrator stages resources into the `PluginContext` registry before each lifecycle call (per `PLUGIN_HOST_PATTERNS.md` §3 owned-handoff contract). The plugin takes resources, does work, puts them back. If a plugin took a resource (any reason: panic, early return, forgotten `insert`) without putting it back, the registry's `BTreeSet<TypeId>` snapshot has a hole.

The host detects the hole via the snapshot diff (`pre_call.difference(&post_call)`) and surfaces it as a structured diagnostic. The leak diagnostic format (per `host.rs`):

```
"plugin {id} leaked {n} resource(s) on {phase} failure"        // on Err path
"plugin {id} returned Ok but leaked {n} resource(s); orchestrator state may be incomplete"
"plugin {id} leaked {n} resource(s) during panic; orchestrator state may be incomplete"
```

A leak on the success path (`Ok(())` + non-empty leaked) is **disciplinary failure** — the plugin returned `Ok` but didn't put back what it took. The host marks the plugin `Failed` and adds to the report's `failed` list. A leak on the error or panic path is compounded (the underlying error already failed the plugin; the leak is reported separately for forensics).

Cross-ref `PLUGIN_API.md` §3 ("`PluginError` taxonomy") + `KERNEL_DIAGNOSTICS.md` §9 ("Plugin-host auto-emit policy") for the consumer surface.

## 7. Auto-emit policy

Per audit-2 Phase 0 + LOW #5 closures (HANDOFF.md, 2026-05-08), the host auto-emits a structured `Diagnostic` on every plugin Err / Panic / leak path. The central dispatch helper (`emit_plugin_err_diagnostic`) lives at `host.rs` lines 666-678:

```rust
fn emit_plugin_err_diagnostic(
    ctx: &mut PluginContext<'_>,
    id: &PluginId,
    phase: PluginPhase,
    plugin_err: &PluginError,
) {
    let msg = format!("plugin {id} {phase} failed: {plugin_err}");
    let diag = match plugin_err {
        PluginError::ContractViolation { .. } => Diagnostic::warning(msg),
        _ => Diagnostic::error(msg),
    };
    ctx.emit_diagnostic(diag);
}
```

### Severity table

| Failure path | Auto-emit Severity | Rationale |
|---|---|---|
| `PluginError::ContractViolation { resource_type }` | `Warning` | Caller misconfiguration (orchestrator failed to stage); not a plugin bug. Avoids 60Hz error-spam from a misconfigured ctx during steady-state. |
| `PluginError::RuntimeFault { reason }` | `Error` | Genuine plugin-side failure. |
| `PluginError::InitFailed { reason }` | `Error` | Genuine plugin-side init failure. |
| `PluginError::Panic { phase, payload }` | `Error` | Host-classified; resources held by the panicking plugin are unrecoverable. |
| `PluginError::ShutdownFailed { reason }` (`shutdown_all`-driven) | `Error` | Plugin's own teardown raised — real failure. |
| `PluginError::ShutdownFailed { reason }` (host-initiated `unregister`) | `Warning` | Host explicitly invoked the unregister; teardown imperfection isn't an "engine is broken" signal. |
| Leak on `Ok` return | `Error` | Disciplinary failure — plugin returned `Ok` but didn't put back what it took. Marks the plugin `Failed`. |
| Leak on `Err` return | `Error` | Compounded failure on top of the underlying error. |
| Leak on Panic | `Error` | Compounded — leak + panic. |
| Leak on `unregister`-shutdown (any outcome) | `Warning` | Host-initiated; non-fatal by design (matches the unregister policy). |

All emitted via `&mut DiagnosticSink` borrowed from `PluginContext::diagnostics()`. The discrimination is pinned by two regression tests in `host.rs`: `tick_all_emits_warning_for_contract_violation` (line 1675) and `unregister_emits_warning_on_shutdown_failure` (line 1726).

Cross-ref `KERNEL_DIAGNOSTICS.md` §9 + `PLUGIN_API.md` §3 for the consumer-side and design rationale.

## 8. The 1766L `host.rs` SPLIT-EXEMPTION

PLAN §1.3 Rule 3 requires any `.rs` >1000L to carry a `// SPLIT-EXEMPTION:` annotation justifying why splitting would harm cohesion. `host.rs` carries the annotation as the first line of the file:

```text
// SPLIT-EXEMPTION: implementation is ~670 lines; the rest is a unit-test
// suite (a TestPlugin behavior matrix + ~25 lifecycle tests covering
// register / init / tick / shutdown / unregister + the Phase 0 audit-2
// catch_unwind / leak-detection / contract-violation / panic-recovery
// closures). Splitting host.rs from its test module would force tests into
// a sibling file and lose `super::*` access to the private `TestPlugin`
// struct without a meaningful reduction in cohesion.
```

The breakdown:

- **~670L impl** — `PluginState` / `PluginRecord` / `PluginHostError` / `PluginHost` + `register` / `unregister` / `init_all` / `tick_all` / `shutdown_all` / `get` / `state` / `count` / `iter_ids` + `InitReport` / `TickReport` / `ShutdownReport` + the two private helpers `emit_plugin_err_diagnostic` and `panic_payload_to_string`.
- **~1100L tests** — a `TestPlugin` behaviour-matrix struct (configurable to fail at init / tick / shutdown / panic / leak) plus 25+ lifecycle tests + dogfood-smoke fixtures + the auto-emit + catch_unwind + leak-detection regression tests landed by audit-2 Phase 0 + LOW #5.

> **Source-truth flag:** the dispatch spec listed `host.rs` as 1743L. The actual line count is 1766L. The spec's "1743L" is consistent with a recent prior state; the doc reflects the source-truth count of 1766L.

Splitting would force the `TestPlugin` fixture into a separate module, losing `super::*` access to private types (`emit_plugin_err_diagnostic`, `panic_payload_to_string`) the regression tests need to exercise directly, without a meaningful reduction in cohesion. The `split-exemption` architecture lint accepts the annotation; cross-ref `tools/architecture-lints/src/split_exemption.rs` for the lint mechanics.

## 9. LIFO shutdown ordering

[`shutdown_all`] iterates `insertion_order` in **reverse**. Required so plugin A (registered first) can depend on plugin B (registered later) being alive during A's `init` and during steady-state ticks; when the host terminates, B is shut down first so A's `shutdown` can still consume B-staged resources if needed.

This mirrors process-tree teardown: shutdown order is the inverse of init order. Concretely, in `host.rs`:

```rust
pub fn shutdown_all(&mut self, ctx: &mut PluginContext<'_>) -> ShutdownReport {
    let mut report = ShutdownReport::default();
    let order: Vec<_> = self.insertion_order.iter().rev().cloned().collect();
    for id in order { /* ... shutdown each ... */ }
    self.insertion_order.clear();
    report
}
```

The `Vec<PluginId>` insertion-order side-table is the keystone — the `BTreeMap<PluginId, PluginRecord>` registry alone wouldn't give us insertion order (its iteration order is lexicographic on the id string). Lookups go through the BTreeMap; ordering goes through the Vec.

`unregister` does NOT respect LIFO — it removes the named plugin in any order. This is appropriate for user-driven unregister (the user picked the plugin to drop) where the cross-plugin dependency story is the user's responsibility.

## 10. Plugin-fatal isolation

One plugin's failure marks ITS record `Failed` but does NOT block other plugins. PLAN §1.13 plugin-fatal isolation is enforced mechanically by:

1. **The `catch_unwind` shield.** A panicking plugin's panic is caught at the host's frame, not propagated.
2. **The per-record state machine.** The `Failed` state is a terminal-during-this-session state: subsequent `tick_all` and `shutdown_all` walks skip the failed plugin.
3. **The per-call iteration.** `init_all` / `tick_all` / `shutdown_all` walk the insertion order from the top; each plugin's outcome is independent. A `for` loop with no early-exit on failure means plugin N+1 still gets called regardless of plugin N's outcome.

> The "untrusted execution domains" framing in `host.rs` module-doc per audit-2 Phase 0 + ChatGPT cross-review: plugin-host treats plugins as kernel/userspace boundary equivalent. The `catch_unwind` + leak-detection + state-machine machinery is the kernel-side enforcement of that boundary.

## 11. The `Plugin` trait + `PluginContext` interface

For a complete reference see `PLUGIN_API.md`. Summary for lifecycle context:

```rust
pub trait Plugin: Send + 'static {
    fn id(&self) -> PluginId;
    fn name(&self) -> &'static str { "" }
    fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>;
    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> { Ok(()) }
    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> { Ok(()) }
}
```

`Send + 'static` is required so the host can store plugins as `Box<dyn Plugin>` and (in a future cross-thread orchestrator) move them between threads. The `id() -> PluginId` is validated against the registered-id at `register` time (rejected with `PluginHostError::IdMismatch`).

The `&mut PluginContext<'_>` carries the type-erased resource registry + a `&mut dyn DiagnosticSink`. The plugin pulls resources via `ctx.take::<T>()`, does work, puts them back via `ctx.insert::<T>(...)`. Host-only inspection happens via `ctx.snapshot_resource_ids()` (a `pub(crate)` method called by the lifecycle wrappers per §5).

## 12. Performance characteristics

Auto-emit allocation cost (per the `host.rs` module-doc Phase 1 cleanup-pass):

- One `String` allocation per failure for the formatted message.
- Failures happen on the off-path; successful plugin calls emit nothing.
- At plugin-tick rate (~60Hz × N plugins), well under 1µs per allocation on commodity hardware. Negligible compared to the actual plugin tick body cost.

If high-throughput plugin-failure scenarios surface (e.g. continuously-misconfigured ctx hammering the auto-emit at 60Hz), a future dispatch could add rate-limiting or a structured `Diagnostic::Code` enum to dedupe. Today the simple String-format approach is sufficient — the documented allocation cost is the design's commitment.

## 13. Failure class

`kernel/plugin-host` declares `//! Failure class: plugin-fatal` per PLAN §1.13 (see `kernel/plugin-host/src/lib.rs`). The `architecture-lints` `failure-class` lint enforces the declaration.

Plugin-fatal means: a plugin failing during init / live / shutdown does not take down the kernel. The host marks the plugin `Failed`, surfaces a diagnostic, and the engine continues. The host itself failing (rare; host invariant violation) is also plugin-fatal — the engine continues without plugin support.

The catch_unwind shield + leak-detection diff + per-record state machine are the mechanical enforcement of this declaration. A plugin panicking after taking the [`rge_kernel_ecs::World`] would, before this hardening, permanently lose World from the orchestrator. Today the host catches the panic, reports the leak, marks the plugin `Failed`, and the orchestrator's other plugins continue with whatever resources weren't lost.

## 14. References

- **ADR-114** — design rationale for the owned-handoff substrate; see §"Decision" + §"Implementation guidance" + §"PluginError variant policy" + §"Amendment 2026-05-08 — Three-substrate validation".
- **PLAN.md §10.4** — dogfood rule; Tier-2 plugins use the same `Plugin` trait as Tier-3.
- **PLAN.md §1.13** — failure-class taxonomy; plugin-fatal isolation.
- **PLAN.md §1.3 Rule 3** — `// SPLIT-EXEMPTION:` annotation requirement for `.rs` files >1000L.
- **`PLUGIN_API.md`** — sibling §18 doc; full API surface for plugin authors. Use this for `Plugin` trait method semantics, `PluginContext` registry methods, `PluginError` constructor surface.
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; pattern-level guide for canary authors.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; plugin-host auto-emit consumer surface. The `emit_plugin_err_diagnostic` helper documented in §9 is the central dispatch.
- **`kernel/plugin-host/src/host.rs`** — `PluginHost` lifecycle manager + `catch_unwind` + leak-detection wrap + `emit_plugin_err_diagnostic` (lines 666-678) + per-phase severity discrimination + the regression tests (`tick_all_emits_warning_for_contract_violation` line 1675, `unregister_emits_warning_on_shutdown_failure` line 1726).
- **`kernel/plugin-host/src/plugin.rs`** — `Plugin` trait, `PluginError` taxonomy (5 variants; only 4 have public constructors), `PluginPhase` enum.
- **`kernel/plugin-host/src/context.rs`** — `PluginContext` with type-erased resource registry; `snapshot_resource_ids()` host-only inspection method.
- **`tools/architecture-lints/src/split_exemption.rs`** — the `split-exemption` lint that accepts host.rs's `// SPLIT-EXEMPTION:` annotation.
- **`tools/architecture-lints/src/failure_class.rs`** — the `failure-class` lint that enforces the lib.rs declaration.
