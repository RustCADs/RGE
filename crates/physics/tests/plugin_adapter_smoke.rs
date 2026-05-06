//! Phase-canary integration smoke tests for `physics::PhysicsPlugin`.
//!
//! `PhysicsPlugin` is the third real Tier-2 plugin canary (after
//! `cad-projection::CadProjectionPlugin` and `gfx::GfxPlugin`) per the §10.4
//! dogfood rule and the ADR-114 canary suite. These tests prove that the v1
//! `PluginContext` owned-resources-handoff design generalises to a third
//! resource family — physics-world state owning the rapier3d arenas
//! ([`World`], [`PhysicsInputLedger`]) — without forcing any change to the Tier-1
//! substrate.
//!
//! Scenarios:
//!
//! 1. **`physics_plugin_lifecycle_via_plugin_host`** — register, init, tick,
//!    shutdown end-to-end through `PluginHost`. Verifies the plugin advances
//!    the world's `tick` counter and appends a tick record to the ledger.
//!
//! 2. **`physics_plugin_tick_returns_contract_violation_when_world_missing`**
//!    — caller fails to stage `World`. Tick fails with
//!    `PluginError::ContractViolation { resource_type: "World" }`,
//!    plugin transitions to `Failed`, and the auto-emit produces a
//!    `Severity::Warning` (not `Error`) per audit-2 A5.1.
//!
//! 3. **`physics_plugin_tick_returns_contract_violation_when_audit_ledger_missing`**
//!    — caller stages `World` but forgets `PhysicsInputLedger`. Tick surfaces
//!    `ContractViolation { resource_type: "PhysicsInputLedger" }`. The `World` WAS
//!    supplied so it must be put back into the registry (idempotent failure
//!    semantics — matching the gfx canary's HeadlessTarget-missing path).
//!
//! 4. **`physics_plugin_puts_resources_back_after_successful_tick`** —
//!    invariant: after a successful tick, both resources are still present
//!    in `ctx`, so the orchestrator can retrieve them.
//!
//! 5. **`physics_plugin_multi_tick_determinism`** — tick the same plugin 10
//!    times in a row over a populated world. rapier 0.32 with the
//!    `enhanced-determinism` feature must produce a byte-identical
//!    `serialize_state` trajectory across runs (the W11 acceptance criterion
//!    for §1.6.8 Replay-Stable v1.0). The plugin canary inherits this
//!    guarantee because it goes through the same `physics_step` entry point.
//!
//! 6. **`physics_plugin_isolation_with_sibling_failure_fixture`** —
//!    multi-plugin isolation: a sibling test fixture deliberately panics
//!    during tick; the host's `catch_unwind` recovers, the sibling is
//!    marked `Failed`, and `PhysicsPlugin` ticks successfully alongside it.
//!
//! All tests are GPU-free — physics simulation is CPU-only and runs on every
//! CI configuration.

use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};
use rge_kernel_plugin_host::{
    Plugin, PluginContext, PluginError, PluginHost, PluginId, PluginState,
};
use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use rge_physics::{PhysicsPlugin, World, PHYSICS_PLUGIN_ID};

