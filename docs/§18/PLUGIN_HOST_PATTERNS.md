# PLUGIN_HOST_PATTERNS

| Companion to | ADR-114 (PluginContext owned-resources-handoff design) |
|---|---|
| Status | First §18 companion-doc landing 2026-05-08 (per PLAN.md §18); convention defined here for subsequent §18 docs |
| Audience | Authors of new Tier-2 plugins (gfx, physics, audio, editor-ui, cad-projection, …) targeting the unified `Plugin` trait per PLAN §10.4 dogfood rule |
| Sibling doc | `PLUGIN_API.md` — API-surface reference; this doc focuses on *how* to author, that doc on *what* the substrate exposes |
| Reference impls | `crates/cad-projection/src/plugin_adapter.rs` (straight-line) · `crates/gfx/src/plugin_adapter.rs` (lazy-build) · `crates/physics/src/plugin_adapter.rs` (straight-line, no-`RuntimeFault` subcase) |

> **Convention for §18 companion docs (defined by this doc and `PLUGIN_API.md`).**
> Each §18 doc lives at `docs/§18/<TOPIC>.md`. The header carries the same five-row table as above (Companion-to / Status / Audience / Sibling doc / Reference impls). Sections are numbered for easy citation from ADRs. Code blocks stay short (<30 lines) — for full canonical examples, link to the source `.rs` files instead of duplicating them inline.

## 1. Overview

The `Plugin` trait + `PluginContext` substrate (lives in `kernel/plugin-host`) is the shape every Tier-2 subsystem implements per PLAN §10.4 dogfood rule: gfx, physics, audio, editor-ui, cad-projection, and any future Tier-3 sandboxed WASM plugin. The trait has three lifecycle methods (`init` / `tick` / `shutdown`); the context exposes a type-erased resource registry plus a diagnostic sink.

This doc captures the canonical authoring patterns surfaced across the three Tier-2 plugin canaries that landed 2026-05-07 / 2026-05-08. It is the "how to author one" companion to ADR-114's "why this design", and the reference for anyone authoring the fourth or fifth canary. The substrate's design rationale is in **ADR-114** §"Decision" (sub-decisions 1–3) and §"Alternatives explicitly NOT chosen and why"; the API surface is in `PLUGIN_API.md`.

This doc focuses on the patterns that *generalise*. Sections 3–7 are pattern-level; section 8 is a test-recipe template directly transcribable into a new canary's test module.

## 2. The owned-handoff contract (5-line summary)

1. The orchestrator `insert<T>(value)`s required resources into the context **before** every plugin lifecycle call.
2. The plugin `take<T>()`s exactly the resources it needs at the start of its body.
3. The plugin does its work with the owned resources.
4. The plugin `insert<T>(value)`s the resources back at the end of its body — *regardless of success or failure*.
5. The host snapshots the registry's `BTreeSet<TypeId>` before and after the call (wrapped in `catch_unwind`) and diffs to detect leaks. A plugin that took a resource but didn't put it back is detected and surfaced as a structured diagnostic — *whether the plugin returned `Ok`, `Err`, or panicked*.

The full lifecycle, panic-safety wrap, and pre/post-snapshot machinery are documented in ADR-114 §"Decision" + §"Implementation guidance / Host-side wrap". The plugin author's responsibility is steps 2–4; the host owns steps 1 and 5.

## 3. Pattern A — straight-line tick

### When to use

Use straight-line tick when **every resource the plugin needs at tick time can be staged by the orchestrator before the call**. This is the default; reach for Pattern B only when the resource shape forces it.

The two reference impls are `cad-projection` (drives a `CadProjection` against a staged `World` + `CadGraph` + `Tolerance`) and `physics` (drives `physics_step` against a staged `World` + `PhysicsInputLedger`). Both keep the plugin struct close to zero state — only an incidental success-counter (e.g. `frames_recorded`, `steps_run`) for orchestrator-side liveness.

### Structure

The body is: take all required resources sequentially (with idempotent failure put-back on any miss); do the inner work; insert all resources back; map the inner result onto the `PluginError` taxonomy. Pseudocode:

```rust
fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
    // Take #1 — required.
    let mut a = ctx
        .take::<A>()
        .ok_or_else(|| PluginError::contract_violation("A"))?;

    // Take #2 — required; put A back if missing (idempotent failure).
    let Some(b) = ctx.take::<B>() else {
        let _ = ctx.insert(a);
        return Err(PluginError::contract_violation("B"));
    };

    // Optional take with default fallback.
    let c = ctx.take::<C>().unwrap_or_default();

    let result = self.inner_work(&mut a, &b, c);

    // Always put resources back (even on result == Err) so the
    // orchestrator can recover them.
    let _ = ctx.insert(a);
    let _ = ctx.insert(b);
    let _ = ctx.insert(c);

    // Map inner result onto the PluginError taxonomy.
    result.map_err(|e| PluginError::runtime_fault(format!("inner failed: {e}")))
}
```

