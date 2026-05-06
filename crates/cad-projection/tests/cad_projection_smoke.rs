//! Phase 7.3 integration smoke tests for `cad-projection`.
//!
//! Scenarios:
//!
//! 1. **`invalidation_within_one_tick`** — exit criterion from PLAN.md /
//!    HANDOFF.md: "cad-projection invalidation triggers ECS update within one
//!    tick of cad-core commit". Build a Cuboid(1,1,1), commit C1, project,
//!    rebuild as Cuboid(2,2,2), commit C2, re-project. Verify the head
//!    advanced and the new vertex positions reflect the bigger cuboid.
//!
//! 2. **`pie_round_trip`** — capture a `PieSnapshot` carrying the projection's
//!    `EntityCadMap`, replace the projection with a fresh instance, restore,
//!    and re-tick. Verify the entity's `BRepHandle` still points at the right
//!    cad node and its mesh re-projects correctly.
//!
//! 3. **`pie_full_round_trip_with_cadgraph_participant`** — exercises the
//!    full Pairing-4 round-trip (PLAN §13.2 closure): both `&cad` AND
//!    `&projection` are captured as participants, restore round-trips both,
//!    `validate_handles` returns empty Vec (no orphans), re-tick reproduces
//!    the original mesh. This is the path the orchestrator should follow.
//!
//! 4. **`validate_handles_detects_orphan_after_partial_restore`** — exercises
//!    the divergent-state recovery path: capture both participants but only
//!    restore the projection (skip cad-graph). `validate_handles` reports the
//!    orphan; `tick` returns `ProjectionError::NodeNotInGraph` rather than
//!    panicking. Demonstrates the safety net for callers who fail to honor
//!    the co-restore convention.
//!
//! 5. **`cad_projection_plugin_lifecycle_via_plugin_host`** — Pairing-3 closure
//!    (post-2026-05-07 audit CRITICAL #2): wraps `CadProjection` in
//!    `CadProjectionPlugin`, registers via `PluginHost`, drives init+tick
//!    through the unified plugin lifecycle, and verifies the projection
//!    actually ran end-to-end (`BRepHandle`'s `mesh_id` updated post-tick).
//!    First real Tier-2 plugin canary — proves the v1 `PluginContext`
//!    capability registry composes.
//!
//! 6. **`cad_projection_plugin_tick_returns_error_when_world_missing`** —
//!    runtime safety: missing required resources surface as `PluginError` +
//!    plugin state Failed (not panic).
//!
//! 7. **`cad_projection_plugin_tick_puts_resources_back`** — invariant: after
//!    a successful tick, all three resources (`World` / `CadGraph` /
//!    `Tolerance`) are still in the context, so the orchestrator can
//!    retrieve them.

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{
    BRepHandle, CadProjection, CadProjectionPlugin, ProjectedMesh, ProjectionError,
    CAD_PROJECTION_PLUGIN_ID,
};
use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::{ParticipantId, PieSnapshot, SnapshotParticipate, World};
use rge_kernel_graph_foundation::NodeId;
use rge_kernel_plugin_host::{PluginContext, PluginHost, PluginId, PluginState};

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

/// Compute `(min, max)` over each x/y/z axis across `mesh.positions`.
fn bbox(mesh: &ProjectedMesh) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for [x, y, z] in &mesh.positions {
        if *x < min[0] {
            min[0] = *x;
        }
        if *y < min[1] {
            min[1] = *y;
        }
        if *z < min[2] {
            min[2] = *z;
        }
        if *x > max[0] {
            max[0] = *x;
        }
        if *y > max[1] {
            max[1] = *y;
        }
        if *z > max[2] {
            max[2] = *z;
        }
    }
    (min, max)
}

