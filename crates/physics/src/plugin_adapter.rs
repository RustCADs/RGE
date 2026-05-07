//! `physics::PhysicsPlugin` — third real Tier-2 plugin canary per the §10.4
//! dogfood rule.
//!
//! Wraps a fixed-timestep physics tick and impls
//! [`rge_kernel_plugin_host::Plugin`]. `tick` extracts an owned [`World`] and
//! [`PhysicsInputLedger`] from the [`PluginContext`], advances the simulation by
//! exactly one [`FIXED_DT`](crate::FIXED_DT) step via [`physics_step`], and
//! puts both resources back into the context. Demonstrates that the v1
//! owned-handoff resource-registry generalises beyond cad-projection's
//! plain-Rust ECS-graph types and gfx's GPU device handles to a third
//! resource family — physics-world state owning the rapier3d arenas
//! (`RigidBodySet`, `ColliderSet`, `PhysicsPipeline`, broadphase, narrowphase,
//! islands, joints, CCD solver, integration parameters).
//!
//! # Why this exists
//!
//! Closes the §10.4 dogfood-rule canary suite at three different resource
//! families: CAD-graph ([`rge_cad_projection::CadProjectionPlugin`]) + GPU
//! ([`rge_gfx::GfxPlugin`]) + physics-world (this plugin). Three distinct
//! substrates exercising the same `PluginContext` design with no kernel-side
//! changes between them is the proof point ADR-114 calls for.
//!
//! The take/insert pattern this module repeats verbatim across all 5 canaries
//! (cad-projection / gfx / physics / audio / editor-ui) is intentional per
//! PLAN §10.4 dogfood rule — see [`rge_cad_projection::plugin_adapter`]'s
//! `# Why this looks duplicated across the five canaries` section for the
//! canonical rationale.
//!
//! # Resource contract
//!
//! On `tick`, the plugin context MUST contain (caller-supplied):
//!
//! * [`World`] — owned `&mut` after `take`; mutated by the rapier3d
//!   simulation step (broadphase rebuild, contact resolution, integration,
//!   `world.tick` increment).
//! * [`PhysicsInputLedger`] — owned `&mut` after `take`; appended to (one
//!   [`TickRecord`](crate::physics_input_ledger::TickRecord) per step) so
//!   replay can reproduce the trajectory per PLAN.md §1.6.8 Replay-Stable
//!   v1.0.
//!
//! Missing either resource surfaces as
//! [`PluginError::ContractViolation`] (caller-supplied resource missing —
//! NOT a plugin-side bug; auto-emit downgrades to a warning per audit-2
//! A5.1). Per the cad-projection / gfx precedent, in every error path the
//! resources that WERE supplied are put back into the context before the
//! error propagates (idempotent failure semantics).
//!
//! Tick is infallible at the plugin-adapter level — [`physics_step`] does
//! not return a `Result`, so there is no [`PluginError::RuntimeFault`]
//! surface here. The variant is reserved for future extensions of the
//! physics plugin (e.g. a fallible joint-build path or a rapier3d API
//! upgrade that exposes step errors). This is the canonical
//! "no-RuntimeFault straight-line subcase" formalised in ADR-114
//! §"Amendment 2026-05-08 — Three-substrate validation" §"No-RuntimeFault
//! subcase" and re-cited in §"Amendment 2026-05-08 — Four-substrate
//! validation" §"Pattern A + fallible inner work — first cross-canary
//! intersection" (where audio is the first canary that DOES exercise
//! Pattern A + fallible inner work end-to-end).
//!
//! # Send + 'static bound
//!
//! Per the kernel substrate's `Box<dyn Any + Send>` registry: every resource
//! a plugin extracts MUST be `Send + 'static`. rapier3d 0.32's
//! `RigidBodySet` / `ColliderSet` / `IslandManager` / `DefaultBroadPhase` /
//! `NarrowPhase` / `ImpulseJointSet` / `MultibodyJointSet` / `CCDSolver` /
//! `IntegrationParameters` / `PhysicsPipeline` are all `Send` (the
//! `enhanced-determinism` feature does not introduce any `!Send` types), so
//! [`World`] and [`PhysicsInputLedger`] satisfy the bound without further wrapping.
//! This is the third-substrate confirmation for ADR-114: the design
//! generalizes cleanly to physics-world resources without forcing a `Mutex`
//! wrapper or a non-Send compromise — matching the gfx canary's data point
//! for GPU resources.

use rge_kernel_plugin_host::{CanaryPlugin, Plugin, PluginContext, PluginError, PluginId};

use crate::physics_input_ledger::PhysicsInputLedger;
use crate::step::physics_step;
use crate::world::World;

/// Stable [`PluginId`] reported by every [`PhysicsPlugin`] instance.
pub const PHYSICS_PLUGIN_ID: &str = "rge-physics.fixed-step-plugin";

