//! End-to-end smoke for D-Fillet sub-α — first BRepEdgeId consumer.
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. `FilletOp::new` accepts edge IDs that came from the upstream
//!    Cuboid's `BRepEdgeProvider`.
//! 2. `FilletOp::new` rejects synthesised edge IDs whose bytes don't
//!    correspond to any valid Cuboid edge.
//! 3. **Load-bearing rebuild-stability test**:
//!    `fillet_edge_ids_remain_valid_across_cuboid_parameter_rebuilds`
//!    captures an edge ID against a unit cube and proves it is still
//!    valid for `FilletOp::new` against a 2x2x2 cube. This is the
//!    end-to-end demonstration that the BRepEdgeId substrate carries
//!    weight as a real consumer.
//! 4. The structural delta (vertex / triangle counts added) is
//!    independent of cube size — same logical edge => same delta.
//! 5. End-to-end Cuboid → Fillet evaluation through `OperatorGraph`
//!    produces a well-formed tessellation.

use rge_cad_core::{
    brep_edge_ids_for_node, brep_face_ids_for_node, BRepEdgeId, BRepEdgeProvider, BRepFaceId,
    BRepOwnerId, BRepProvider, CadGraph, CuboidOp, FilletError, FilletOp, Operator, OperatorNode,
    Tessellation, TessellationCache, Tolerance, TopologyFaceId,
};

fn unit_cube() -> CuboidOp {
    CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    }
}

fn double_cube() -> CuboidOp {
    CuboidOp {
        width: 2.0,
        height: 2.0,
        depth: 2.0,
    }
}

/// All 12 edge IDs returned by the upstream Cuboid's
/// `BRepEdgeProvider` are accepted by `FilletOp::new`.
#[test]
fn fillet_validates_edge_ids_against_upstream_cuboid() {
    let owner = BRepOwnerId::from_bytes([0xed; 16]);
    let cube = unit_cube();
    let edge_ids = cube.brep_edge_ids(owner);
    assert_eq!(edge_ids.len(), 12);

    // All 12 IDs are valid against the upstream.
    let fillet = FilletOp::new(&cube, owner, edge_ids.clone(), 0.1).expect("valid edges accepted");
    assert_eq!(fillet.edges().len(), 12);
    assert_eq!(fillet.edges(), &edge_ids[..]);
}

/// A synthesised `BRepEdgeId` whose raw bytes don't correspond to any
/// canonical Cuboid edge under the supplied owner is rejected with
/// `FilletError::EdgeNotInUpstream`.
#[test]
fn fillet_rejects_unknown_edge_id() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let cube = unit_cube();

    // Synthesise an edge ID with bytes that won't correspond to any
    // valid Cuboid edge under this owner. Use raw bytes of all zeros.
    let phantom = BRepEdgeId::from_bytes([0u8; 16]);

    let result = FilletOp::new(&cube, owner, vec![phantom], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// **Load-bearing rebuild-stability test.**
///
/// Captures an edge ID against a unit cube, then verifies the same
/// edge ID is valid for `FilletOp::new` against a different-sized
/// cube. This is the end-to-end proof that the BRepEdgeId substrate
/// carries weight: an edge ID minted at one parameter set is still
/// a real consumer-usable identity at another parameter set.
#[test]
fn fillet_edge_ids_remain_valid_across_cuboid_parameter_rebuilds() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);

    // Capture an edge ID from the unit cube — the (NegZ, NegY) edge.
    let cube_a = unit_cube();
    let edge_id_x = cube_a.brep_edge_ids(owner)[0];

    // Rebuild a different-sized cube; the same canonical edge ID is
    // present in that cube's edge list (rebuild-stable substrate).
    let cube_b = double_cube();
    let edge_ids_b = cube_b.brep_edge_ids(owner);
    assert!(
        edge_ids_b.contains(&edge_id_x),
        "edge id captured against unit cube must remain in rebuilt 2x2x2 cube's edge list"
    );

    // FilletOp construction succeeds against both rebuilds with the
    // SAME edge ID — proves the substrate carries weight as a real
    // consumer.
    let fillet_a = FilletOp::new(&cube_a, owner, vec![edge_id_x], 0.1).expect("a");
    let fillet_b = FilletOp::new(&cube_b, owner, vec![edge_id_x], 0.1).expect("b");

    assert_eq!(fillet_a.edges(), fillet_b.edges());
    assert!((fillet_a.radius() - fillet_b.radius()).abs() < f32::EPSILON);
    // Same edge selection AND same radius AND same owner means the
    // structural hashes are byte-identical — Fillet's structural
    // hash captures only the operator's own parameters, not the
    // upstream Cuboid dimensions.
    assert_eq!(
        fillet_a.structural_hash(),
        fillet_b.structural_hash(),
        "FilletOp structural hash must depend only on (owner, edges, radius), not upstream Cuboid size"
    );
}

