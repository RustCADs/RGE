//! Phase-canary integration smoke tests for `cad-projection::CadProjectionPlugin`.
//!
//! `CadProjectionPlugin` is the first real Tier-2 plugin canary per the
//! §10.4 dogfood rule (closes Pairing-3 of the 2026-05-07 deep audit /
//! post-audit CRITICAL #2). These tests prove that the v1 `PluginContext`
//! owned-resources-handoff design composes for the cad-projection substrate
//! (`World` + `CadGraph` + `Tolerance`) without forcing any change to the
//! Tier-1 substrate.
//!
//! Mirrors the structure of `crates/{audio,gfx,physics}/tests/plugin_adapter_smoke.rs`.
//!
//! Scenarios:
//!
//! 1. **`cad_projection_plugin_lifecycle_via_plugin_host`** — Pairing-3
//!    closure. Wraps `CadProjection` in `CadProjectionPlugin`, registers via
//!    `PluginHost`, drives init+tick through the unified plugin lifecycle,
//!    and verifies the projection actually ran end-to-end (`BRepHandle`'s
//!    `mesh_id` updated post-tick). First real Tier-2 plugin canary —
//!    proves the v1 `PluginContext` capability registry composes.
//!
//! 2. **`cad_projection_plugin_tick_returns_error_when_world_missing`** —
//!    runtime safety: missing required resources surface as `PluginError`
//!    + plugin state Failed (not panic). Per audit-2 A5.1, the host's
//!    auto-emit classifies `ContractViolation` as a Warning (not Error).
//!
//! 3. **`cad_projection_plugin_tick_puts_resources_back`** — invariant:
//!    after a successful tick, all three resources (`World` / `CadGraph` /
//!    `Tolerance`) are still in the context, so the orchestrator can
//!    retrieve them.
//!
//! 4. **`cad_projection_plugin_isolation_with_sibling_panic`** —
//!    multi-plugin isolation: a sibling test fixture deliberately panics
//!    during tick; the host's `catch_unwind` recovers, the sibling is
//!    marked `Failed`, and `CadProjectionPlugin` ticks successfully
//!    alongside it. Mirrors `gfx::gfx_plugin_isolation_with_sibling_failure_fixture`.

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{
    BRepHandle, CadProjection, CadProjectionPlugin, CAD_PROJECTION_PLUGIN_ID,
};
use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};
use rge_kernel_ecs::World;
use rge_kernel_graph_foundation::NodeId;
use rge_kernel_plugin_host::{
    Plugin, PluginContext, PluginError, PluginHost, PluginId, PluginState,
};

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Helper: install Cuboid(`w`,`h`,`d`) as the only node + root in `cad`,
/// committed under the given label. Returns the new node id.
fn add_cuboid(cad: &mut CadGraph, w: f32, h: f32, d: f32, label: &str) -> NodeId {
    cad.begin_operation().expect("begin");
    let node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: w,
            height: h,
            depth: d,
        }))
        .expect("add");
    cad.graph_mut().expect("mut2").set_root(node).expect("root");
    cad.commit(label).expect("commit");
    node
}

// ===========================================================================
// CadProjectionPlugin canary — first real Tier-2 plugin via the §10.4 dogfood
// rule. Closes Pairing-3 of the 2026-05-07 deep audit (post-audit CRITICAL #2).
// ===========================================================================