/// Shared helper: build a minimal but non-trivial scene (one dynamic cube
/// dropped onto a fixed plane) so the physics step has something to chew on.
/// Mirrors the `falling_cube.rs` smoke-test setup; we don't need it to settle
/// here — just to produce non-zero solver work each tick.
fn make_scene_world() -> World {
    let mut world = World::new();
    // Fixed ground plane at y=0.
    let _ground = world.insert_body(
        RigidBody {
            kind: BodyKind::Fixed,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Plane,
            ..Collider::default()
        }),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    // Dynamic cube at y=5 — guaranteed to fall and contact the plane.
    let _cube = world.insert_body(
        RigidBody {
            kind: BodyKind::Dynamic,
            mass: 1.0,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Cuboid {
                hx: 0.5,
                hy: 0.5,
                hz: 0.5,
            },
            ..Collider::default()
        }),
        [0.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    world
}

/// The `PhysicsPlugin` adapter drives the rapier3d simulation end-to-end
/// through the unified `Plugin` trait + `PluginHost` lifecycle. Verifies
/// that:
///
/// 1. The plugin registers under its canonical id.
/// 2. `init_all` advances the plugin from `Pending` -> `Initialized`.
/// 3. `tick_all` extracts `World` + `PhysicsInputLedger` from the context, advances
///    the simulation by one step, and reports a successful tick.
/// 4. The world's `tick` counter increments by exactly one and the ledger
///    records exactly one new tick entry — proof the solver work actually ran.
/// 5. `shutdown_all` LIFO-shuts the plugin down without error.
#[test]
fn physics_plugin_lifecycle_via_plugin_host() {
    let world = make_scene_world();
    let ledger = PhysicsInputLedger::new();

    let plugin = PhysicsPlugin::new();
    let plugin_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");
    assert_eq!(host.state(&plugin_id), Some(PluginState::Pending));

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Init: must succeed — physics has no GPU / lazy state.
    let init_report = host.init_all(&mut ctx);
    assert_eq!(init_report.initialized, vec![plugin_id.clone()]);
    assert!(
        init_report.failed.is_empty(),
        "init failed: {:?}",
        init_report.failed
    );
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Stage resources for the tick.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(ledger).is_none());
    assert_eq!(ctx.resource_count(), 2);

    // Tick.
    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(
        tick_report.ticked, 1,
        "ticked count: {:?}",
        tick_report.failed
    );
    assert!(
        tick_report.failed.is_empty(),
        "tick failed: {:?}",
        tick_report.failed
    );
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Take resources back from ctx — they MUST be present after a successful
    // tick (the plugin contract requires putting them back).
    let world_back: World = ctx.take().expect("World present after tick");
    let ledger_back: PhysicsInputLedger =
        ctx.take().expect("PhysicsInputLedger present after tick");
    assert_eq!(ctx.resource_count(), 0);

    // Verify the solver actually ran: world tick advanced, ledger has one
    // record at tick 0 (the tick we just stepped past).
    assert_eq!(
        world_back.tick, 1,
        "world tick must advance from 0 to exactly 1 after one step"
    );
    assert_eq!(
        ledger_back.len(),
        1,
        "ledger must have exactly one record after one step"
    );

    // Re-stage so shutdown_all has a clean ctx (no resource pressure either way).
    drop(world_back);
    drop(ledger_back);

    // Shutdown LIFO. No plugin-level error expected.
    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(shutdown_report.shutdown.len(), 1);
    assert!(shutdown_report.failed.is_empty());
    assert_eq!(host.count(), 0);
}

/// Runtime safety: a tick with `World` missing surfaces as
/// `PluginError::ContractViolation { resource_type: "World" }` and marks the
/// plugin Failed (per plugin-fatal isolation), without panicking. Per
/// audit-2 A5.1, the host's auto-emit classifies this as a Warning (not
/// Error) — the plugin code is fine; the caller failed to stage prerequisites.
#[test]
fn physics_plugin_tick_returns_contract_violation_when_world_missing() {
    let plugin = PhysicsPlugin::new();
    let plugin_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(init_report.failed.is_empty());
    }
    assert_eq!(diags.len(), 0, "init must not auto-emit on success");

    let tick_report = {
        let mut ctx = PluginContext::new(&mut diags);
        // Deliberately do NOT insert World (or PhysicsInputLedger). Tick must fail
        // cleanly at the first take.
        host.tick_all(&mut ctx)
    };
    assert_eq!(tick_report.ticked, 0);
    assert_eq!(
        tick_report.failed.len(),
        1,
        "missing World must surface as a failed tick"
    );
    let (failed_id, failed_msg) = &tick_report.failed[0];
    assert_eq!(*failed_id, plugin_id);
    assert!(
        failed_msg.contains("missing resource of type World"),
        "error message must mention missing-World contract violation; got: {failed_msg}"
    );
    // Per plugin-fatal isolation, the plugin is now Failed.
    assert_eq!(host.state(&plugin_id), Some(PluginState::Failed));

    // Audit-2 A5.1: ContractViolation auto-emits as Warning, not Error.
    let new_diags: Vec<_> = diags.iter().collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected one auto-emit diagnostic for the contract violation",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation must auto-emit as Warning (not Error) per audit-2 A5.1",
    );
}

