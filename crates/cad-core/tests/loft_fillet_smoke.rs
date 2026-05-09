//! End-to-end smoke for D-Fillet sub-δ — Loft variant of the
//! BRepEdgeId consumer pattern.
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. `FilletOp::new_for_loft` accepts edge IDs that came from the
//!    upstream Loft's `BRepEdgeProvider`.
//! 2. `FilletOp::new_for_loft` rejects synthesised edge IDs whose
//!    bytes don't correspond to any valid Loft edge.
//! 3. **Load-bearing rebuild-stability test (length axis)**:
//!    `fillet_loft_edge_ids_remain_valid_across_length_changes`
//!    captures an edge ID against a unit-square loft at length=1
//!    and proves it is still valid for `FilletOp::new_for_loft`
//!    against rebuilds at length=2 and length=0.5. Length is a
//!    topology-preserving parameter (D-7.2-ζ.δ).
//! 4. **Load-bearing rebuild-stability test (coordinate axis, same N)**:
//!    `fillet_loft_edge_ids_remain_valid_across_coordinate_changes_with_same_n`
//!    swaps coordinates while preserving profile-count. The edge ID
//!    stays valid because the substrate doesn't inspect coordinates
//!    (only profile_count + tag-state per D-7.2-ζ.δ).
//! 5. **Load-bearing topology-change test (profile_count axis)**:
//!    `fillet_loft_edge_ids_invalidated_by_profile_count_change`
//!    proves that the square loft's edge IDs do NOT validate against
//!    a pentagon loft upstream — profile-count changes break edge
//!    identity by construction (D-7.2-ζ.δ).
//! 6. **Load-bearing owner-disjointness test**:
//!    `fillet_loft_distinct_owners_produce_disjoint_specs` mirrors
//!    sub-α/β/γ owner-disjointness pattern — same loft instance under
//!    different owners produces edge IDs that don't cross-validate.
//! 7. The structural delta (vertex / triangle counts added) is
//!    independent of length — same logical edge across length changes
//!    => same delta.
//! 8. End-to-end Loft → Fillet evaluation through `CadGraph`/
//!    `OperatorGraph` produces a well-formed tessellation.

use rge_cad_core::{
    BRepEdgeId, BRepEdgeProvider, BRepOwnerId, CadGraph, FilletError, FilletOp, LoftOp, Operator,
    OperatorNode, Polygon2D, TessellationCache, Tolerance,
};

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("ccw unit square")
}

fn larger_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]).expect("ccw larger square")
}

fn unit_pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("ccw regular pentagon")
}

/// All `3 * N` edge IDs returned by the upstream Loft's
/// `BRepEdgeProvider` are accepted by `FilletOp::new_for_loft`.
#[test]
fn fillet_validates_loft_edge_ids_against_upstream() {
    let owner = BRepOwnerId::from_bytes([0xed; 16]);
    let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
    let edges = loft.brep_edge_ids(owner);
    assert_eq!(edges.len(), 12); // 3 * N=4

    let fillet = FilletOp::new_for_loft(&loft, owner, edges.clone(), 0.1).expect("all valid");
    assert_eq!(fillet.edges().len(), 12);
    assert_eq!(fillet.edges(), &edges[..]);
}

/// A synthesised `BRepEdgeId` whose raw bytes don't correspond to any
/// canonical Loft edge under the supplied owner is rejected with
/// `FilletError::EdgeNotInUpstream`.
#[test]
fn fillet_rejects_unknown_loft_edge_id() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
    let phantom = BRepEdgeId::from_bytes([0u8; 16]);
    let result = FilletOp::new_for_loft(&loft, owner, vec![phantom], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// **Load-bearing rebuild-stability test (length axis).**
///
/// Length is a topology-preserving parameter. Edge IDs must be stable
/// across length rebuilds (D-7.2-ζ.δ proved this for Loft); FilletOp
/// construction must therefore succeed against rebuilt lofts with
/// the same edge ID list.
#[test]
fn fillet_loft_edge_ids_remain_valid_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let p = unit_square();
    let loft_a = LoftOp::new(p.clone(), p.clone(), 1.0).expect("len=1");
    let edge_id = loft_a.brep_edge_ids(owner)[0];

    let loft_b = LoftOp::new(p.clone(), p.clone(), 2.0).expect("len=2");
    let loft_c = LoftOp::new(p.clone(), p.clone(), 0.5).expect("len=0.5");

    assert!(
        loft_b.brep_edge_ids(owner).contains(&edge_id),
        "edge id captured against len=1 must remain in rebuilt len=2 edge list"
    );
    assert!(
        loft_c.brep_edge_ids(owner).contains(&edge_id),
        "edge id captured against len=1 must remain in rebuilt len=0.5 edge list"
    );

    let fa = FilletOp::new_for_loft(&loft_a, owner, vec![edge_id], 0.1).expect("a");
    let fb = FilletOp::new_for_loft(&loft_b, owner, vec![edge_id], 0.1).expect("b");
    let fc = FilletOp::new_for_loft(&loft_c, owner, vec![edge_id], 0.1).expect("c");
    assert_eq!(fa.edges(), fb.edges());
    assert_eq!(fb.edges(), fc.edges());
    // FilletOp's structural hash captures only the operator's own
    // parameters (owner + edges + radius), not the upstream Loft length.
    assert_eq!(
        fa.structural_hash(),
        fb.structural_hash(),
        "FilletOp structural hash must depend only on (owner, edges, radius), not upstream length"
    );
}