/// Pairing-3 closure: the `CadProjectionPlugin` adapter drives a real
/// Tier-2 subsystem (cad-projection) end-to-end through the unified
/// `Plugin` trait + `PluginHost` lifecycle. Verifies that:
///
/// 1. The plugin registers successfully under its canonical id.
/// 2. `init_all` advances the plugin from `Pending` → `Initialized`.
/// 3. `tick_all` extracts World+CadGraph+Tolerance from the context, drives
///    the projection, and reports a successful tick.
/// 4. Post-tick, the `BRepHandle` component in `World` has its `mesh_id`
///    field populated — proof that the projection actually ran.
/// 5. `shutdown_all` LIFO-shuts the plugin down without error.
#[test]
fn cad_projection_plugin_lifecycle_via_plugin_host() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "cuboid for plugin smoke");

    // Build a projection, spawn the BRepHandle entity, then wrap in plugin.
    let mut projection = CadProjection::new();
    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let plugin = CadProjectionPlugin::from_projection(projection);
    // Sanity: the wrapped projection's mapping persisted.
    assert_eq!(plugin.projection().node_for(entity), Some(node));

    // Build the host + register the plugin.
    let plugin_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");
    assert_eq!(host.state(&plugin_id), Some(PluginState::Pending));

    // Build the context. The diagnostic aggregator outlives the context.
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Init.
    let init_report = host.init_all(&mut ctx);
    assert_eq!(init_report.initialized, vec![plugin_id.clone()]);
    assert!(
        init_report.failed.is_empty(),
        "init failed: {:?}",
        init_report.failed
    );
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Insert resources for the tick. The orchestrator pattern: take owned
    // resources from somewhere, hand them to ctx, drive ticks, take them
    // back when done.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(cad).is_none());
    let _ = ctx.insert(tol());
    assert_eq!(ctx.resource_count(), 3);

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
    // Plugin state stays Initialized after a successful tick.
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Take resources back from ctx — they MUST be present after a successful
    // tick (the plugin contract requires putting them back).
    let world_back: World = ctx.take().expect("World present after tick");
    let _cad_back: CadGraph = ctx.take().expect("CadGraph present after tick");
    let _tolerance_back: Tolerance = ctx.take().expect("Tolerance present after tick");
    assert_eq!(ctx.resource_count(), 0);

    // Verify the projection actually ran: BRepHandle's mesh_id must be set.
    let er = world_back.entity(entity).expect("entity preserved");
    let handle = er.get::<BRepHandle>().expect("brep handle present");
    assert!(
        handle.mesh_id.is_some(),
        "BRepHandle.mesh_id must be Some after a successful tick"
    );
    assert!(
        handle.last_projected_checkpoint.is_some(),
        "BRepHandle.last_projected_checkpoint must be Some after a successful tick"
    );

    // Shutdown LIFO. No plugin-level error expected.
    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(shutdown_report.shutdown.len(), 1);
    assert!(shutdown_report.failed.is_empty());
    assert_eq!(host.count(), 0);
}

/// Runtime safety: a tick with the World resource missing surfaces as
/// `PluginError::ContractViolation { resource_type: "World" }` and marks the
/// plugin Failed (per plugin-fatal isolation), without panicking. Per
/// audit-2 A5.1, the host's auto-emit classifies this as a Warning (not
/// Error) — the plugin code is fine; the caller failed to stage the
/// prerequisites.
#[test]
fn cad_projection_plugin_tick_returns_error_when_world_missing() {
    let plugin = CadProjectionPlugin::new();
    let plugin_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(init_report.failed.is_empty());
    }
    // Init produced no diagnostics (TestPlugin/CadProjectionPlugin emit
    // none on success); the only diagnostic that follows comes from the
    // tick failure.
    assert_eq!(diags.len(), 0, "init must not auto-emit on success");

    let tick_report = {
        let mut ctx = PluginContext::new(&mut diags);

        // Deliberately do NOT insert World. Tick must fail cleanly.
        let mut cad = CadGraph::new();
        let _node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "missing-World canary");
        assert!(ctx.insert(cad).is_none());
        let _ = ctx.insert(tol());
        // Note: World absent.
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
    // Display impl for ContractViolation includes the resource type name —
    // "missing resource of type World" — so the historical "missing World"
    // substring assertion still holds.
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

/// After a successful tick, all three resources (World/CadGraph/Tolerance)
/// must be back in the context — the plugin is responsible for returning
/// them so the orchestrator can retrieve them.
#[test]
fn cad_projection_plugin_tick_puts_resources_back() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "resources-back canary");

    let mut projection = CadProjection::new();
    let _entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let plugin = CadProjectionPlugin::from_projection(projection);

    let plugin_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    let _init_report = host.init_all(&mut ctx);

    // Stage resources.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(cad).is_none());
    let _ = ctx.insert(tol());
    assert!(ctx.contains::<World>());
    assert!(ctx.contains::<CadGraph>());
    assert!(ctx.contains::<Tolerance>());
    assert_eq!(ctx.resource_count(), 3);

    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(tick_report.ticked, 1);
    assert!(tick_report.failed.is_empty());

    // The invariant: after a successful tick, every resource we staged is
    // still present.
    assert!(ctx.contains::<World>(), "World must be put back after tick");
    assert!(
        ctx.contains::<CadGraph>(),
        "CadGraph must be put back after tick"
    );
    assert!(
        ctx.contains::<Tolerance>(),
        "Tolerance must be put back after tick"
    );
    assert_eq!(ctx.resource_count(), 3);
}

// ===========================================================================
// Multi-plugin isolation canary — closes audit-2 gap: 3 of 4 Tier-2 canaries
// (gfx / physics / audio) have a sibling-panic isolation fixture; this test
// brings cad-projection to parity per the §10.4 dogfood rule. Mirrors
// `crates/gfx/tests/plugin_adapter_smoke.rs::gfx_plugin_isolation_with_sibling_failure_fixture`.
// ===========================================================================

