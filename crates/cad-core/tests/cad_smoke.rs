//! End-to-end smoke test for cad-core MVP.
//!
//! Exercises the full Phase 7.1 D-prime path: `begin_operation` → build a
//! Cuboid → Transform chain → commit → evaluate → rollback → `restore_to`.

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance, TransformOp};

#[test]
fn round_trip_cuboid_through_transform_with_checkpoint() {
    let mut cad = CadGraph::new();

    // Begin an operation, build Cuboid → Transform, commit.
    cad.begin_operation().expect("begin");
    let cuboid_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid");
    let transform_id = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [10.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("add transform");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_id, transform_id, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(transform_id)
        .expect("set root");
    let c1 = cad.commit("first cuboid+transform").expect("commit");

    // Evaluate.
    let mut cache = rge_cad_core::TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(
            transform_id,
            &mut cache,
            Tolerance::new(0.001).expect("tol"),
        )
        .expect("evaluate");
    assert_eq!(tess.positions.len(), 8);
    assert_eq!(tess.indices.len(), 36);

    // Verify all vertices have x >= 9.0 (cuboid half-width is 1.0, so the
    // pre-translation min x is -1.0; +10 translation puts the min at +9.0).
    for [x, _y, _z] in &tess.positions {
        assert!(*x >= 9.0 - 1e-6, "vertex x out of expected range: {x}");
    }

    // Begin a second operation, abort it via rollback, then restore_to C1.
    cad.begin_operation().expect("begin 2");
    cad.rollback().expect("rollback");
    cad.restore_to(c1).expect("restore");
    assert_eq!(cad.head(), c1);
}