### Notes for the author

- **`debug_assert!` the put-back returns `None`.** After a `take`, the slot is empty; the corresponding `insert` should return `None`. Asserting this catches a class of resource-shape mistakes (e.g. accidentally inserting twice) at debug-build cost zero. Both cad-projection and physics canaries use this pattern.
- **Use `let-else` for missing-required-resource branches.** It expresses "if the resource is missing, return early after putting back what we already took" tightly. Both canaries use it.
- **`Tolerance`-style optional resources fall back to a default.** cad-projection's tolerance is optional; the canary uses `unwrap_or_else(|| Tolerance::new(0.001).expect(...))`. Optional resources MUST still be put back if they were `take`n with a `Some` result, so the orchestrator's view of the registry stays consistent across calls. The canary's pattern handles this correctly because `take` only returns `Some` when the slot was occupied.

### Reference impls (canonical examples)

- `crates/cad-projection/src/plugin_adapter.rs` `impl Plugin for CadProjectionPlugin :: tick` — three takes (`World` + `CadGraph` + optional `Tolerance`), one inner call, three inserts, mapped result.
- `crates/physics/src/plugin_adapter.rs` `impl Plugin for PhysicsPlugin :: tick` — two takes (`World` + `PhysicsInputLedger`), one inner call, two inserts, no result-mapping (the inner work is infallible — see §5 no-`RuntimeFault` subcase).

## 4. Pattern B — lazy-build-on-first-tick

### When to use

Use lazy-build when **a resource the plugin holds internally requires a `&Resource` from the context to construct**. The orchestrator can't reasonably stage that resource before `init` (init has no `tick`-time context yet), so the plugin must defer construction to the first `tick` and cache the result.

The reference impl is `gfx`: `TrianglePipeline::new(gfx_ctx, target.format())` requires both a live `&GfxContext` AND knowledge of the target's format. Neither is available at `init` time (no `GfxContext` is staged before `init` runs in gfx's lifecycle), so the pipeline must be built lazily on the first tick.

### Structure

The plugin holds an `Option<Resource>` field initialised to `None`. The first `tick` checks `is_none()`, builds via a fallible constructor, and assigns. Subsequent ticks reuse the built value. Pseudocode:

```rust
pub struct LazyPlugin {
    resource: Option<MyResource>,
    // ... other state
}

impl Plugin for LazyPlugin {
    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No-op — lazy resource is built on first tick.
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let dep_a = ctx
            .take::<DepA>()
            .ok_or_else(|| PluginError::contract_violation("DepA"))?;
        let Some(dep_b) = ctx.take::<DepB>() else {
            let _ = ctx.insert(dep_a);
            return Err(PluginError::contract_violation("DepB"));
        };

        let result = self.tick_inner(&dep_a, &dep_b);

        let _ = ctx.insert(dep_a);
        let _ = ctx.insert(dep_b);
        result
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(()) // RAII cleans up self.resource at drop.
    }
}

impl LazyPlugin {
    fn tick_inner(&mut self, dep_a: &DepA, dep_b: &DepB) -> Result<(), PluginError> {
        // Lazy build on first tick; subsequent ticks reuse.
        if self.resource.is_none() {
            let r = MyResource::new(dep_a, dep_b.format()).map_err(|e| {
                PluginError::runtime_fault(format!("resource build failed: {e}"))
            })?;
            self.resource = Some(r);
        }
        let r = self.resource.as_ref().expect("just built or already present");
        do_work(r, dep_a, dep_b);
        Ok(())
    }
}
```

### Notes for the author

