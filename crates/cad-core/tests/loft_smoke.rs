//! End-to-end smoke for the Loft operator through the full cad-core stack.

use rge_cad_core::{CadGraph, LoftOp, OperatorNode, Polygon2D, TessellationCache, Tolerance};

#[test]
fn loft_square_to_square_through_full_pipeline() {
    let profile_a =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("unit square");
    let profile_b = Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]])
        .expect("2-unit square");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let loft_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(
            LoftOp::new(profile_a, profile_b, 3.0).expect("loft"),
        ))
        .expect("add loft");
    cad.graph_mut()
        .expect("mut")
        .set_root(loft_id)
        .expect("set root");
    let _c1 = cad.commit("square-to-square loft").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(loft_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    // n=4 → 2n=8 vertices, 4n-4=12 triangles, 36 indices
    assert_eq!(tess.positions.len(), 8);
    assert_eq!(tess.indices.len(), 36);
    assert_eq!(tess.triangle_count(), 12);

    // Bottom ring at z=0, top ring at z=3.0
    for v in &tess.positions[..4] {
        assert!(
            (v[2] - 0.0).abs() < f32::EPSILON,
            "bottom z must be 0: {v:?}"
        );
    }
    for v in &tess.positions[4..8] {
        assert!(
            (v[2] - 3.0).abs() < f32::EPSILON,
            "top z must be 3.0: {v:?}"
        );
    }
}
