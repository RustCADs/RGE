//! End-to-end smoke for D-Fillet sub-β — Extrude variant of the
//! BRepEdgeId consumer pattern.
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. `FilletOp::new_for_extrude` accepts edge IDs that came from the
//!    upstream Extrude's `BRepEdgeProvider`.
//! 2. `FilletOp::new_for_extrude` rejects synthesised edge IDs whose
//!    bytes don't correspond to any valid Extrude edge.
//! 3. **Load-bearing rebuild-stability test (length axis)**:
//!    `fillet_extrude_edge_ids_remain_valid_across_length_changes`
//!    captures an edge ID against a unit-square extrude at length=1
//!    and proves it is still valid for `FilletOp::new_for_extrude`
//!    against rebuilds at length=2 and length=0.5. Length is a
//!    topology-preserving parameter (D-7.2-ζ.β).
//! 4. **Load-bearing topology-change test (profile_count axis)**:
//!    `fillet_extrude_edge_ids_invalidated_by_profile_count_change`
//!    proves that the square's edge IDs do NOT validate against a
//!    pentagon upstream — profile-count changes break edge identity
//!    by construction (D-7.2-ζ.β).
//! 5. The structural delta (vertex / triangle counts added) is
//!    independent of length — same logical edge across length changes
//!    => same delta.
//! 6. End-to-end Extrude → Fillet evaluation through `CadGraph`
//!    produces a well-formed tessellation.

use rge_cad_core::{
    BRepEdgeId, BRepEdgeProvider, BRepOwnerId, CadGraph, ExtrudeOp, FilletError, FilletOp,
    Operator, OperatorNode, Polygon2D, TessellationCache, Tolerance,
};

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("ccw unit square")
}

fn small_pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("ccw regular pentagon")
}

/// All `3 * N` edge IDs returned by the upstream Extrude's
/// `BRepEdgeProvider` are accepted by `FilletOp::new_for_extrude`.
#[test]
fn fillet_validates_extrude_edge_ids_against_upstream() {
    let owner = BRepOwnerId::from_bytes([0xed; 16]);
    let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("extrude");
    let edge_ids = extrude.brep_edge_ids(owner);
    assert_eq!(edge_ids.len(), 12); // 3 * N=4

    let fillet =
        FilletOp::new_for_extrude(&extrude, owner, edge_ids.clone(), 0.1).expect("valid edges");
    assert_eq!(fillet.edges().len(), 12);
    assert_eq!(fillet.edges(), &edge_ids[..]);
}

/// A synthesised `BRepEdgeId` whose raw bytes don't correspond to any
/// canonical Extrude edge under the supplied owner is rejected with
/// `FilletError::EdgeNotInUpstream`.
#[test]
fn fillet_rejects_unknown_extrude_edge_id() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("extrude");
    let phantom = BRepEdgeId::from_bytes([0u8; 16]);
    let result = FilletOp::new_for_extrude(&extrude, owner, vec![phantom], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// **Load-bearing rebuild-stability test (length axis).**
///
/// Length is a topology-preserving parameter. Edge IDs must be stable
/// across length rebuilds (D-7.2-ζ.β proved this); FilletOp
/// construction must therefore succeed against rebuilt extrudes with
/// the same edge ID list.
#[test]
fn fillet_extrude_edge_ids_remain_valid_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let square = unit_square();
    let extrude_a = ExtrudeOp::new(square.clone(), 1.0).expect("len=1");
    let edge_ids = extrude_a.brep_edge_ids(owner);
    let edge_id_x = edge_ids[0];

    // Same edge ID is valid against rebuilds at different lengths.
    let extrude_b = ExtrudeOp::new(square.clone(), 2.0).expect("len=2");
    let extrude_c = ExtrudeOp::new(square, 0.5).expect("len=0.5");
    let edges_b = extrude_b.brep_edge_ids(owner);
    let edges_c = extrude_c.brep_edge_ids(owner);
    assert!(
        edges_b.contains(&edge_id_x),
        "edge id captured against len=1 must remain in rebuilt len=2 edge list"
    );
    assert!(
        edges_c.contains(&edge_id_x),
        "edge id captured against len=1 must remain in rebuilt len=0.5 edge list"
    );

    let fillet_a = FilletOp::new_for_extrude(&extrude_a, owner, vec![edge_id_x], 0.1).expect("a");
    let fillet_b = FilletOp::new_for_extrude(&extrude_b, owner, vec![edge_id_x], 0.1).expect("b");
    let fillet_c = FilletOp::new_for_extrude(&extrude_c, owner, vec![edge_id_x], 0.1).expect("c");
    assert_eq!(fillet_a.edges(), fillet_b.edges());
    assert_eq!(fillet_b.edges(), fillet_c.edges());
    // Same edge selection AND same radius AND same owner means the
    // structural hashes are byte-identical — Fillet's structural
    // hash captures only the operator's own parameters, not the
    // upstream Extrude length.
    assert_eq!(
        fillet_a.structural_hash(),
        fillet_b.structural_hash(),
        "FilletOp structural hash must depend only on (owner, edges, radius), not upstream length"
    );
}