/// Idempotent failure: when `PhysicsInputLedger` is missing but `World` was
/// supplied, the plugin must put `World` back into the registry before
/// returning the contract violation — the orchestrator should still be able
/// to recover the `World` handle to re-issue the call later.
///
/// This test exercises the plugin adapter directly (no `PluginHost` wrap)
/// because the put-back invariant is tested at the plugin level; the host's
/// resource-leak detection is independently exercised by `host.rs`'s own
/// unit tests. Mirrors the gfx canary's HeadlessTarget-missing-after-
/// GfxContext-supplied test.
#[test]
fn physics_plugin_tick_returns_contract_violation_when_input_ledger_missing() {
    let world = make_scene_world();

    let mut plugin = PhysicsPlugin::new();
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage World but NOT PhysicsInputLedger. Tick must put World back.
    assert!(ctx.insert(world).is_none());
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
    assert!(
        ctx.contains::<World>(),
        "World must be put back after a partial-resource contract violation"
    );
    assert_eq!(ctx.resource_count(), 1);
    // Counter unchanged on failure.
    assert_eq!(plugin.steps_run(), 0);
}

/// After a successful tick, both resources (`World` / `PhysicsInputLedger`) must
/// be back in the context — the plugin is responsible for returning them so
/// the orchestrator can retrieve them. Mirrors the cad-projection +
/// gfx `puts_resources_back` precedents.
#[test]
fn physics_plugin_puts_resources_back_after_successful_tick() {
    let world = make_scene_world();
    let ledger = PhysicsInputLedger::new();

    let plugin = PhysicsPlugin::new();
    let plugin_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    let _init_report = host.init_all(&mut ctx);

    // Stage resources.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(ledger).is_none());
    assert!(ctx.contains::<World>());
    assert!(ctx.contains::<PhysicsInputLedger>());
    assert_eq!(ctx.resource_count(), 2);

    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(tick_report.ticked, 1);
    assert!(tick_report.failed.is_empty());

    // The invariant: after a successful tick, every resource we staged is
    // still present.
    assert!(ctx.contains::<World>(), "World must be put back after tick");
    assert!(
        ctx.contains::<PhysicsInputLedger>(),
        "PhysicsInputLedger must be put back after tick"
    );
    assert_eq!(ctx.resource_count(), 2);
}

/// Determinism: ticking the same fresh scene N times via the plugin must
/// produce a byte-identical `serialize_state` trajectory across two runs.
/// rapier 0.32's `enhanced-determinism` feature is the load-bearing
/// guarantee here (see `world.rs` / `step.rs` module-docs); the plugin
/// canary inherits the same guarantee because it routes through
/// `physics_step` unchanged.
///
/// Verifies the W11 acceptance criterion for §1.6.8 Replay-Stable v1.0
/// composes through the plugin lifecycle.
#[test]
fn physics_plugin_multi_tick_determinism() {
    /// Drive `n` ticks of the plugin against a fresh scene and return the
    /// concatenated per-tick `serialise_state` bytes.
    fn run(n: usize) -> Vec<u8> {
        let mut plugin = PhysicsPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        plugin.init(&mut ctx).expect("init");
        ctx.insert(make_scene_world());
        ctx.insert(PhysicsInputLedger::new());

        let mut trajectory = Vec::new();
        for _ in 0..n {
            plugin.tick(&mut ctx).expect("tick must succeed");
            // Peek at the world without removing — get_mut keeps it in ctx.
            let world = ctx.get_mut::<World>().expect("world stays in ctx");
            trajectory.extend_from_slice(&world.serialize_state());
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "test loop bound n=10 fits usize on every supported target"
        )]
        let steps_run_usize = plugin.steps_run() as usize;
        assert_eq!(steps_run_usize, n);
        trajectory
    }

    let trajectory_a = run(10);
    let trajectory_b = run(10);
    assert_eq!(
        trajectory_a, trajectory_b,
        "two identical 10-tick runs through PhysicsPlugin must be byte-equal \
         (rapier3d enhanced-determinism + plugin canary composition)"
    );
    assert!(
        !trajectory_a.is_empty(),
        "trajectory must be non-empty (the scene has bodies)"
    );
}