- **Pull the inner body out into a separate method.** The gfx canary uses `tick_inner(&self, &gfx_ctx, &target)` so the resource put-back path in the outer `tick` stays straight-line. This is a readability discipline, not a soundness requirement, but it makes the ContractViolation / RuntimeFault separation visually obvious. See `crates/gfx/src/plugin_adapter.rs` `GfxPlugin::tick_inner`.
- **The `is_none()` branch is one extra check per tick after the first.** Negligible at plugin-tick rate. If the per-tick cost ever surfaces as a measurable hot-spot, refactor to a state-machine (`enum LazyState { Pending, Built(Resource), Failed }`) to remove the option-check; not necessary for v0.
- **A failed lazy build leaves `self.resource` as `None`.** This means subsequent ticks will retry the build. If retries should be suppressed (e.g. shader compilation failed; unlikely to succeed on next tick), introduce a third state like `Failed { reason }` and short-circuit. Not required by the canary.
- **Lazy-built resources aren't on the orchestrator's registry.** They live on the plugin struct and are dropped when the plugin is dropped (RAII). The orchestrator NEVER sees them in the resource snapshot; the leak-detection diff doesn't fire on them. This is correct: the resources the orchestrator owns are the ones it staged.
- **`Debug` impl needs a manual hand-roll.** Resources like `wgpu::RenderPipeline` aren't `Debug`. The gfx canary derives a custom impl that prints `pipeline_built: bool` instead of the underlying handle. See `impl std::fmt::Debug for GfxPlugin`.

### Reference impl (canonical example)

- `crates/gfx/src/plugin_adapter.rs` `impl Plugin for GfxPlugin :: tick` + `GfxPlugin::tick_inner` — two takes (`GfxContext` + `HeadlessTarget`), lazy `TrianglePipeline` build using `&GfxContext`, two inserts, mapped result. The pipeline build is wrapped in `RuntimeFault` because the build itself is fallible (the WGSL compile path).

## 5. Error classification cheat-sheet

Map an error scenario to the corresponding `PluginError` variant. The host's auto-emit downgrades / elevates each variant to the right `Diagnostic` severity per ADR-114 §"PluginError variant policy".

| Scenario | Variant | Auto-emit severity | Blame |
|---|---|---|---|
| Required resource missing from `ctx` | `ContractViolation { resource_type }` (use `&'static str`) | `Warning` | Caller (orchestrator failed to stage) |
| Inner work returned an error (fallible API) | `RuntimeFault { reason }` | `Error` | Plugin (its own logic failed) |
| Inner work is infallible (returns `()`) | *no `RuntimeFault` mapping needed* — see no-`RuntimeFault` subcase below | n/a | n/a |
| Plugin panicked | `Panic { phase, payload }` (host-constructed) | `Error` | Plugin (host-classified, host-recovered) |
| `init` returned an error (resource unavailable, validation, etc.) | `InitFailed { reason }` | `Error` | Plugin |
| `shutdown` returned an error | `ShutdownFailed { reason }` (host-initiated unregister surfaces this as `Warning` instead) | `Error` (or `Warning` for unregister) | Plugin |

### The no-`RuntimeFault` straight-line subcase

The physics canary's `physics_step` is **infallible** at the call boundary (returns `()`, not `Result<(), _>`). There is no failure path to map onto `RuntimeFault`, and the variant is statically unreachable in the canary's `tick`. **This is acceptable v0 design**: the variant remains *reserved* in the canary for future fallible-step extensions (e.g. joint-build paths, rapier3d API upgrades that surface step errors, optional per-step validity gates), but it is not aliased to `ContractViolation` (would conflate plugin bugs with caller-misconfigured ctx) and not repurposed for missing-resource paths.

When you author a plugin whose inner work is infallible at the call boundary, follow physics's pattern: do the work, insert resources back, return `Ok(())` unconditionally. Document the infallibility in the module-level doc-comment so future maintainers don't add a speculative `RuntimeFault` mapping. See `crates/physics/src/plugin_adapter.rs` module-doc § "Resource contract" — the line "Tick is infallible at the plugin-adapter level" is the canonical recipe-instance.

### Why `&'static str` for `ContractViolation`

The variant carries a `resource_type: &'static str` so it doesn't allocate on the failure path and can be matched at zero cost. Use the type's name as the literal: `PluginError::contract_violation("World")`, `PluginError::contract_violation("CadGraph")`. Don't use `format!("{:?}", T)` or runtime-derived names — the variant's value-equality story relies on the static string.

## 6. The idempotent-failure-put-back invariant

If the plugin's `take<T1>()` succeeds and the subsequent `take<T2>()` fails, the plugin **MUST put `T1` back into the context before returning the error**. The orchestrator's invariant is that any resource it staged before a plugin call is recoverable after the call regardless of outcome — `Ok`, `Err`, or `Panic`. Failing to put a partial-take back leaves the orchestrator's view of the registry inconsistent with what it expected.

The canonical shape uses `let-else`:

```rust
let mut a = ctx.take::<A>().ok_or_else(|| PluginError::contract_violation("A"))?;
let Some(b) = ctx.take::<B>() else {
    let _ = ctx.insert(a);
    return Err(PluginError::contract_violation("B"));
};
// ... continue with both a and b held by the plugin ...
```