/// Phase 7.3 exit criterion: invalidation triggers re-projection within one
/// tick of `cad-core` commit.
#[test]
fn invalidation_within_one_tick() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    // Build & commit Cuboid(1,1,1) as C1.
    let mut cad = CadGraph::new();
    let node_c1 = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "C1: cuboid(1,1,1)");
    let head_c1 = cad.head();

    // Spawn the BRepHandle entity for the C1 cuboid; tick & verify mesh.
    let entity = projection
        .spawn_brep_entity(&mut world, node_c1)
        .expect("spawn");
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);
    assert_eq!(r1.head_advanced_to, head_c1);

    let mesh1 = projection.projected_mesh(entity).expect("mesh1").clone();
    assert_eq!(mesh1.vertex_count(), 8);
    let (min1, max1) = bbox(&mesh1);
    // Cuboid(1,1,1) → corners at ±0.5.
    for i in 0..3 {
        assert!(
            (min1[i] - (-0.5)).abs() < 1e-5,
            "axis {i} min should be -0.5, got {}",
            min1[i]
        );
        assert!(
            (max1[i] - 0.5).abs() < 1e-5,
            "axis {i} max should be +0.5, got {}",
            max1[i]
        );
    }

    // Replace the cuboid with Cuboid(2,2,2) under a new operation. Because
    // operator-node ids are content-addressed, this is a different NodeId
    // than node_c1 — we re-map our entity to the new node before ticking.
    let node_c2 = add_cuboid(&mut cad, 2.0, 2.0, 2.0, "C2: cuboid(2,2,2)");
    let head_c2 = cad.head();
    assert_ne!(head_c1, head_c2, "commit must advance head");

    // Update the entity's BRepHandle.cad_node + remap entity_cad_map.
    {
        let mut em = world.entity_mut(entity).expect("entity");
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.cad_node = node_c2;
        };
    }
    // Despawn + respawn through the projection facade is the supported
    // way to remap; alternatively we can clear the cache for this entity
    // and use the projection's tick to pick up the new node. We'll use a
    // direct-clean-and-respawn pattern for the smoke test.
    projection.despawn_brep_entity(&mut world, entity);
    let entity2 = projection
        .spawn_brep_entity(&mut world, node_c2)
        .expect("respawn");

    // Tick — head advanced, every entity dirty, re-project.
    let r2 = projection.tick(&mut world, &cad, tol()).expect("tick2");
    assert_eq!(r2.entities_reprojected, 1);
    assert_eq!(r2.head_advanced_to, head_c2);

    let mesh2 = projection.projected_mesh(entity2).expect("mesh2");
    assert_eq!(mesh2.vertex_count(), 8);
    let (min2, max2) = bbox(mesh2);
    // Cuboid(2,2,2) → corners at ±1.0.
    for i in 0..3 {
        assert!(
            (min2[i] - (-1.0)).abs() < 1e-5,
            "axis {i} min should be -1.0 after re-projection, got {}",
            min2[i]
        );
        assert!(
            (max2[i] - 1.0).abs() < 1e-5,
            "axis {i} max should be +1.0 after re-projection, got {}",
            max2[i]
        );
    }
}

/// PIE round-trip: capture a snapshot containing the projection state,
/// replace the projection, restore, tick. The entity's `BRepHandle` survives
/// (round-trips through ECS world snapshot) and its cad node is preserved.
/// The mesh re-projects correctly post-restore.
#[test]
fn pie_round_trip() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.5, 1.5, 1.5, "cuboid for snapshot");

    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let _ = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert!(projection.projected_mesh(entity).is_some());

    // Capture the PIE envelope (world bytes + projection participant payload).
    let snap =
        PieSnapshot::capture(&world, &[&projection as &dyn SnapshotParticipate]).expect("capture");
    assert_eq!(snap.participants.len(), 1, "one participant");
    let pid = ParticipantId::new("cad-projection.brep-handles");
    assert!(snap.participants.contains_key(&pid));

    // Replace world + projection with fresh instances. The receiving world
    // must register the same SnapshotComponent types so restore can decode
    // them.
    let mut world2 = World::new();
    world2.register_snapshot_component::<BRepHandle>();
    let mut projection2 = CadProjection::new();

    snap.restore(
        &mut world2,
        &mut [(&pid, &mut projection2 as &mut dyn SnapshotParticipate)],
    )
    .expect("restore");

    // The world has the original entity + its BRepHandle, with cad_node
    // pointing at the original node.
    let er = world2.entity(entity).expect("entity preserved");
    let handle = er.get::<BRepHandle>().expect("brep handle preserved");
    assert_eq!(handle.cad_node, node);

    // The projection's EntityCadMap survived.
    assert_eq!(projection2.node_for(entity), Some(node));
    assert_eq!(projection2.entity_for(node), Some(entity));

    // Tick re-projects; the new mesh is the cuboid we stored before snapshot.
    let r = projection2.tick(&mut world2, &cad, tol()).expect("tick2");
    assert_eq!(r.entities_reprojected, 1);
    let mesh = projection2
        .projected_mesh(entity)
        .expect("mesh reprojected");
    assert_eq!(mesh.vertex_count(), 8);
    assert_eq!(mesh.triangle_count(), 12);
    let (min, max) = bbox(mesh);
    // Cuboid(1.5, 1.5, 1.5) → corners at ±0.75.
    for i in 0..3 {
        assert!(
            (min[i] - (-0.75)).abs() < 1e-5,
            "axis {i} min should be -0.75 post-restore, got {}",
            min[i]
        );
        assert!(
            (max[i] - 0.75).abs() < 1e-5,
            "axis {i} max should be +0.75 post-restore, got {}",
            max[i]
        );
    }
}

