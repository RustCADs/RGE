//! D-projection-α end-to-end smoke for cad-projection face-ID integration.
//!
//! These tests prove:
//!
//! 1. `CadProjection::brep_face_id_for_triangle` returns a stable
//!    [`BRepFaceId`] for every triangle of a Cuboid root, matching the
//!    direct-provider face IDs.
//! 2. The mapping is canonical: triangles 0-1 → NegZ, 2-3 → PosZ, 4-5 →
//!    NegY, 6-7 → PosY, 8-9 → NegX, 10-11 → PosX.
//! 3. Out-of-bounds triangle indices and unknown entities return `None`.
//! 4. An entity with `brep_owner: None` returns `None` even with a valid
//!    projected mesh.
//! 5. **LOAD-BEARING: rebuild stability across Cuboid parameter changes.**
//!    A face ID captured at one parameter set is byte-identical to the face
//!    ID resolved at a different parameter set with the same owner. This is
//!    the cad-projection consumer-pressure test for the D-7.2-α
//!    rebuild-stability contract.
//! 6. **LOAD-BEARING: Cuboid -> Fillet face lookup.** Inherited upstream
//!    triangles resolve to the upstream Cuboid face IDs, while appended
//!    chamfer-cap triangles labeled `TopologyFaceId::DEGENERATE` resolve
//!    to `None`.
//! 7. Distinct owners produce disjoint face IDs even through the same
//!    projected entity.

use rge_cad_core::{
    BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, CuboidOp, FilletOp,
    OperatorNode, Tolerance, TopologyFaceId,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Cuboid
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn so `brep_face_id_for_triangle` resolves
/// against a known owner-seed.
fn build_cuboid_projection() -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
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

    // Set the owner-seed on the BRepHandle component post-spawn. The
    // existing `spawn_brep_entity` API leaves brep_owner as `None`
    // (additive default per D-projection-α).
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }

    // Tick to project the mesh.
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Each of the 12 triangles in a Cuboid's projected mesh resolves to one
/// of the 6 stable `BRepFaceId`s minted by the upstream's
/// `BRepProvider::brep_face_ids` impl.
#[test]
fn cuboid_projection_query_returns_brep_face_id_for_each_triangle() {
    let (graph, projection, world, entity) = build_cuboid_projection();
    let mesh = projection.projected_mesh(entity).expect("mesh");
    assert_eq!(mesh.triangle_count(), 12);

    // Direct provider face IDs for comparison.
    let cuboid_for_compare = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let direct_pairs = cuboid_for_compare.brep_face_ids(TEST_OWNER);
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    // Each of 12 triangles maps to one of the 6 face IDs.
    for tri in 0..12 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// The triangle → `BRepFaceId` mapping follows the canonical Cuboid face
/// emission order documented in `CuboidOp::evaluate` and `impl
/// BRepProvider for CuboidOp`:
///
/// * Triangles 0-1 → NegZ (face id 0)
/// * Triangles 2-3 → PosZ (1)
/// * Triangles 4-5 → NegY (2)
/// * Triangles 6-7 → PosY (3)
/// * Triangles 8-9 → NegX (4)
/// * Triangles 10-11 → PosX (5)
#[test]
fn cuboid_projection_query_canonical_face_order() {
    let (graph, projection, world, entity) = build_cuboid_projection();
    let cuboid_for_compare = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let direct_ids: Vec<BRepFaceId> = cuboid_for_compare
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(direct_ids.len(), 6);

    for tri in 0..12 {
        let expected = direct_ids[tri / 2];
        let actual = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(
            actual, expected,
            "triangle {tri} expected face id {expected:?}, got {actual:?}"
        );
    }
}

/// An out-of-bounds triangle index returns `None` rather than panicking.
#[test]
fn cuboid_query_returns_none_for_out_of_bounds_triangle() {
    let (graph, projection, world, entity) = build_cuboid_projection();
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 99, &world, graph.graph()),
        None
    );
}

/// Querying a never-spawned entity returns `None`.
#[test]
fn cuboid_query_returns_none_for_unknown_entity() {
    let (graph, projection, world, _real_entity) = build_cuboid_projection();
    let phantom = EntityId::new();
    assert_eq!(
        projection.brep_face_id_for_triangle(phantom, 0, &world, graph.graph()),
        None
    );
}

/// An entity whose `BRepHandle.brep_owner` is `None` (the legacy default)
/// returns `None` even when the projected mesh has `face_labels`.
#[test]
fn cuboid_query_returns_none_when_brep_owner_is_none() {
    let (graph, projection, mut world, entity) = build_cuboid_projection();

    // Clear the brep_owner on the existing entity.
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }

    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 0, &world, graph.graph()),
        None
    );
}

