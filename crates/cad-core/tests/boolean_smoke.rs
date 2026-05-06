//! End-to-end integration smokes for the Boolean operator through the full
//! cad-core stack: `CadGraph` + `OperatorGraph` + transactional checkpoints +
//! `TessellationCache` + recursive `effective_hash` propagation through a
//! 2-arity operator.
//!
//! Per ADR-112 §"Implementation guidance" — Boolean is the first cad-core
//! operator that consumes two upstream tessellations from inside the graph,
//! so these smokes specifically exercise the multi-input port wiring path
//! (`connect(.., .., 0)` for lhs / `connect(.., .., 1)` for rhs).

use rge_cad_core::{
    BooleanMode, BooleanOp, CadGraph, CuboidOp, ExtrudeOp, OperatorNode, Polygon2D,
    TessellationCache, Tolerance, TransformOp,
};

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// End-to-end pipeline: two cubes (one translated by +0.5 each axis) through
/// a `BooleanOp::Union` consuming both. Asserts non-empty output that has
/// strictly more vertices than a single 8-vertex cube (because the union of
/// overlapping cubes introduces clip vertices on the seam between them).
///
/// Note: `OperatorGraph` derives `NodeId` from operator content (BLAKE3 over
/// the serialized payload), so two identical-payload operators dedupe via
/// `DuplicateNode`. We use slightly different cube dimensions for the lhs
/// and rhs so they get distinct ids; geometrically they're still cubes that
/// overlap by ~half a unit per axis when one is translated.
#[test]
fn boolean_through_pipeline_union() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");

    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default())) // 1x1x1
        .expect("cube a");
    // The second cube is dimensionally distinct so its NodeId differs from
    // cube_a's, then translated via TransformOp into the overlap zone.
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001, // tiny perturbation just for distinct NodeId
        }))
        .expect("cube b raw");
    let translate = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            ..TransformOp::default()
        }))
        .expect("translate");
    g.connect(cube_b, translate, 0).expect("cube_b → translate");

    let boolean = g
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("union op");
    g.connect(cube_a, boolean, 0).expect("cube_a → bool port 0");
    g.connect(translate, boolean, 1)
        .expect("translate → bool port 1");
    g.set_root(boolean).expect("set root");
    cad.commit("union-of-two-cubes").expect("commit");

    let mut cache = TessellationCache::new();
    let mesh = cad
        .graph()
        .evaluate(boolean, &mut cache, tol())
        .expect("eval union");

    assert!(
        mesh.vertex_count() > 8,
        "union of two overlapping cubes should have > 8 vertices, got {}",
        mesh.vertex_count()
    );
    assert!(mesh.triangle_count() > 0, "union must have triangles");

    // Bounding box must span the union of both cubes:
    // cube_a: [-0.5, 0.5] in each axis; translated cube_b: [0, 1] in each.
    // Union extents: [-0.5, 1.0] in each axis.
    let xs: Vec<f32> = mesh.positions.iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = mesh.positions.iter().map(|p| p[1]).collect();
    let zs: Vec<f32> = mesh.positions.iter().map(|p| p[2]).collect();
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_z = zs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_z = zs.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    assert!(min_x <= -0.49, "lhs corner missing on -X: {min_x}");
    assert!(max_x >= 0.99, "rhs corner missing on +X: {max_x}");
    assert!(min_y <= -0.49, "lhs corner missing on -Y: {min_y}");
    assert!(max_y >= 0.99, "rhs corner missing on +Y: {max_y}");
    assert!(min_z <= -0.49, "lhs corner missing on -Z: {min_z}");
    assert!(max_z >= 0.99, "rhs corner missing on +Z: {max_z}");
}