/// Multi-plugin isolation: register `CadProjectionPlugin` alongside a sibling
/// test fixture that deliberately panics during `tick`. Verify:
///
/// 1. The host's `catch_unwind` recovers from the sibling's panic.
/// 2. The sibling is marked `Failed` (plugin-fatal isolation per PLAN §1.13).
/// 3. `CadProjectionPlugin` ticks successfully alongside the sibling — its
///    state, resources, and projection output are entirely unaffected by the
///    sibling's failure.
/// 4. The diagnostic stream contains an Error-severity diagnostic mentioning
///    the panic (attributable to the sibling, not to cad-projection).
/// 5. Resources staged for cad-projection (`World` / `CadGraph` / `Tolerance`)
///    are still in the context post-tick — the put-back invariant held
///    despite the sibling panic.
#[test]
fn cad_projection_plugin_isolation_with_sibling_panic() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "cuboid for sibling-panic canary");

    let mut projection = CadProjection::new();
    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let plugin = CadProjectionPlugin::from_projection(projection);

    let proj_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let panicker_id = PluginId::new("test.panic-sibling");

    let mut host = PluginHost::new();
    host.register(proj_id.clone(), Box::new(plugin))
        .expect("register cad-projection plugin");
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

    // Stage cad-projection resources; the PanickingTickPlugin doesn't take
    // any, so it panics on entry.
    assert!(ctx.insert(world).is_none());
    assert!(ctx.insert(cad).is_none());
    let _ = ctx.insert(tol());
    assert_eq!(ctx.resource_count(), 3);

    let tick_report = host.tick_all(&mut ctx);

    assert_eq!(
        tick_report.ticked, 1,
        "exactly one plugin (cad-projection) ticked Ok"
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

    // CadProjectionPlugin survived in spite of the sibling's panic — plugin-
    // fatal isolation per PLAN §1.13.
    assert_eq!(host.state(&proj_id), Some(PluginState::Initialized));
    assert_eq!(host.state(&panicker_id), Some(PluginState::Failed));

    // Put-back invariant held despite sibling panic: all three resources
    // staged for cad-projection are still present.
    assert!(
        ctx.contains::<World>(),
        "World must be put back after tick (sibling panic must not disturb)"
    );
    assert!(
        ctx.contains::<CadGraph>(),
        "CadGraph must be put back after tick"
    );
    assert!(
        ctx.contains::<Tolerance>(),
        "Tolerance must be put back after tick"
    );

    // Verify the projection actually ran on its own resources: take World
    // back and inspect the BRepHandle's mesh_id was populated.
    let world_back: World = ctx.take().expect("World present after tick");
    let er = world_back.entity(entity).expect("entity preserved");
    let handle = er.get::<BRepHandle>().expect("brep handle present");
    assert!(
        handle.mesh_id.is_some(),
        "BRepHandle.mesh_id must be Some — projection ran successfully despite sibling panic"
    );

    // Drop ctx so the diagnostic borrow ends, then inspect diagnostics.
    drop(ctx);

    // Exactly one new diagnostic — the PANICKED-during-tick one for the
    // sibling. Severity::Error per the plugin-panic auto-emit semantics.
    let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
    assert!(
        new_diags.iter().any(|d| d.severity == Severity::Error
            && d.message.contains("PANICKED during tick")
            && d.message.contains("test.panic-sibling")),
        "expected Error-severity PANICKED-during-tick diagnostic for sibling; got {:?}",
        new_diags
            .iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>()
    );
    // CadProjectionPlugin must NOT have produced any failure diagnostic.
    assert!(
        !new_diags
            .iter()
            .any(|d| d.message.contains(CAD_PROJECTION_PLUGIN_ID)
                && (d.message.contains("PANICKED")
                    || d.message.contains("violation")
                    || d.message.contains("failed"))),
        "cad-projection must not have produced failure diagnostics; got {:?}",
        new_diags
            .iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Test fixture: a plugin whose tick deliberately panics, used to drive the
// host's catch_unwind recovery path while cad-projection ticks normally
// alongside it. Mirrors the gfx / physics / audio canary fixtures verbatim.
// ---------------------------------------------------------------------------

/// Minimal `Plugin` impl that panics on every `tick`. Test-only sibling
/// fixture for the isolation test above. Mirrors the spirit of
/// `host.rs::TestPlugin::with_tick_panic` but lives outside the kernel
/// crate so this test file doesn't need privileged access.
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