/// **LOAD-BEARING — rebuild stability across Cuboid parameter changes.**
///
/// Project Cuboid(1,1,1), capture the per-triangle face IDs, rebuild the
/// graph as Cuboid(2,1,1), re-project, and verify the same triangle
/// indices map to the same `BRepFaceId`s. The owner-seeded contract from
/// D-7.2-α — "BRepFaceIds are stable across parameter rebuilds when the
/// owner is the same and the face tag is the same" — is the contract this
/// test validates from the consumer side via `cad-projection`.
#[test]
fn cuboid_face_ids_stable_across_parameter_rebuilds() {
    // Stage 1 — Cuboid(1,1,1).
    let (mut graph, mut projection, mut world, entity) = build_cuboid_projection();
    let initial_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(initial_ids.len(), 12);

    // Stage 2 — rebuild as Cuboid(2,1,1) via begin_operation / add_operator
    // / set_root. The new node has a different content-derived NodeId.
    graph.begin_operation().expect("begin");
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid 2x1x1");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("set root");
    graph.commit("rebuild cuboid").expect("commit");

    // Update entity's mapping to the new node.
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("re-tick");

    // Stage 3 — capture face IDs at the new parameter set and compare.
    let rebuilt_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across parameter rebuilds (D-7.2-α contract \
         lifted through cad-projection per D-projection-α)"
    );
}

/// Cuboid -> Fillet output resolves inherited upstream face labels through
/// the Fillet root, while appended chamfer-cap triangles remain nameless
/// `DEGENERATE` geometry and resolve to `None`.
#[test]
fn cuboid_through_fillet_resolves_upstream_faces_and_degenerate_caps() {
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

    let mesh = projection.projected_mesh(entity).expect("mesh");
    let face_labels = mesh
        .face_labels
        .as_ref()
        .expect("FilletOp should preserve labels from labeled Cuboid input");
    assert_eq!(
        mesh.triangle_count(),
        14,
        "Cuboid + one chamfer edge emits 12 upstream triangles + 2 chamfer caps"
    );
    assert_eq!(
        face_labels.len(),
        mesh.triangle_count(),
        "one face label per projected triangle"
    );

    let direct_pairs = cuboid.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 6);
    let non_degenerate: Vec<TopologyFaceId> = face_labels
        .iter()
        .copied()
        .filter(|label| *label != TopologyFaceId::DEGENERATE)
        .collect();
    assert_eq!(
        non_degenerate.len(),
        12,
        "the original 12 Cuboid triangles keep their labels"
    );
    assert!(
        face_labels
            .iter()
            .any(|label| *label == TopologyFaceId::DEGENERATE),
        "the appended chamfer caps must be marked DEGENERATE"
    );

    for (topo, _) in &direct_pairs {
        assert!(
            non_degenerate.contains(topo),
            "Cuboid face {topo:?} must survive as an inherited Fillet label"
        );
    }

    for tri in 0..mesh.triangle_count() {
        let label = face_labels[tri];
        let resolved = projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph());
        if label == TopologyFaceId::DEGENERATE {
            assert_eq!(
                resolved, None,
                "triangle {tri}: chamfer-cap geometry has no stable BRepFaceId"
            );
        } else {
            let expected = direct_pairs
                .iter()
                .find(|(topo, _)| *topo == label)
                .map(|(_, id)| *id)
                .unwrap_or_else(|| panic!("triangle {tri}: unknown inherited label {label:?}"));
            assert_eq!(
                resolved,
                Some(expected),
                "triangle {tri}: inherited Fillet label must resolve to upstream Cuboid face id"
            );
        }
    }
}

/// Distinct `BRepOwnerId` seeds produce disjoint `BRepFaceId` spaces even
/// when the geometry is byte-identical. Owner-seeded identity is preserved
/// through the projection consumer surface.
#[test]
fn distinct_owners_produce_disjoint_face_ids_through_projection() {
    let (graph, projection, mut world, entity) = build_cuboid_projection();
    let owner_y = BRepOwnerId::from_bytes([0xab; 16]);
    assert_ne!(TEST_OWNER, owner_y, "owners must be distinct for this test");

    // Capture x-space face IDs (TEST_OWNER seeded by build_cuboid_projection).
    let ids_x: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(ids_x.len(), 12);

    // Switch the entity's owner to owner_y and re-resolve.
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner_y);
        }
    }
    // No need to re-tick: brep_face_id_for_triangle reads
    // BRepHandle.brep_owner directly and resolves through the resolver.
    let ids_y: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(ids_y.len(), 12);

    // Disjoint sets — owner-seeded.
    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-x face id leaked into owner-y space"
        );
    }
}