/// Same pipeline as the union smoke but with `BooleanMode::Difference`.
/// Asserts the result differs from the union case (different vertex count
/// or different geometry) — proves the mode wiring + dispatch propagation
/// works end-to-end.
#[test]
fn boolean_through_pipeline_difference() {
    let build_pipeline = |mode: BooleanMode| {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");

        let g = cad.graph_mut().expect("mut");
        let cube_a = g
            .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
            .expect("cube a");
        let cube_b = g
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0,
                height: 1.0,
                depth: 1.0001, // tiny perturbation so NodeId differs from cube_a
            }))
            .expect("cube b raw");
        let translate = g
            .add_operator(OperatorNode::Transform(TransformOp {
                translation: [0.5, 0.5, 0.5],
                ..TransformOp::default()
            }))
            .expect("translate");
        g.connect(cube_b, translate, 0).expect("cube_b → translate");

        let boolean = g
            .add_operator(OperatorNode::Boolean(BooleanOp::new(mode)))
            .expect("bool op");
        g.connect(cube_a, boolean, 0).expect("cube_a → bool port 0");
        g.connect(translate, boolean, 1)
            .expect("translate → bool port 1");
        g.set_root(boolean).expect("set root");
        cad.commit(format!("boolean-{mode:?}")).expect("commit");

        let mut cache = TessellationCache::new();
        let mesh = cad
            .graph()
            .evaluate(boolean, &mut cache, tol())
            .expect("eval");
        // Return owned tessellation for comparison.
        rge_cad_core::Tessellation::new(mesh.positions.clone(), mesh.indices.clone())
            .expect("clone")
    };

    let union_mesh = build_pipeline(BooleanMode::Union);
    let diff_mesh = build_pipeline(BooleanMode::Difference);

    // Difference output must be non-empty.
    assert!(diff_mesh.vertex_count() > 0);
    assert!(diff_mesh.triangle_count() > 0);

    // Difference and Union must produce different output (different vertex
    // count OR different positions). Same input cubes + different mode →
    // different effective_hash → different output.
    let same_count = union_mesh.vertex_count() == diff_mesh.vertex_count();
    let same_positions = union_mesh.positions == diff_mesh.positions;
    assert!(
        !same_count || !same_positions,
        "Union and Difference must produce distinct outputs: union={} verts, diff={} verts",
        union_mesh.vertex_count(),
        diff_mesh.vertex_count()
    );

    // Difference output: lhs (origin cube, [-0.5, 0.5]) minus rhs (translated
    // cube, [0, 1.0]). The carved-out region is `[0, 0.5]³`. So no vertex of
    // the difference output should lie strictly inside the carve (boundary
    // vertices are fine — those are the cut planes).
    for [x, y, z] in &diff_mesh.positions {
        let in_dent_strict = *x > 0.0 + 0.01
            && *x < 0.5 - 0.01
            && *y > 0.0 + 0.01
            && *y < 0.5 - 0.01
            && *z > 0.0 + 0.01
            && *z < 0.5 - 0.01;
        assert!(
            !in_dent_strict,
            "difference output has vertex strictly inside the carved-out region: ({x},{y},{z})"
        );
    }
}

/// Heterogeneous-input pipeline: Extrude(triangle profile) as lhs, Cuboid as
/// rhs of a `BooleanOp::Union`. Verifies the operator graph correctly routes
/// distinct upstream operator types through Boolean's two ports.
#[test]
fn boolean_with_extrude_input() {
    // Triangle profile in XY plane (CCW): (0,0), (2,0), (1, 1.5). Extrude
    // by 1 unit produces a 6-vertex / 8-triangle prism.
    let triangle =
        Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [1.0, 1.5]]).expect("triangle profile");
    let extrude_op = ExtrudeOp::new(triangle, 1.0).expect("extrude op");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");

    let g = cad.graph_mut().expect("mut");
    let extrude = g
        .add_operator(OperatorNode::Extrude(extrude_op))
        .expect("extrude");
    let cube = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("cube");
    let translate = g
        .add_operator(OperatorNode::Transform(TransformOp {
            // Position the cube to overlap the prism at (1, 0.5, 0.5).
            translation: [1.0, 0.5, 0.5],
            ..TransformOp::default()
        }))
        .expect("translate");
    g.connect(cube, translate, 0).expect("cube → translate");

    let boolean = g
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("union");
    g.connect(extrude, boolean, 0)
        .expect("extrude → bool port 0");
    g.connect(translate, boolean, 1)
        .expect("translate → bool port 1");
    g.set_root(boolean).expect("set root");
    cad.commit("extrude-union-cube").expect("commit");

    let mut cache = TessellationCache::new();
    let mesh = cad
        .graph()
        .evaluate(boolean, &mut cache, tol())
        .expect("evaluate");

    // Output non-empty.
    assert!(mesh.vertex_count() > 0, "heterogeneous union empty");
    assert!(mesh.triangle_count() > 0, "no triangles produced");

    // Output must be reasonable: no NaN / inf / wildly out-of-bounds verts.
    for [x, y, z] in &mesh.positions {
        assert!(
            x.is_finite() && y.is_finite() && z.is_finite(),
            "non-finite vertex {x},{y},{z}"
        );
        assert!(*x >= -0.001 && *x <= 2.501, "x out of plausible range: {x}");
        assert!(*y >= -0.001 && *y <= 1.501, "y out of plausible range: {y}");
        assert!(*z >= -0.001 && *z <= 1.501, "z out of plausible range: {z}");
    }
}