/// **Load-bearing topology-change test (profile_count axis).**
///
/// Profile count change breaks edge IDs (per D-7.2-ζ.β) — the
/// square's edge IDs must NOT be valid against the pentagon's
/// upstream, confirming topology-change rejection.
#[test]
fn fillet_extrude_edge_ids_invalidated_by_profile_count_change() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let square_extrude = ExtrudeOp::new(unit_square(), 1.0).expect("sq");
    let pentagon_extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("pen");

    let square_edge = square_extrude.brep_edge_ids(owner)[0];
    let pentagon_edges = pentagon_extrude.brep_edge_ids(owner);

    // Square's edge ID must not appear in pentagon's edge list — the
    // BRepEdgeId derivation hashes the Side face's `profile_count`
    // (4 vs 5) into its BLAKE3 input, so the edges are disjoint sets
    // by construction.
    assert!(
        !pentagon_edges.contains(&square_edge),
        "square's bottom-perimeter edge[0] must NOT collide with any pentagon edge ID"
    );

    // FilletOp::new_for_extrude with the square's edge ID against the
    // pentagon must error with EdgeNotInUpstream.
    let result = FilletOp::new_for_extrude(&pentagon_extrude, owner, vec![square_edge], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// Filleting "the same logical edge" of an Extrude at different
/// lengths produces the same structural delta (vertex/index count
/// change is identical even though absolute positions differ).
#[test]
fn fillet_extrude_rebuild_produces_same_structural_delta_across_lengths() {
    let owner = BRepOwnerId::from_bytes([0x9a; 16]);
    let extrude_a = ExtrudeOp::new(unit_square(), 1.0).expect("a");
    let extrude_b = ExtrudeOp::new(unit_square(), 3.0).expect("b");
    let edge_id = extrude_a.brep_edge_ids(owner)[0];

    let fillet_a = FilletOp::new_for_extrude(&extrude_a, owner, vec![edge_id], 0.1).expect("a");
    let fillet_b = FilletOp::new_for_extrude(&extrude_b, owner, vec![edge_id], 0.1).expect("b");

    let tess_a = extrude_a.evaluate(&[]).expect("eval a");
    let tess_b = extrude_b.evaluate(&[]).expect("eval b");

    let out_a = fillet_a.evaluate(&[&tess_a]).expect("out a");
    let out_b = fillet_b.evaluate(&[&tess_b]).expect("out b");

    // Same structural delta: each fillet adds 2 vertices and 2
    // triangles (= 6 indices).
    assert_eq!(out_a.positions.len(), tess_a.positions.len() + 2);
    assert_eq!(out_b.positions.len(), tess_b.positions.len() + 2);
    assert_eq!(out_a.indices.len(), tess_a.indices.len() + 6);
    assert_eq!(out_b.indices.len(), tess_b.indices.len() + 6);
}

/// End-to-end Extrude → Fillet through `CadGraph`/`OperatorGraph`
/// evaluates and produces a well-formed tessellation.
#[test]
fn fillet_extrude_through_operator_graph_evaluates_correctly() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
    let edge_id = extrude.brep_edge_ids(owner)[0];
    let fillet = FilletOp::new_for_extrude(&extrude, owner, vec![edge_id], 0.1).expect("fillet");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let extrude_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("extrude");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(extrude_node, fillet_node, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    cad.commit("extrude -> fillet").expect("commit");

    // Evaluate end-to-end. Square extrude: 2N=8 verts, 4N-4=12
    // triangles (36 indices). After 1 fillet: +2 verts + 6 indices.
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
fn fillet_extrude_zero_radius_rejected() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
    let edge = extrude.brep_edge_ids(owner)[0];
    let result = FilletOp::new_for_extrude(&extrude, owner, vec![edge], 0.0);
    assert!(matches!(result, Err(FilletError::InvalidRadius { .. })));
}

/// Fillet all `3 * N` edges of a pentagon Extrude — confirms the
/// constructor validates and the evaluation produces the expected
/// linear structural delta for variable-N topology.
#[test]
fn fillet_extrude_all_edges_pentagon() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let extrude = ExtrudeOp::new(small_pentagon(), 2.0).expect("ext");
    let edges = extrude.brep_edge_ids(owner);
    assert_eq!(edges.len(), 15); // 3 * N=5

    let fillet = FilletOp::new_for_extrude(&extrude, owner, edges, 0.05).expect("all");
    let tess = extrude.evaluate(&[]).expect("eval");
    let out = fillet.evaluate(&[&tess]).expect("filleted");

    // 15 edges × 2 verts/edge = +30 vertices; 15 edges × 2 tris × 3
    // indices = +90 indices.
    assert_eq!(out.positions.len(), tess.positions.len() + 30);
    assert_eq!(out.indices.len(), tess.indices.len() + 90);
}
