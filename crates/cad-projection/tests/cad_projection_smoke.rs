//! Phase 7.3 integration smoke tests for `cad-projection`.
//!
//! Substrate-level smoke tests for `CadProjection` itself. The Tier-2
//! `CadProjectionPlugin` canary tests live in the sibling
//! `plugin_adapter_smoke.rs` (matching the audio/gfx/physics convention).
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
//! 5. **Pairing-6 BRepHandle SSoT regression tests** — guard the
//!    post-2026-05-08 invariant that `BRepHandle` does NOT carry the cad-node
//!    FK; the FK lives exclusively in `EntityCadMap`. Cover the
//!    `node_for` / `entity_for` / `remap_entity` accessors.

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection, ProjectedMesh, ProjectionError};
use rge_kernel_ecs::{ParticipantId, PieSnapshot, SnapshotParticipate, World};
use rge_kernel_graph_foundation::NodeId;

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

    // Post-2026-05-08 SSoT refactor: the cad-node FK lives ONLY in the
    // EntityCadMap. To remap an entity to a different cad-node, we use
    // `CadProjection::remap_entity` (no separate handle field write needed).
    // Despawn + respawn would also work, but `remap_entity` exercises the
    // canonical accessor path and preserves the original EntityId for
    // post-remap verification.
    projection
        .remap_entity(entity, node_c2)
        .expect("remap entity to node_c2");

    // Tick — head advanced, every entity dirty, re-project.
    let r2 = projection.tick(&mut world, &cad, tol()).expect("tick2");
    assert_eq!(r2.entities_reprojected, 1);
    assert_eq!(r2.head_advanced_to, head_c2);

    let mesh2 = projection.projected_mesh(entity).expect("mesh2");
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

    // The world has the original entity + its BRepHandle, with bookkeeping
    // fields zeroed (mesh_id re-derives on the next tick post-restore per
    // the SnapshotParticipate convention).
    let er = world2.entity(entity).expect("entity preserved");
    let _handle = er.get::<BRepHandle>().expect("brep handle preserved");

    // Post-2026-05-08 SSoT refactor: the cad-node FK lives in the
    // EntityCadMap, not the BRepHandle. The projection's map survived
    // PIE round-trip, so the entity-↔-node binding is preserved.
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

    // BRepHandle entity lives in the fresh world (it was in world_bytes).
    // Post-2026-05-08 SSoT refactor: the cad-node FK lives in the
    // EntityCadMap, not the handle. The projection's map (recovered from
    // the snapshot) still records `entity → node`, but `node` doesn't
    // exist in `empty_cad` — exactly the orphan condition we're testing.
    let er = fresh_world.entity(entity).expect("entity preserved");
    let _handle = er.get::<BRepHandle>().expect("brep handle preserved");
    assert_eq!(
        fresh_projection.node_for(entity),
        Some(node),
        "EntityCadMap (the SSoT) still records the original (now-orphaned) node",
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
// Pairing-6 closure: BRepHandle SSoT regression tests
//
// These tests guard the post-2026-05-08 invariant that `BRepHandle` does NOT
// carry the cad-node FK; the FK lives exclusively in `EntityCadMap`. If a
// future refactor accidentally re-introduces a `cad_node` field on the
// handle, `brep_handle_does_not_carry_cad_node_field` flags the regression
// at runtime via the serialized payload.
// ===========================================================================

/// Compile-time + runtime guard: `BRepHandle`'s only state is `mesh_id` and
/// `last_projected_checkpoint`. The cad-node FK is owned exclusively by
/// `EntityCadMap`. If `BRepHandle.cad_node` is ever re-introduced, the RON
/// payload check below will fail.
#[test]
fn brep_handle_does_not_carry_cad_node_field() {
    let handle = BRepHandle::new();
    assert_eq!(handle.mesh_id, None);
    assert_eq!(handle.last_projected_checkpoint, None);

    // Default impl matches new(): both produce identical zero-state handles.
    assert_eq!(handle, BRepHandle::default());

    // Serialize and assert the wire format does not contain the field name
    // `cad_node`. This is a structural guard against accidental
    // re-introduction of the duplicated FK.
    let serialized = ron::ser::to_string(&handle).expect("serialize handle");
    assert!(
        !serialized.contains("cad_node"),
        "BRepHandle should not serialize a cad_node field; SSoT is EntityCadMap. Got: {serialized}",
    );
}

/// `CadProjection::node_for` and `entity_for` accessors round-trip the
/// entity ↔ cad-node binding through the (single source of truth)
/// `EntityCadMap`. Callers replacing the old `handle.cad_node` field read
/// migrate to `projection.node_for(entity)`.
#[test]
fn cad_projection_node_for_accessor_returns_mapped_node() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut cad = CadGraph::new();
    let cube_id = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "node_for accessor canary");

    let entity = projection
        .spawn_brep_entity(&mut world, cube_id)
        .expect("spawn");

    // Forward and reverse lookup both resolve through the EntityCadMap.
    assert_eq!(projection.node_for(entity), Some(cube_id));
    assert_eq!(projection.entity_for(cube_id), Some(entity));

    // An unknown entity / node yields None.
    let unknown_node = NodeId::from_raw(0xdead_beef);
    assert_eq!(projection.entity_for(unknown_node), None);
}

