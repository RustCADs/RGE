//! End-to-end smoke for the Revolve operator through the full cad-core stack.

use rge_cad_core::{CadGraph, OperatorNode, Polygon2D, RevolveOp, TessellationCache, Tolerance};

#[test]
fn revolve_square_profile_through_full_pipeline() {
    // Square profile on +X side of Y-axis: x in [1, 2], y in [0, 1].
    // 8 segments → 32 vertices, 64 triangles, 192 indices.
    let profile =
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("square");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let revolve_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(
            RevolveOp::new(profile, 8).expect("revolve"),
        ))
        .expect("add revolve");
    cad.graph_mut()
        .expect("mut")
        .set_root(revolve_id)
        .expect("set root");
    let _c1 = cad.commit("square torus 8-seg").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(revolve_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    // n=4 profile points × 8 segments = 32 vertices
    // 2 × 4 × 8 = 64 triangles → 192 indices
    assert_eq!(tess.positions.len(), 32);
    assert_eq!(tess.indices.len(), 192);
    assert_eq!(tess.triangle_count(), 64);

    // Every output vertex must satisfy x²+z² in {1, 4} (the inner and outer
    // radii of the square cross-section), to within tolerance, and y in [0,1].
    for [x, y, z] in &tess.positions {
        let r2 = x * x + z * z;
        let close_to_1 = (r2 - 1.0).abs() < 1.0e-4;
        let close_to_4 = (r2 - 4.0).abs() < 1.0e-4;
        assert!(close_to_1 || close_to_4, "vertex r² unexpected: {r2}");
        assert!(*y >= 0.0 && *y <= 1.0, "vertex y out of range: {y}");
    }
}
