//! Editor selection persistence sub-α end-to-end smoke for `FaceSelectionSet`
//! partition over `cad-projection::face_resolves_in_projection`.
//!
//! These tests prove:
//!
//! 1. A `FaceSelectionSet` built from a Cuboid's direct-provider face IDs
//!    partitions cleanly — every selection lands in survivors when the
//!    projection is intact.
//! 2. **LOAD-BEARING — rebuild stability across Cuboid parameter changes.**
//!    Rebuilding a cuboid with different dimensions (still a cuboid) leaves
//!    every face selection in survivors. The owner-seeded contract from
//!    D-7.2-α survives parameter rebuilds; the editor selection persistence
//!    substrate consumes that surface.
//! 3. **LOAD-BEARING — profile-count change invalidates Side selections
//!    (Extrude).** Square → Pentagon rebuilds preserve Bottom + Top (which
//!    are categorical per D-7.2-β) but invalidate the 4 square Side
//!    selections (Side IDs include `profile_count` in the tag).
//! 4. **LOAD-BEARING — filleted output preserves upstream face selections.**
//!    FilletOp preserves inherited Cuboid face labels and marks chamfer caps
//!    `TopologyFaceId::DEGENERATE`, so FaceSelections built from pre-fillet
//!    cuboid IDs remain in survivors when partitioned against the filleted
//!    projection.
//! 5. Owner mismatch invalidates everything (the entity's
//!    `BRepHandle.brep_owner` is set to a different owner than the
//!    selections were minted under).
//! 6. Empty sets partition into two empty sets.
//! 7. Selections referencing an unknown entity all land in invalidated.
//! 8. Round-tripping a `FaceSelectionSet` through RON preserves the
//!    partition outcome — serde correctness extends to partition correctness.

use rge_cad_core::{
    BRepEdgeProvider, BRepOwnerId, BRepProvider, CadGraph, CuboidOp, ExtrudeOp, FilletOp,
    OperatorNode, Polygon2D, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_state::{FaceSelection, FaceSelectionSet};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);
const OTHER_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0xab; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

fn pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("pentagon")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Cuboid
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn.
fn build_cuboid_projection(
    width: f32,
    height: f32,
    depth: f32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width,
            height,
            depth,
        }))
        .expect("add cuboid");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(cuboid_node)
        .expect("set root");
    graph.commit("test cuboid").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

