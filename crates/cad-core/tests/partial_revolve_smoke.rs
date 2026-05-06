//! End-to-end smoke for partial `RevolveOp` through the full cad-core stack.

use std::f32::consts::PI;

use rge_cad_core::{
    BooleanMode, BooleanOp, CadGraph, CuboidOp, OperatorNode, Polygon2D, RevolveOp,
    TessellationCache, Tolerance,
};

#[test]
fn partial_revolve_pi_through_full_pipeline() {
    let profile =
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("square");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let revolve_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(
            RevolveOp::partial(profile, 8, PI).expect("revolve"),
        ))
        .expect("add revolve");
    cad.graph_mut()
        .expect("mut")
        .set_root(revolve_id)
        .expect("set root");
    let _c1 = cad.commit("half-revolution square").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(revolve_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    // n=4 × (segments+1) = 4 × 9 = 36 vertices
    // side: 2*4*8 = 64 tris ; caps: 2*(4-2) = 4 tris ; total: 68
    assert_eq!(tess.positions.len(), 36);
    assert_eq!(tess.indices.len(), 204);
    assert_eq!(tess.triangle_count(), 68);

    // Every vertex must satisfy x² + z² ∈ {1, 4}.
    for [x, y, z] in &tess.positions {
        let r2 = x * x + z * z;
        let close_to_1 = (r2 - 1.0).abs() < 1.0e-4;
        let close_to_4 = (r2 - 4.0).abs() < 1.0e-4;
        assert!(close_to_1 || close_to_4, "vertex r² unexpected: {r2}");
        assert!(*y >= 0.0 && *y <= 1.0, "vertex y out of range: {y}");
    }

    // The end ring (ring 8 = `segments`) at θ=π should have z ≈ 0 and x ≤ 0.
    // Ring base index: 8 * 4 = 32. Ring 8 occupies positions[32..36].
    for [x, _y, z] in &tess.positions[32..36] {
        assert!(z.abs() < 1.0e-5, "end-ring z must be ≈ 0 at θ=π: {z}");
        assert!(*x < 0.0, "end-ring x must be negative at θ=π: {x}");
    }
}

#[test]
fn partial_revolve_quarter_pi_with_boolean_pipeline() {
    // Validate that the new partial-revolve output flows through Boolean —
    // construct a partial revolution + a Cuboid + a BooleanOp::union() and
    // assert non-empty + non-panic. (Full geometric correctness is covered
    // by the unit tests; this is a "the pipeline doesn't break" smoke.)
    let profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [1.5, 1.0]]).expect("triangle");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let revolve_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(
            RevolveOp::partial(profile, 6, PI / 2.0).expect("revolve"),
        ))
        .expect("revolve");
    let cube_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 3.0,
            height: 3.0,
            depth: 3.0,
        }))
        .expect("cube");
    let union_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Boolean(BooleanOp::new(BooleanMode::Union)))
        .expect("union");
    cad.graph_mut()
        .expect("mut")
        .connect(revolve_id, union_id, 0)
        .expect("connect lhs");
    cad.graph_mut()
        .expect("mut")
        .connect(cube_id, union_id, 1)
        .expect("connect rhs");
    cad.graph_mut()
        .expect("mut")
        .set_root(union_id)
        .expect("set root");
    cad.commit("partial revolve ∪ cube").expect("commit");

    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(union_id, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");

    assert!(!tess.positions.is_empty(), "union mesh must be non-empty");
    assert_eq!(tess.indices.len() % 3, 0, "indices must be multiple of 3");
}