/// Filleting "the same logical edge" of unit cube and double cube
/// adds the same number of vertices and triangles. Geometric
/// positions differ (cubes are different sizes) but the STRUCTURAL
/// change is the same.
#[test]
fn fillet_rebuild_produces_same_structural_delta_across_sizes() {
    let owner = BRepOwnerId::from_bytes([0x9a; 16]);
    let cube_a = unit_cube();
    let cube_b = double_cube();
    let edge_id = cube_a.brep_edge_ids(owner)[0];

    let fillet_a = FilletOp::new(&cube_a, owner, vec![edge_id], 0.1).expect("a");
    let fillet_b = FilletOp::new(&cube_b, owner, vec![edge_id], 0.1).expect("b");

    let cube_a_tess = cube_a.evaluate(&[]).expect("cube_a tess");
    let cube_b_tess = cube_b.evaluate(&[]).expect("cube_b tess");

    let out_a = fillet_a.evaluate(&[&cube_a_tess]).expect("a output");
    let out_b = fillet_b.evaluate(&[&cube_b_tess]).expect("b output");

    // SAME structural delta (vertex count, triangle count).
    assert_eq!(out_a.positions.len(), cube_a_tess.positions.len() + 2);
    assert_eq!(out_b.positions.len(), cube_b_tess.positions.len() + 2);
    assert_eq!(out_a.indices.len(), cube_a_tess.indices.len() + 6);
    assert_eq!(out_b.indices.len(), cube_b_tess.indices.len() + 6);
}

/// Labeled upstream triangles keep their face labels through chamfer
/// `FilletOp`, while the newly appended chamfer-cap triangles are marked
/// `DEGENERATE` because they have no stable upstream `BRepFaceId`.
#[test]
fn fillet_preserves_upstream_face_labels_and_marks_chamfer_caps_degenerate() {
    let owner = BRepOwnerId::from_bytes([0x6a; 16]);
    let cube = unit_cube();
    let edge_id = cube.brep_edge_ids(owner)[0];
    let fillet = FilletOp::new(&cube, owner, vec![edge_id], 0.1).expect("fillet");

    let cube_tess = cube.evaluate(&[]).expect("cube tess");
    let upstream_labels = cube_tess
        .face_labels()
        .expect("CuboidOp emits labeled tessellation");
    assert_eq!(upstream_labels.len(), 12);
    assert!(fillet.output_is_labeled(&[true]));
    assert!(!fillet.output_is_labeled(&[false]));

    let out = fillet.evaluate(&[&cube_tess]).expect("fillet output");
    let out_labels = out
        .face_labels()
        .expect("labeled upstream should produce labeled fillet output");
    assert_eq!(out.triangle_count(), 14);
    assert_eq!(out_labels.len(), out.triangle_count());
    assert_eq!(
        &out_labels[..upstream_labels.len()],
        upstream_labels,
        "upstream triangle labels must pass through unchanged"
    );
    assert_eq!(
        &out_labels[upstream_labels.len()..],
        &[TopologyFaceId::DEGENERATE, TopologyFaceId::DEGENERATE],
        "new chamfer-cap triangles are nameless output-side geometry"
    );

    let unlabeled = Tessellation::new(cube_tess.positions.clone(), cube_tess.indices.clone())
        .expect("unlabeled cube copy");
    let out_unlabeled = fillet
        .evaluate(&[&unlabeled])
        .expect("fillet output from unlabeled input");
    assert!(
        out_unlabeled.face_labels().is_none(),
        "unlabeled upstream must remain unlabeled"
    );
}