fn build_extrude_projection(
    profile: Polygon2D,
    length: f32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let extrude = ExtrudeOp::new(profile, length).expect("extrude");
    let extrude_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("add extrude");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(extrude_node)
        .expect("set root");
    graph.commit("test extrude").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, extrude_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Build a `FaceSelectionSet` from every face id minted by `op`'s
/// `BRepProvider` impl under `owner`, scoped to `entity`. The set ends up
/// holding one `FaceSelection` per face emitted by the provider.
fn build_face_selection_set<P: BRepProvider>(
    op: &P,
    owner: BRepOwnerId,
    entity: EntityId,
) -> FaceSelectionSet {
    let mut set = FaceSelectionSet::new();
    for (_, face_id) in op.brep_face_ids(owner) {
        set.add(FaceSelection {
            entity,
            owner,
            face_id,
        });
    }
    set
}

/// All 6 face selections of an intact 1×1×1 cuboid resolve under the current
/// projection — the substrate-baseline test.
#[test]
fn face_selection_partition_all_resolve_for_intact_cuboid() {
    let (graph, projection, world, entity) = build_cuboid_projection(1.0, 1.0, 1.0);
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let set = build_face_selection_set(&cuboid, TEST_OWNER, entity);
    assert_eq!(set.len(), 6, "Cuboid emits exactly 6 face IDs");

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    assert_eq!(
        survivors.len(),
        6,
        "all 6 cuboid face selections must survive on intact projection"
    );
    assert!(invalidated.is_empty());
}

/// **LOAD-BEARING — rebuild stability across Cuboid parameter changes.**
///
/// Capture all 6 cuboid faces as a FaceSelectionSet at one parameter set,
/// rebuild the cuboid with different parameters (still a cuboid), and
/// partition the set against the rebuilt projection. Per the D-7.2-α
/// contract lifted through cad-projection (per D-projection-α), the same
/// owner + same operator-kind + same per-face tag yields the same
/// `BRepFaceId`, so all 6 selections must survive.
#[test]
fn face_selection_partition_survives_cuboid_parameter_rebuild() {
    let (mut graph, mut projection, mut world, entity) = build_cuboid_projection(1.0, 1.0, 1.0);
    let initial_cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let set = build_face_selection_set(&initial_cuboid, TEST_OWNER, entity);
    assert_eq!(set.len(), 6);

    // Rebuild as a 2×1×1 cuboid via a fresh node.
    graph.begin_operation().expect("begin");
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("rebuild cuboid");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("set root");
    graph.commit("rebuild 2x1x1").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("re-tick");

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    assert_eq!(
        survivors.len(),
        6,
        "all 6 cuboid faces must survive a parameter-only rebuild"
    );
    assert_eq!(invalidated.len(), 0);
}

/// **LOAD-BEARING — profile-count change invalidates Side selections
/// (Extrude).**
///
/// Capture the 6 face IDs of a square Extrude (Bottom + Top + 4 Sides) as a
/// FaceSelectionSet, rebuild as a pentagon Extrude on the same entity, and
/// partition. Per D-7.2-β:
///
/// * Bottom + Top are categorical (no `profile_count` in the tag) — they
///   resolve under the new pentagon projection (the pentagon also emits
///   Bottom + Top with the same categorical IDs).
/// * Side IDs include `profile_count` in the tag — every square Side ID is
///   disjoint from every pentagon Side ID, so the 4 captured Side
///   selections do NOT resolve in the pentagon projection.
///
/// Expected partition: 2 survivors (Bottom + Top) + 4 invalidated (Sides).
#[test]
fn face_selection_partition_invalidates_side_selections_on_profile_count_change() {
    let (mut graph, mut projection, mut world, entity) =
        build_extrude_projection(unit_square(), 1.0);
    let initial_extrude = ExtrudeOp::new(unit_square(), 1.0).expect("extrude");
    let set = build_face_selection_set(&initial_extrude, TEST_OWNER, entity);
    assert_eq!(set.len(), 6, "square Extrude emits 6 face IDs (n+2)");

    // Rebuild as a pentagon Extrude on the same entity.
    graph.begin_operation().expect("begin");
    let new_extrude = ExtrudeOp::new(pentagon(), 1.0).expect("pentagon");
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(new_extrude))
        .expect("rebuild extrude");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("set root");
    graph.commit("rebuild pentagon").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("re-tick");

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    // Bottom + Top survive (categorical caps); 4 square Sides invalidated.
    assert_eq!(
        survivors.len(),
        2,
        "Bottom + Top must survive (categorical per D-7.2-β)"
    );
    assert_eq!(
        invalidated.len(),
        4,
        "all 4 square Side selections must be invalidated by topology change \
         (profile_count is in Side tag per D-7.2-β)"
    );
}

/// **LOAD-BEARING — filleted output preserves upstream face selections.**
///
/// Build a `Cuboid → Fillet` graph and bind the entity to the FILLET node.
/// Capture the 6 cuboid face IDs as a FaceSelectionSet (built from the
/// upstream cuboid's `BRepProvider` impl, so each selection holds a
/// well-formed `BRepFaceId`). Partition the set against the filleted
/// projection.
///
/// FilletOp preserves inherited Cuboid labels and the face resolver inherits
/// Cuboid `BRepFaceId`s through the Fillet root. The pre-fillet face
/// selections therefore remain resolvable; only chamfer-cap triangles are
/// `TopologyFaceId::DEGENERATE` and nameless.
#[test]
fn face_selection_partition_preserves_upstream_faces_on_filleted_output() {
    // Build a Cuboid → Fillet graph, with the entity bound to the FILLET
    // root (so the projected mesh is the filleted output).
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("cuboid");
    let edge_id = cuboid.brep_edge_ids(TEST_OWNER)[0];
    let fillet =
        FilletOp::new(&cuboid, TEST_OWNER, vec![edge_id], 0.1).expect("fillet construction");
    let fillet_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet node");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, fillet_node, 0)
        .expect("connect");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    graph.commit("cuboid -> fillet").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, fillet_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // Build the FaceSelectionSet from the upstream cuboid's face IDs.
    let set = build_face_selection_set(&cuboid, TEST_OWNER, entity);
    assert_eq!(set.len(), 6);

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    assert_eq!(
        survivors.len(),
        6,
        "FilletOp should preserve all upstream Cuboid face selections"
    );
    assert_eq!(
        invalidated.len(),
        0,
        "no upstream Cuboid face selection should invalidate through FilletOp"
    );
}

