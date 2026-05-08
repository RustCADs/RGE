//! End-to-end smoke for the Sweep operator through the full cad-core stack.

use rge_cad_core::{
    CadGraph, OperatorNode, Polygon2D, Polyline3D, SweepOp, TessellationCache, Tolerance,
};

#[test]
fn sweep_square_along_3_point_z_path_through_full_pipeline() {
    // Square profile + 3-point Z path 0 → 1.5 → 3.0.
    // Expected: n=4, m=3 → 12 vertices, 2*4*2 + 2*2 = 20 triangles, 60 indices.
    let profile = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("unit square profile");
    let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.5], [0.0, 0.0, 3.0]])
        .expect("z-axis path");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let sweep_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Sweep(SweepOp::new(profile, path)))
        .expect("add sweep");
    cad.graph_mut()
        .expect("mut")
        .set_root(sweep_id)
        .expect("set root");
    let _c1 = cad.commit("square-along-3-point-z sweep").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(sweep_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    assert_eq!(tess.positions.len(), 12);
    assert_eq!(tess.indices.len(), 60);
    assert_eq!(tess.triangle_count(), 20);

    // First ring (0..4) at z=0, middle ring (4..8) at z=1.5, last ring
    // (8..12) at z=3.0.
    for v in &tess.positions[..4] {
        assert!((v[2] - 0.0).abs() < f32::EPSILON, "first ring z=0: {v:?}");
    }
    for v in &tess.positions[4..8] {
        assert!(
            (v[2] - 1.5).abs() < f32::EPSILON,
            "middle ring z=1.5: {v:?}"
        );
    }
    for v in &tess.positions[8..12] {
        assert!((v[2] - 3.0).abs() < f32::EPSILON, "last ring z=3.0: {v:?}");
    }
}