/// **Load-bearing rebuild-stability test (coordinate axis, same N).**
///
/// Coordinate-only changes (same profile_count, different XY values)
/// preserve edge IDs because the substrate doesn't inspect coordinates,
/// per the D-7.2-ζ.δ contract.
#[test]
fn fillet_loft_edge_ids_remain_valid_across_coordinate_changes_with_same_n() {
    let owner = BRepOwnerId::from_bytes([0x9a; 16]);
    let small = LoftOp::new(unit_square(), unit_square(), 1.0).expect("small");
    let large = LoftOp::new(larger_square(), larger_square(), 1.0).expect("large");
    let edge_id = small.brep_edge_ids(owner)[0];

    assert!(
        large.brep_edge_ids(owner).contains(&edge_id),
        "edge id from unit-square loft must validate against larger-square loft (same N=4)"
    );

    let fa = FilletOp::new_for_loft(&small, owner, vec![edge_id], 0.1).expect("a");
    let fb = FilletOp::new_for_loft(&large, owner, vec![edge_id], 0.1).expect("b");
    assert_eq!(fa.edges(), fb.edges());
}

/// **Load-bearing topology-change test (profile_count axis).**
///
/// Profile count change breaks edge IDs (per D-7.2-ζ.δ). LoftOp
/// enforces equal profile counts at evaluate time, so changing
/// topology means changing BOTH profiles together
/// (sq×sq → pen×pen). The square's edge IDs must NOT be valid
/// against the pentagon loft.
#[test]
fn fillet_loft_edge_ids_invalidated_by_profile_count_change() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let sq_loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("sq");
    let pen_loft = LoftOp::new(unit_pentagon(), unit_pentagon(), 1.0).expect("pen");

    // A side edge from the square (canonical_index = 2 in 3N=12 range
    // for the 4-vertex profile).
    let sq_edge = sq_loft.brep_edge_ids(owner)[2];
    let pen_edges = pen_loft.brep_edge_ids(owner);

    assert!(
        !pen_edges.contains(&sq_edge),
        "square's bottom-perimeter edge[2] must NOT collide with any pentagon loft edge ID"
    );
    let result = FilletOp::new_for_loft(&pen_loft, owner, vec![sq_edge], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// Filleting "the same logical edge" of a Loft at different lengths
/// produces the same structural delta (vertex/index count change is
/// identical even though absolute positions differ).
#[test]
fn fillet_loft_rebuild_produces_same_structural_delta_across_lengths() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let p = unit_square();
    let loft_a = LoftOp::new(p.clone(), p.clone(), 1.0).expect("a");
    let loft_b = LoftOp::new(p.clone(), p.clone(), 3.0).expect("b");
    let edge_id = loft_a.brep_edge_ids(owner)[0];

    let fa = FilletOp::new_for_loft(&loft_a, owner, vec![edge_id], 0.1).expect("a");
    let fb = FilletOp::new_for_loft(&loft_b, owner, vec![edge_id], 0.1).expect("b");

    let tess_a = loft_a.evaluate(&[]).expect("eval a");
    let tess_b = loft_b.evaluate(&[]).expect("eval b");

    let out_a = fa.evaluate(&[&tess_a]).expect("out a");
    let out_b = fb.evaluate(&[&tess_b]).expect("out b");

    // Same structural delta: each fillet adds 2 vertices and 2
    // triangles (= 6 indices).
    assert_eq!(out_a.positions.len(), tess_a.positions.len() + 2);
    assert_eq!(out_b.positions.len(), tess_b.positions.len() + 2);
    assert_eq!(out_a.indices.len(), tess_a.indices.len() + 6);
    assert_eq!(out_b.indices.len(), tess_b.indices.len() + 6);
}

/// End-to-end Loft → Fillet through `CadGraph`/`OperatorGraph`
/// evaluates and produces a well-formed tessellation.
#[test]
fn fillet_loft_through_operator_graph_evaluates_correctly() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
    let edge_id = loft.brep_edge_ids(owner)[0];
    let fillet = FilletOp::new_for_loft(&loft, owner, vec![edge_id], 0.1).expect("fillet");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let loft_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(loft))
        .expect("loft");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(loft_node, fillet_node, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    cad.commit("loft -> fillet").expect("commit");

    // Evaluate end-to-end. Square loft (sq×sq, len=1): 2N=8 verts,
    // 4N-4=12 triangles (36 indices). After 1 fillet: +2 verts +
    // 6 indices.
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
fn fillet_loft_zero_radius_rejected() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
    let edge = loft.brep_edge_ids(owner)[0];
    let result = FilletOp::new_for_loft(&loft, owner, vec![edge], 0.0);
    assert!(matches!(result, Err(FilletError::InvalidRadius { .. })));
}

/// **Load-bearing owner-disjointness test.**
///
/// Same loft instance under different `BRepOwnerId`s produces edge ID
/// vectors that are owner-disjoint at the substrate level (face IDs
/// hash the owner bytes into their derivation, so any two distinct
/// owners produce two disjoint edge ID sets). FilletOp construction
/// must therefore reject an edge captured against owner_x when the
/// caller passes owner_y. Mirrors sub-α/β/γ owner-disjointness pattern.
#[test]
fn fillet_loft_distinct_owners_produce_disjoint_specs() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");

    let edges_x = loft.brep_edge_ids(owner_x);
    let edges_y = loft.brep_edge_ids(owner_y);
    assert_eq!(edges_x.len(), 12);
    assert_eq!(edges_y.len(), 12);

    // Owner-disjoint at the substrate level: no edge ID from owner_x
    // appears in owner_y's edge list and vice versa.
    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "edge ID under owner_x must NOT appear in owner_y's edge list"
        );
    }

    // The same edge ID against the wrong owner is NOT a valid input.
    let result = FilletOp::new_for_loft(&loft, owner_y, vec![edges_x[0]], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}
