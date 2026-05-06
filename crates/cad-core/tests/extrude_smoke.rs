//! End-to-end smoke for the Extrude operator through the full cad-core stack.

use rge_cad_core::{CadGraph, ExtrudeOp, OperatorNode, Polygon2D, TessellationCache, Tolerance};

#[test]
fn extrude_pentagon_through_full_pipeline() {
    let profile = Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("regular pentagon");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let extrude_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(
            ExtrudeOp::new(profile, 2.5).expect("extrude"),
        ))
        .expect("add extrude");
    cad.graph_mut()
        .expect("mut")
        .set_root(extrude_id)
        .expect("set root");
    let _c1 = cad.commit("pentagon prism").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(extrude_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    // n=5 → 2n=10 vertices, 4n-4=16 triangles, 48 indices
    assert_eq!(tess.positions.len(), 10);
    assert_eq!(tess.indices.len(), 48);
    assert_eq!(tess.triangle_count(), 16);

    // Bottom ring at z=0, top ring at z=2.5
    for v in &tess.positions[..5] {
        assert!(
            (v[2] - 0.0).abs() < f32::EPSILON,
            "bottom z must be 0: {v:?}"
        );
    }
    for v in &tess.positions[5..10] {
        assert!(
            (v[2] - 2.5).abs() < f32::EPSILON,
            "top z must be 2.5: {v:?}"
        );
    }
}