/// Tier-2 plugin adapter that advances the rapier3d simulation by exactly
/// one fixed timestep per `tick` against a caller-supplied [`World`] +
/// [`PhysicsInputLedger`].
///
/// Exposes the canary's tick lifecycle through the unified [`Plugin`] trait
/// per PLAN §10.4 dogfood rule. The adapter is a thin shim: real solver
/// work is delegated to [`physics_step`]. The adapter's job is to
/// (1) extract resources from the [`PluginContext`], (2) drive the step,
/// and (3) put the resources back so the orchestrator can retrieve them.
///
/// Mirrors the cad-projection + gfx canary pattern: zero state besides the
/// per-tick liveness counter; no GPU / wgpu surface; no lazy resource (the
/// physics solver state lives entirely inside the caller-supplied
/// [`World`], unlike gfx's `Option<TrianglePipeline>` which needs a
/// `GfxContext` to construct).
#[derive(Debug)]
pub struct PhysicsPlugin {
    /// Counts the number of successful physics steps the plugin has driven.
    /// Useful for tests and as a basic liveness signal for the orchestrator.
    /// Increments only on the success path; failed ticks (contract
    /// violation) leave the counter unchanged — matching the gfx canary's
    /// `frames_recorded` precedent.
    steps_run: u64,
}

impl PhysicsPlugin {
    /// Build a fresh plugin with zero recorded steps.
    #[must_use]
    pub fn new() -> Self {
        Self { steps_run: 0 }
    }

    /// Number of physics steps successfully driven across all completed
    /// ticks. Increments only on the success path; failed ticks (contract
    /// violation) leave the counter unchanged.
    #[must_use]
    pub fn steps_run(&self) -> u64 {
        self.steps_run
    }
}

impl Default for PhysicsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for PhysicsPlugin {
    fn id(&self) -> PluginId {
        PluginId::new(PHYSICS_PLUGIN_ID)
    }

    fn name(&self) -> &'static str {
        "rge-physics fixed-step canary"
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Construction already produced the zero-state counter; physics has
        // no GPU / pipeline / lazy-init machinery. Mirrors the cad-projection
        // + gfx precedent of an init that does no real work — the World +
        // PhysicsInputLedger are caller-staged, not plugin-built.
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Sequential takes — each `take` releases the borrow on `ctx`
        // immediately so the next `take` / `insert` is unhindered. If a
        // required resource is missing, restore whatever we already took
        // before erroring (idempotent failure semantics — the cad-projection
        // / gfx precedent).
        //
        // Missing-resource cases are CONTRACT violations (caller didn't
        // stage prerequisites) — distinct from RUNTIME faults coming out
        // of the solver itself. The host's auto-emit downgrades
        // ContractViolation to a warning per audit-2 A5.1.
        let mut world = ctx
            .take::<World>()
            .ok_or_else(|| PluginError::contract_violation("World"))?;
        let Some(mut ledger) = ctx.take::<PhysicsInputLedger>() else {
            // Put World back before erroring out so the orchestrator can
            // recover its handle — same shape as gfx's HeadlessTarget
            // missing-after-GfxContext-supplied path.
            let replaced = ctx.insert(world);
            debug_assert!(replaced.is_none(), "World slot was empty after take");
            return Err(PluginError::contract_violation("PhysicsInputLedger"));
        };

        // physics_step is infallible (returns ()): rapier3d 0.32's
        // PhysicsPipeline::step doesn't surface a Result, and the audit
        // ledger's begin_tick is also infallible. So there's no
        // RuntimeFault surface to map here today — see module-doc note.
        physics_step(&mut world, &mut ledger);

        // Always put resources back, even if a future fallible variant
        // surfaces, so the orchestrator can retrieve them. Slots are empty
        // after the takes above, so insert returns None — no resource is
        // dropped on the floor.
        debug_assert!(
            ctx.insert(world).is_none(),
            "World slot was empty after tick"
        );
        debug_assert!(
            ctx.insert(ledger).is_none(),
            "PhysicsInputLedger slot was empty after tick"
        );

        self.steps_run += 1;
        Ok(())
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No external resources held; the World + PhysicsInputLedger are
        // caller-owned and remain in the registry for the orchestrator to
        // retrieve. Mirrors the cad-projection + gfx precedent of a default
        // Ok(()) shutdown.
        Ok(())
    }
}