/// Multi-plugin isolation: register `PhysicsPlugin` alongside a sibling
/// test fixture that deliberately panics during tick. Verify:
///
/// 1. The host's `catch_unwind` recovers from the sibling's panic.
/// 2. The sibling is marked `Failed` (plugin-fatal isolation per §1.13).
/// 3. `PhysicsPlugin` ticks successfully alongside the sibling — its state
///    and resource handoff are entirely unaffected by the sibling's failure.
/// 4. The diagnostic stream contains exactly one new error
///    (`PANICKED during tick`) attributable to the sibling, not to physics.
#[test]
fn physics_plugin_isolation_with_sibling_failure_fixture() {
    let world = make_scene_world();
    let ledger = PhysicsInputLedger::new();

    let physics_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let panicker_id = PluginId::new("test.panic-sibling");

    let mut host = PluginHost::new();
    host.register(physics_id.clone(), Box::new(PhysicsPlugin::new()))
        .expect("register physics");
    host.register(
        panicker_id.clone(),
        Box::new(PanickingTickPlugin::new(panicker_id.clone())),
    )
    .expect("register panicker");

    let mut diags = DiagnosticAggregator::new();

    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(
            init_report.failed.is_empty(),
            "init: {:?}",
            init_report.failed
        );
        assert_eq!(init_report.initialized.len(), 2);
    }

    let pre_tick_diag_count = diags.len();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage physics-only resources; the PanickingTickPlugin doesn't take any.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(ledger).is_none());

    let tick_report = host.tick_all(&mut ctx);

    assert_eq!(
        tick_report.ticked, 1,
        "exactly one plugin (physics) ticked Ok"
    );
    assert_eq!(
        tick_report.failed.len(),
        1,
        "exactly one plugin (sibling) failed"
    );
    assert_eq!(tick_report.failed[0].0, panicker_id);
    assert!(
        tick_report.failed[0].1.contains("panicked during tick"),
        "sibling failure must mention panic; got: {}",
        tick_report.failed[0].1
    );

    // PhysicsPlugin survived in spite of the sibling's panic — plugin-fatal
    // isolation per PLAN §1.13.
    assert_eq!(host.state(&physics_id), Some(PluginState::Initialized));
    assert_eq!(host.state(&panicker_id), Some(PluginState::Failed));

    // Resources put back successfully despite the sibling's failure.
    assert!(ctx.contains::<World>());
    assert!(ctx.contains::<PhysicsInputLedger>());

    // The world tick still advanced — physics did its job.
    let world_ref = ctx.get_mut::<World>().expect("world present");
    assert_eq!(
        world_ref.tick, 1,
        "physics must have ticked despite sibling panic"
    );

    // Exactly one new diagnostic — the PANICKED one for the sibling.
    let new_messages: Vec<&str> = diags
        .iter()
        .skip(pre_tick_diag_count)
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        new_messages
            .iter()
            .any(|m| m.contains("PANICKED during tick") && m.contains("test.panic-sibling")),
        "expected PANICKED-during-tick diagnostic for sibling; got {new_messages:?}",
    );
    // Physics must NOT have produced any failure diagnostic.
    assert!(
        !new_messages.iter().any(|m| m.contains(PHYSICS_PLUGIN_ID)
            && (m.contains("PANICKED") || m.contains("violation"))),
        "physics must not have produced failure diagnostics; got {new_messages:?}",
    );
}

// ---------------------------------------------------------------------------
// Test fixture: a plugin whose tick deliberately panics, used to drive the
// host's catch_unwind recovery path while physics ticks normally alongside it.
// Mirrors the gfx canary's `PanickingTickPlugin` fixture verbatim — kept
// local to this test file so it doesn't need privileged access to
// kernel-level test helpers.
// ---------------------------------------------------------------------------

/// Minimal `Plugin` impl that panics on every `tick`. Test-only sibling
/// fixture for the isolation test above.
struct PanickingTickPlugin {
    id: PluginId,
}

impl PanickingTickPlugin {
    fn new(id: PluginId) -> Self {
        Self { id }
    }
}

impl Plugin for PanickingTickPlugin {
    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Deliberate panic to drive the host's catch_unwind recovery.
        panic!("PanickingTickPlugin: deliberate tick panic for isolation test");
    }
}