If a third resource fails after a + b succeed, the put-back grows accordingly:

```rust
let Some(c) = ctx.take::<C>() else {
    let _ = ctx.insert(a);
    let _ = ctx.insert(b);
    return Err(PluginError::contract_violation("C"));
};
```

For complex multi-resource plugins, prefer a guard pattern (RAII-style) where the put-back happens on drop. None of the three canaries needed this yet; the let-else shape is fine for two-to-three required resources.

### Validation in the integration tests

Both the gfx and physics canaries validate this invariant explicitly:

- `gfx::plugin_adapter_smoke::*` integration suite — there are tests (the missing-second-resource branch) that stage `GfxContext` only (skip `HeadlessTarget`), call `tick`, assert `Err(ContractViolation { resource_type: "HeadlessTarget" })`, and assert `ctx.contains::<GfxContext>()` AFTER the call. The first take must have been put back. See `crates/gfx/tests/plugin_adapter_smoke.rs` for the canonical test.
- `physics::plugin_adapter_smoke::*` integration suite — same pattern: stage `World` only, assert `Err(ContractViolation { resource_type: "PhysicsInputLedger" })`, assert `ctx.contains::<World>()` afterwards. See `crates/physics/tests/plugin_adapter_smoke.rs` and the inline unit test `physics_plugin_tick_with_world_only_returns_contract_violation_for_input_ledger` in `plugin_adapter.rs`.

When you author a new plugin, write the analogous test as the second-required-resource case in the unit suite. The pattern is mechanical: insert N-1 of N required resources, call `tick`, assert the missing one's `ContractViolation`, assert the N-1 you DID supply are still present.

## 7. Multi-plugin isolation guarantees

### What the host gives you

- **Failure isolation per PLAN §1.13.** A plugin that fails (`Err`, panic, leaked resource) is marked `Failed` by the host and skipped on subsequent `tick_all` and `shutdown_all` calls. Other plugins continue running.
- **`catch_unwind` panic recovery.** Every host → plugin call is wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))`. A panicking plugin's panic is caught, the plugin is marked `Failed`, the orchestrator's state is preserved, and the host emits `PluginError::Panic { phase, payload }` plus a separate leak diagnostic if the panic happened mid-take.
- **Pre/post-snapshot leak detection.** The host snapshots the resource registry's `BTreeSet<TypeId>` before each call and again after, regardless of outcome. A plugin that took a resource but didn't put it back triggers a structured warning (or error, on the failure path).

### What you must NOT do

- **Don't call other plugins from your `tick`.** The orchestrator owns scheduling. Calling another plugin's methods from your body bypasses lifecycle ordering, breaks the panic-recovery boundary (your panic would surface inside the wrong call site), and circumvents the host's failure isolation.
- **Don't snapshot or inspect the resource registry from the plugin side.** `snapshot_resource_ids` is `pub(crate)` for a reason: the host owns the inspection surface. If a plugin needs to know whether a resource is staged, use `ctx.contains::<T>()` for that single type. Bulk inspection is a host-only concern.
- **Don't hold `&mut PluginContext<'_>` across `await` points** (when async lands; not yet in v0). The borrow's lifetime is the call body's duration; storing it elsewhere is a soundness violation when the host retakes it.
- **Don't synthesize `PluginError::Panic` from inside your code.** The variant has no public constructor; it is host-classified. If your code wants to signal "a soft fault occurred", use `PluginError::RuntimeFault` instead.

### The `PanickingTickPlugin` sibling-fixture pattern

Each canary's integration-test suite includes a sibling fixture (e.g. `PanickingTickPlugin` in `cad-projection/tests/`) that panics inside `tick`. The orchestrator-level tests then register both the real canary and the panicking sibling, run `tick_all`, and assert:

1. The panicking plugin is marked `Failed` and surfaces a `Panic { phase: Tick, payload: ... }` diagnostic.
2. The real canary continues running and its tick completes normally.
3. Resources held by the panicking sibling at the moment of panic are reported as leaked; resources held by the real canary stay consistent.

This is the canonical multi-plugin isolation test. Adopt the pattern in any new canary's integration suite — the fixture is ~30 lines (a struct with a `panic!()` in `tick`) and the assertion shape is ~50 lines. See `crates/cad-projection/tests/projection_error_coverage.rs` for the existing reference.

## 8. Test recipe template