/// Pairing-4 closure: full PIE round-trip with BOTH `&cad` and `&projection`
/// as participants. This is the convention the orchestrator should follow —
/// post-restore `validate_handles` returns no orphans, and the projection
/// re-projects matching the original mesh.
#[test]
fn pie_full_round_trip_with_cadgraph_participant() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.5, 1.5, 1.5, "cuboid for full round-trip");

    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let _ = projection.tick(&mut world, &cad, tol()).expect("tick1");
    let mesh_before = projection.projected_mesh(entity).expect("mesh1").clone();
    let (min_before, max_before) = bbox(&mesh_before);

    // Capture PIE with BOTH cad-graph and projection participants.
    let snap = PieSnapshot::capture(
        &world,
        &[
            &cad as &dyn SnapshotParticipate,
            &projection as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture");
    assert_eq!(snap.participants.len(), 2, "two participants");
    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");
    assert!(snap.participants.contains_key(&pid_cad));
    assert!(snap.participants.contains_key(&pid_proj));

    // Restore into a fresh world + cad-graph + projection.
    let mut fresh_world = World::new();
    fresh_world.register_snapshot_component::<BRepHandle>();
    let mut fresh_cad = CadGraph::new();
    let mut fresh_projection = CadProjection::new();
    snap.restore(
        &mut fresh_world,
        &mut [
            (&pid_cad, &mut fresh_cad as &mut dyn SnapshotParticipate),
            (
                &pid_proj,
                &mut fresh_projection as &mut dyn SnapshotParticipate,
            ),
        ],
    )
    .expect("restore");

    // Cad-graph state recovered.
    assert_eq!(fresh_cad.head(), cad.head(), "cad head preserved");
    assert_eq!(fresh_cad.graph().node_count(), cad.graph().node_count());
    assert!(
        fresh_cad.graph().node(node).is_some(),
        "cuboid node present"
    );

    // Pairing-4 invariant: validate_handles returns NO orphans because
    // cad-graph and projection were captured + restored coherently.
    let orphans = fresh_projection.validate_handles(&fresh_cad);
    assert!(
        orphans.is_empty(),
        "cad+projection co-restore must yield zero orphans; got {orphans:?}"
    );

    // Re-tick — mesh re-projects matching the original.
    let r = fresh_projection
        .tick(&mut fresh_world, &fresh_cad, tol())
        .expect("tick post-restore");
    assert_eq!(r.entities_reprojected, 1);
    let mesh_after = fresh_projection.projected_mesh(entity).expect("mesh2");
    assert_eq!(mesh_after.vertex_count(), mesh_before.vertex_count());
    assert_eq!(mesh_after.triangle_count(), mesh_before.triangle_count());
    let (min_after, max_after) = bbox(mesh_after);
    for i in 0..3 {
        assert!(
            (min_after[i] - min_before[i]).abs() < 1e-5,
            "axis {i} min drift: before={} after={}",
            min_before[i],
            min_after[i]
        );
        assert!(
            (max_after[i] - max_before[i]).abs() < 1e-5,
            "axis {i} max drift: before={} after={}",
            max_before[i],
            max_after[i]
        );
    }
}

/// Divergent-state recovery: capture cad-graph alongside projection but only
/// restore the projection (caller fails to honor the co-restore convention).
/// `validate_handles` surfaces the orphan, and a subsequent `tick` against
/// the empty cad-graph returns `ProjectionError::NodeNotInGraph` rather than
/// panicking.
#[test]
fn validate_handles_detects_orphan_after_partial_restore() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut cad = CadGraph::new();
    let node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "cuboid for divergent test");

    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");
    let _ = projection.tick(&mut world, &cad, tol()).expect("tick1");

    // Capture with BOTH participants...
    let snap = PieSnapshot::capture(
        &world,
        &[
            &cad as &dyn SnapshotParticipate,
            &projection as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture");

    // ...but on restore, register ONLY the projection participant. We
    // simulate the divergent-state by also passing a pristine empty
    // cad-graph that doesn't carry the captured node.
    //
    // The `PieSnapshot::restore` API requires a handler for every id in the
    // snapshot — to skip cad-graph we'd have to drop its participant from
    // the captured snapshot first. Take a clone, remove cad-graph's payload,
    // then restore.
    let mut snap_no_cad = snap.clone();
    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");
    snap_no_cad.participants.remove(&pid_cad);
    assert_eq!(snap_no_cad.participants.len(), 1, "only projection remains");

    let mut fresh_world = World::new();
    fresh_world.register_snapshot_component::<BRepHandle>();
    let empty_cad = CadGraph::new();
    let mut fresh_projection = CadProjection::new();
    snap_no_cad
        .restore(
            &mut fresh_world,
            &mut [(
                &pid_proj,
                &mut fresh_projection as &mut dyn SnapshotParticipate,
            )],
        )
        .expect("restore projection-only");

    // BRepHandle entity lives in the fresh world (it was in world_bytes),
    // but its cad_node references a node that doesn't exist in `empty_cad`.
    let er = fresh_world.entity(entity).expect("entity preserved");
    let handle = er.get::<BRepHandle>().expect("brep handle preserved");
    assert_eq!(
        handle.cad_node, node,
        "BRepHandle.cad_node points at the original (now-orphaned) node"
    );

    // validate_handles surfaces the orphan.
    let orphans = fresh_projection.validate_handles(&empty_cad);
    assert_eq!(
        orphans.len(),
        1,
        "exactly one orphan expected; got {orphans:?}"
    );
    assert_eq!(orphans[0].0, entity, "orphan entity matches");
    assert_eq!(orphans[0].1, node, "orphan node matches");

    // Tick against empty_cad surfaces the divergent state cleanly:
    // ProjectionError::NodeNotInGraph rather than a panic.
    let res = fresh_projection.tick(&mut fresh_world, &empty_cad, tol());
    match res {
        Err(ProjectionError::NodeNotInGraph(n)) => {
            assert_eq!(n, node, "error carries the orphaned node id");
        }
        other => panic!("expected ProjectionError::NodeNotInGraph, got {other:?}"),
    }
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
/// `PluginError::Runtime` and marks the plugin Failed (per plugin-fatal
/// isolation), without panicking.
#[test]
fn cad_projection_plugin_tick_returns_error_when_world_missing() {
    let plugin = CadProjectionPlugin::new();
    let plugin_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    let init_report = host.init_all(&mut ctx);
    assert!(init_report.failed.is_empty());

    // Deliberately do NOT insert World. Tick must fail cleanly.
    let mut cad = CadGraph::new();
    let _node = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "missing-World canary");
    assert!(ctx.insert(cad).is_none());
    let _ = ctx.insert(tol());
    // Note: World absent.

    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(tick_report.ticked, 0);
    assert_eq!(
        tick_report.failed.len(),
        1,
        "missing World must surface as a failed tick"
    );
    let (failed_id, failed_msg) = &tick_report.failed[0];
    assert_eq!(*failed_id, plugin_id);
    assert!(
        failed_msg.contains("missing World"),
        "error message must mention missing World; got: {failed_msg}"
    );
    // Per plugin-fatal isolation, the plugin is now Failed.
    assert_eq!(host.state(&plugin_id), Some(PluginState::Failed));
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