/// End-to-end Cuboid -> Fillet through `CadGraph`/`OperatorGraph`
/// evaluates and produces a well-formed tessellation.
#[test]
fn fillet_through_operator_graph_evaluates_correctly() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let cube = unit_cube();
    let edges = cube.brep_edge_ids(owner);
    let fillet = FilletOp::new(&cube, owner, vec![edges[0]], 0.1).expect("ok");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cube_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cube))
        .expect("cube");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(cube_node, fillet_node, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    cad.commit("cuboid -> fillet").expect("commit");

    // Evaluate the chain end-to-end. The resolved tessellation should
    // have 10 positions (8 + 2) and 42 indices (36 + 6).
    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(fillet_node, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");
    assert_eq!(tess.positions.len(), 10);
    assert_eq!(tess.indices.len(), 42);
    assert_eq!(tess.triangle_count(), 14);
}

/// Zero radius is rejected at construction with `FilletError::InvalidRadius`.
#[test]
fn fillet_zero_radius_rejected() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let cube = unit_cube();
    let edge = cube.brep_edge_ids(owner)[0];
    let result = FilletOp::new(&cube, owner, vec![edge], 0.0);
    assert!(matches!(result, Err(FilletError::InvalidRadius { .. })));
}

/// Post-D-Fillet-sub-ε.α + sub-ε.β: both resolvers inherit for a
/// Fillet node, with different inheritance shapes:
///
/// * Face resolver (sub-ε.α): identity-preserving — every upstream
///   face exists bit-identical in the output mesh because
///   FilletOp.evaluate clones upstream positions/indices verbatim
///   and only appends chamfer-cap geometry.
/// * Edge resolver (sub-ε.β): filtered-inheriting — upstream edges
///   pass through except those in the FilletOp's selection
///   (`op.edges()`), which are excluded because they lose 2-endpoint
///   geometry under chamfering.
#[test]
fn fillet_node_face_and_edge_both_inherit_with_edge_filtering() {
    let owner = BRepOwnerId::from_bytes([0x77; 16]);
    let cube = unit_cube();
    let direct_face_ids: Vec<BRepFaceId> = cube
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let upstream_edges: Vec<BRepEdgeId> = cube.brep_edge_ids(owner);
    let filleted = vec![upstream_edges[0]];
    let fillet = FilletOp::new(&cube, owner, filleted.clone(), 0.1).expect("ok");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cube_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cube))
        .expect("cube");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(cube_node, fillet_node, 0)
        .expect("connect");
    cad.commit("cuboid -> fillet").expect("commit");

    // Face resolver: sub-ε.α inherits upstream Cuboid face IDs unchanged.
    let face_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), fillet_node, owner)
        .expect("face resolver inherits via sub-ε.α")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(
        face_ids, direct_face_ids,
        "Fillet face resolver must inherit Cuboid face IDs unchanged"
    );

    // Edge resolver: sub-ε.β inherits upstream Cuboid edges minus the
    // filleted selection. Surviving edges are byte-equal to the
    // upstream emission order with the filleted set removed.
    let edge_ids: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), fillet_node, owner)
        .expect("edge resolver inherits via sub-ε.β");
    let expected_edges: Vec<BRepEdgeId> = upstream_edges
        .iter()
        .filter(|e| !filleted.contains(e))
        .copied()
        .collect();
    assert_eq!(
        edge_ids, expected_edges,
        "Fillet edge resolver must inherit Cuboid edges minus the filleted set"
    );
    assert_eq!(edge_ids.len(), 11);
}