Every canary that landed (cad-projection / gfx / physics) shipped with a 12–16-test split distributed across the inline unit suite (`#[cfg(test)] mod tests` in `plugin_adapter.rs`) and an integration test file (`crates/<name>/tests/plugin_adapter_smoke.rs` or analogous). The template:

### Unit tests (in `plugin_adapter.rs`)

1. **`*_id_matches_convention`** — assert `plugin.id() == PluginId::new(<CRATE_PLUGIN_ID>)`.
2. **`*_name_is_stable_human_readable_string`** — assert the human-readable name (overrides `Plugin::name`'s default `""` if the canary cares).
3. **`*_default_impl_matches_new`** — `Default::default()` must round-trip to the same state as `new()` for the field set the canary owns.
4. **`*_starts_at_zero` / `*_starts_unbuilt`** — initial-state predicates (counters at zero, `Option<T>` fields `None`, etc.).
5. **`*_init_succeeds_without_resources`** — `init` is a no-op for the canonical patterns; assert it returns `Ok` even with an empty context.
6. **`*_tick_with_no_resources_returns_contract_violation_for_<first_resource>`** — assert the first-required-resource error path.
7. **`*_tick_with_<first>_only_returns_contract_violation_for_<second>`** (for ≥2-required-resource canaries) — assert idempotent failure put-back: the first resource must remain in the registry; the error variant names the second resource.
8. **`*_tick_advances_<state>_when_resources_supplied`** — happy-path with all resources staged; assert the inner work ran (counter incremented, world tick advanced, ledger appended, etc.).
9. **`*_shutdown_succeeds_without_resources`** — `shutdown` is a no-op for the canonical patterns; assert it returns `Ok` even with an empty context.

### Integration tests (in `crates/<name>/tests/plugin_adapter_smoke.rs`)

10. **Cross-plugin lifecycle.** Register the canary with a real `PluginHost`, drive `init_all` / `tick_all` / `shutdown_all`, assert state transitions (`Pending` → `Initialized` → `Shutdown`).
11. **Resource-leak detection on `Ok`.** Construct a fixture plugin that returns `Ok` without putting back a resource it took; assert the host emits a leak diagnostic and (depending on policy) marks the plugin `Failed` or just emits the warning.
12. **Multi-plugin sibling isolation.** Register the canary and a `PanickingTickPlugin`; tick both; assert the canary continues while the sibling is marked `Failed` with a `Panic` diagnostic.

### Pattern-specific tests

13. **(Pattern B only) Lazy-build state assertions.** Assert `plugin.pipeline_built() == false` after `init` (resource not staged); assert `true` after one successful `tick`.
14. **(Pattern A no-`RuntimeFault` subcase) Determinism soak.** For canaries whose inner work is infallible, the failure surface narrows to caller misconfiguration. Use the freed test budget on a determinism assertion: tick N times, check `BLAKE3(serialized_world)` byte-identity across runs. See `crates/physics/tests/deterministic_replay.rs`.
15. **(Pattern A fallible inner) `RuntimeFault` propagation.** Force the inner work to fail; assert the resulting `Err` is `PluginError::RuntimeFault { reason: contains the inner error message }`; assert resources are still in the registry afterwards.

This split lands at 12–16 tests for a v0 canary, scales to 20+ when integration scenarios deepen.

## 9. References

- **ADR-114** — design rationale for the owned-handoff substrate; see §"Decision" + §"Implementation guidance" + §"Amendment 2026-05-08 — Three-substrate validation".
- **`PLUGIN_API.md`** — sibling §18 doc; full API surface for `kernel/plugin-host`. Use it as the type-level reference while authoring; this doc is the pattern-level guide.
- **PLAN.md §10.4** — dogfood rule; Tier-2 plugins use the same `Plugin` trait as Tier-3.
- **PLAN.md §1.13** — failure containment model; plugin-fatal isolation.
- **PLAN.md §18** — companion-doc index; this doc is the first §18 landing.
- **`kernel/plugin-host/src/plugin.rs`** — `Plugin` trait, `PluginError` taxonomy, `PluginPhase` enum.
- **`kernel/plugin-host/src/context.rs`** — `PluginContext` with type-erased resource registry.
- **`kernel/plugin-host/src/host.rs`** — `PluginHost` lifecycle manager + `catch_unwind` + leak-detection wrap.
- **`crates/cad-projection/src/plugin_adapter.rs`** — Pattern A canary; CAD-graph resource family.
- **`crates/gfx/src/plugin_adapter.rs`** — Pattern B canary; GPU resource family.
- **`crates/physics/src/plugin_adapter.rs`** — Pattern A canary with no-`RuntimeFault` subcase; physics-world resource family.