/// `CadProjection::remap_entity` updates the `EntityCadMap`'s binding and
/// marks the entity dirty so the next `tick` re-projects against the new
/// node. Replaces the old `handle.cad_node = new_node` field-write idiom.
#[test]
fn cad_projection_remap_entity_marks_dirty_for_reprojection() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut cad = CadGraph::new();
    let node_a = add_cuboid(&mut cad, 1.0, 1.0, 1.0, "remap canary A: cuboid(1,1,1)");

    // Initial spawn + tick populates the projected mesh for node_a.
    let entity = projection
        .spawn_brep_entity(&mut world, node_a)
        .expect("spawn");
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);
    let mesh_a = projection.projected_mesh(entity).expect("mesh_a").clone();
    let (min_a, max_a) = bbox(&mesh_a);
    // Cuboid(1,1,1) has corners at ±0.5.
    for i in 0..3 {
        assert!((min_a[i] - (-0.5)).abs() < 1e-5);
        assert!((max_a[i] - 0.5).abs() < 1e-5);
    }

    // Add a second cuboid (different size → different content-derived
    // NodeId) and remap the entity to it.
    let node_b = add_cuboid(&mut cad, 3.0, 3.0, 3.0, "remap canary B: cuboid(3,3,3)");
    projection.remap_entity(entity, node_b).expect("remap");

    // The EntityCadMap now records the new binding.
    assert_eq!(projection.node_for(entity), Some(node_b));
    assert_eq!(projection.entity_for(node_b), Some(entity));
    assert_eq!(
        projection.entity_for(node_a),
        None,
        "old node should no longer map to anything",
    );

    // Tick re-projects: remap marked the entity dirty and the head also
    // advanced (second commit). The mesh now reflects the bigger cuboid.
    let r2 = projection.tick(&mut world, &cad, tol()).expect("tick2");
    assert_eq!(r2.entities_reprojected, 1);
    let mesh_b = projection.projected_mesh(entity).expect("mesh_b");
    let (min_b, max_b) = bbox(mesh_b);
    // Cuboid(3,3,3) has corners at ±1.5.
    for i in 0..3 {
        assert!(
            (min_b[i] - (-1.5)).abs() < 1e-5,
            "axis {i} min should be -1.5 after remap; got {}",
            min_b[i],
        );
        assert!(
            (max_b[i] - 1.5).abs() < 1e-5,
            "axis {i} max should be +1.5 after remap; got {}",
            max_b[i],
        );
    }
}

/// `CadProjection::remap_entity` returns `EntityCadMapError::NotFound` when
/// the entity is not registered in the projection.
#[test]
fn cad_projection_remap_unknown_entity_errors_not_found() {
    use rge_cad_projection::EntityCadMapError;

    let mut projection = CadProjection::new();
    let bogus_entity = rge_kernel_ecs::EntityId::new();
    let some_node = NodeId::from_raw(0xfeed);

    let err = projection
        .remap_entity(bogus_entity, some_node)
        .expect_err("unknown entity must error");
    assert!(matches!(err, EntityCadMapError::NotFound));
}