/// ADR-116 §10.4 dogfood-rule canary protocol impl. Delegates to the
/// inherent `steps_run` accessor; backwards-compat per ADR-116
/// Sub-decision 2.
impl CanaryPlugin for PhysicsPlugin {
    fn successful_ticks(&self) -> u64 {
        self.steps_run()
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;

    #[test]
    fn physics_plugin_id_matches_convention() {
        let plugin = PhysicsPlugin::new();
        assert_eq!(plugin.id(), PluginId::new("rge-physics.fixed-step-plugin"));
        assert_eq!(plugin.id().as_str(), PHYSICS_PLUGIN_ID);
    }

    #[test]
    fn physics_plugin_name_is_stable_human_readable_string() {
        let plugin = PhysicsPlugin::new();
        assert_eq!(plugin.name(), "rge-physics fixed-step canary");
    }

    #[test]
    fn physics_plugin_steps_run_starts_at_zero() {
        let plugin = PhysicsPlugin::new();
        assert_eq!(plugin.steps_run(), 0);
    }

    #[test]
    fn physics_plugin_default_impl_matches_new() {
        let from_default: PhysicsPlugin = PhysicsPlugin::default();
        let from_new = PhysicsPlugin::new();
        assert_eq!(from_default.steps_run(), from_new.steps_run());
    }

    #[test]
    fn physics_plugin_init_succeeds_without_resources() {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        // No resources inserted; init should still succeed (it's a no-op —
        // the cad-projection + gfx precedent).
        assert!(plugin.init(&mut ctx).is_ok());
        // Init must not have inserted anything either.
        assert_eq!(ctx.resource_count(), 0);
        // And it must NOT have advanced any state — the counter is unchanged.
        assert_eq!(plugin.steps_run(), 0);
    }

    #[test]
    fn physics_plugin_tick_with_no_resources_returns_contract_violation_for_world() {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        let err = plugin.tick(&mut ctx).expect_err("tick must fail");
        match err {
            PluginError::ContractViolation { resource_type } => {
                assert_eq!(resource_type, "World");
            }
            other => panic!("expected ContractViolation for World; got {other:?}"),
        }
        // Counter unchanged on failure.
        assert_eq!(plugin.steps_run(), 0);
        // No resources were left behind in the registry.
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn physics_plugin_tick_with_world_only_returns_contract_violation_for_input_ledger() {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage World but NOT PhysicsInputLedger. Tick must put World back.
        assert!(ctx.insert(World::new()).is_none());
        assert!(ctx.contains::<World>());
        assert!(!ctx.contains::<PhysicsInputLedger>());

        let err = plugin.tick(&mut ctx).expect_err("tick must fail");
        match err {
            PluginError::ContractViolation { resource_type } => {
                assert_eq!(
                    resource_type, "PhysicsInputLedger",
                    "second-resource missing must surface as PhysicsInputLedger violation"
                );
            }
            other => panic!("expected ContractViolation for PhysicsInputLedger; got {other:?}"),
        }

        // Idempotent failure invariant: World (the one we DID supply) must
        // still be in the registry so the orchestrator can recover it.
        // Mirrors the gfx canary's HeadlessTarget-missing-after-GfxContext
        // precedent.
        assert!(
            ctx.contains::<World>(),
            "World must be put back after a partial-resource contract violation"
        );
        assert_eq!(ctx.resource_count(), 1);
        // Counter unchanged on failure.
        assert_eq!(plugin.steps_run(), 0);
    }

    #[test]
    fn physics_plugin_tick_advances_world_and_ledger_when_both_supplied() {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage both required resources.
        let world = World::new();
        assert_eq!(world.tick, 0, "fresh world starts at tick 0");
        assert!(ctx.insert(world).is_none());
        assert!(ctx.insert(PhysicsInputLedger::new()).is_none());
        assert_eq!(ctx.resource_count(), 2);

        // Tick once.
        plugin.tick(&mut ctx).expect("tick must succeed");
        assert_eq!(plugin.steps_run(), 1);

        // Recover the resources from the registry; verify world advanced.
        let world_back: World = ctx.take().expect("World still present");
        let ledger_back: PhysicsInputLedger = ctx.take().expect("PhysicsInputLedger still present");
        assert_eq!(world_back.tick, 1, "world tick must advance by exactly one");
        assert_eq!(
            ledger_back.len(),
            1,
            "ledger must have recorded exactly one tick record"
        );
    }

    #[test]
    fn physics_plugin_shutdown_succeeds_without_resources() {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        assert!(plugin.shutdown(&mut ctx).is_ok());
    }

    /// ADR-116 acceptance: `PhysicsPlugin` impls the `CanaryPlugin` protocol.
    /// Trait method delegates to the existing inherent `steps_run` accessor;
    /// calling through `&dyn CanaryPlugin` exercises the dynamic-dispatch
    /// path future cross-canary tooling will use.
    #[test]
    fn physics_plugin_impls_canary_protocol() {
        let plugin = PhysicsPlugin::new();
        let canary: &dyn CanaryPlugin = &plugin;
        assert_eq!(canary.successful_ticks(), 0);
        assert_eq!(canary.successful_ticks(), plugin.steps_run());
    }
}