/// Owner mismatch invalidates everything: the entity's
/// `BRepHandle.brep_owner` is set to OWNER while selections were minted
/// under TEST_OWNER. `face_resolves_in_projection` short-circuits to false
/// on owner mismatch, so the partition lands every selection in
/// invalidated.
#[test]
fn face_selection_partition_invalidates_on_owner_mismatch() {
    let (graph, projection, mut world, entity) = build_cuboid_projection(1.0, 1.0, 1.0);
    // Build the set under TEST_OWNER…
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let set = build_face_selection_set(&cuboid, TEST_OWNER, entity);

    // …then mutate the entity's BRepHandle.brep_owner to a different owner.
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(OTHER_OWNER);
        }
    }

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    assert_eq!(survivors.len(), 0);
    assert_eq!(
        invalidated.len(),
        6,
        "owner mismatch must invalidate every selection in the set"
    );
}

/// An empty set partitions into two empty sets regardless of predicate.
#[test]
fn face_selection_partition_returns_empty_for_empty_set() {
    let empty = FaceSelectionSet::new();
    let (survivors, invalidated) = empty.partition(|_| true);
    assert!(survivors.is_empty());
    assert!(invalidated.is_empty());

    let (survivors, invalidated) = empty.partition(|_| false);
    assert!(survivors.is_empty());
    assert!(invalidated.is_empty());
}

/// Selections referencing an entity not present in the world all land in
/// invalidated. `face_resolves_in_projection` returns `false` for unknown
/// entities, which the partition uniformly classifies as invalidated.
#[test]
fn face_selection_partition_returns_empty_for_unknown_entity() {
    let (graph, projection, world, _real_entity) = build_cuboid_projection(1.0, 1.0, 1.0);
    let phantom = EntityId::new();
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let set = build_face_selection_set(&cuboid, TEST_OWNER, phantom);
    assert_eq!(set.len(), 6);

    let (survivors, invalidated) = set.partition(|fs| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    });
    assert_eq!(survivors.len(), 0);
    assert_eq!(
        invalidated.len(),
        6,
        "selections for an unknown entity must all land in invalidated"
    );
}

/// RON round-trip preserves partition outcome — serde correctness extends
/// to partition correctness. Build a FaceSelectionSet, serialize +
/// deserialize via RON, partition both, and assert identical
/// `(survivors, invalidated)`.
#[test]
fn face_selection_round_trip_through_ron_preserves_partition_outcome() {
    let (graph, projection, world, entity) = build_cuboid_projection(1.0, 1.0, 1.0);
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let set = build_face_selection_set(&cuboid, TEST_OWNER, entity);

    let serialized = ron::to_string(&set).expect("serialize FaceSelectionSet");
    let restored: FaceSelectionSet = ron::from_str(&serialized).expect("deserialize");
    assert_eq!(set, restored);

    let predicate = |fs: &FaceSelection| {
        projection.face_resolves_in_projection(
            fs.entity,
            fs.owner,
            fs.face_id,
            &world,
            graph.graph(),
        )
    };
    let (orig_survivors, orig_invalidated) = set.partition(predicate);
    let (rest_survivors, rest_invalidated) = restored.partition(predicate);
    assert_eq!(orig_survivors, rest_survivors);
    assert_eq!(orig_invalidated, rest_invalidated);
}
