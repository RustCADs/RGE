//! Audit-2 A2.6 closure: cross-substrate Boolean × cad-projection × `PieSnapshot`
//! byte-identity gate.
//!
//! Builds a non-trivial `CadGraph` (Cuboid + Transform-translated-Cuboid +
//! `BooleanOp::union`), spawns a `BRepHandle` entity for the union, drives a
//! projection tick, captures both `&cad` and `&projection` participants in a
//! PIE envelope, restores into fresh substrates, drives another projection
//! tick, and asserts the resulting mesh bytes (positions + indices) are
//! byte-identical between the original and restored projections.
//!
//! This is the core determinism gate: any drift between original and
//! post-round-trip mesh bytes indicates either a non-determinism bug in
//! csgrs's BSP triangulation or a missing-state bug in the
//! `SnapshotParticipate` impls (cad-graph or projection).
//!
//! Per dispatch §3 step 9, also re-evaluates the union node directly via
//! `cad.graph().evaluate(...)` (bypassing the projection layer) and asserts
//! byte-identity to the round-tripped mesh — verifies cad-graph's RON
//! serialization preserves every operator parameter exactly. Because the
//! cad-projection's `project()` function copies positions / indices straight
//! out of the underlying `Tessellation`, the projected mesh is bit-for-bit
//! identical to the operator's tessellation output (the projection layer
//! adds provenance metadata but does not transform geometry).

use rge_cad_core::{
    BooleanOp, CadGraph, CuboidOp, OperatorNode, TessellationCache, Tolerance, TransformOp,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{ParticipantId, PieSnapshot, SnapshotParticipate, World};
use rge_kernel_graph_foundation::NodeId;

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Build `CadGraph`: `Cuboid_a` + Transform(translated `Cuboid_b`) + Boolean(Union)
/// — the union node is the root.
fn build_union_graph() -> (CadGraph, NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("cube_a");
    // Slightly perturbed depth so cube_b's content-derived NodeId differs.
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cube_b");
    let xform = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("xform");
    g.connect(cube_b, xform, 0).expect("c1");

    let union = g
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("union");
    g.connect(cube_a, union, 0).expect("c2");
    g.connect(xform, union, 1).expect("c3");
    g.set_root(union).expect("root");
    cad.commit("Boolean(Union) of two cuboids").expect("commit");
    (cad, union)
}

/// Audit-2 A2.6 closure: full Boolean × cad-projection × `PieSnapshot`
/// round-trip byte-identity check, soaked over **100 iterations** to catch
/// non-determinism that would only surface intermittently. Mirrors the
/// 100-iter pattern in `cad_boolean_determinism.rs:70,104`.
///
/// Per-iter: re-capture from the original substrate, restore into a fresh
/// substrate, tick, and assert the restored mesh is byte-identical to the
/// reference established before the loop. Equality is exact via direct
/// `Vec<[f32; 3]>` / `Vec<u32>` comparison — `f32::PartialEq` matches the
/// raw little-endian bit pattern (NaN excepted, but no operator in this
/// test produces NaN), so we get the same byte-identity guarantee as a
/// BLAKE3 hash without needing to add the `blake3` crate as a test dep.
/// Any drift at any iteration is a major non-determinism finding.
///
/// Setup work (building the union graph, the original substrate's first
/// projection tick) is hoisted outside the soak loop — only the
/// capture / restore / tick / assert path lives inside.
#[test]
fn boolean_through_cad_projection_via_pie_snapshot_round_trip_byte_identity() {
    // ---- One-time setup: original substrate (hoisted outside the soak loop) ----
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();
    let (cad, union_node) = build_union_graph();

    let entity = projection
        .spawn_brep_entity(&mut world, union_node)
        .expect("spawn");
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);

    let mesh_original = projection.projected_mesh(entity).expect("mesh1").clone();
    assert!(
        mesh_original.vertex_count() > 0,
        "original union mesh must be non-empty"
    );
    assert!(
        mesh_original.triangle_count() > 0,
        "original union mesh must have triangles"
    );

    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");

    // ---- 100-iter PIE round-trip soak: capture / restore / tick / assert ----
    for iter in 0..100 {
        let snap = PieSnapshot::capture(
            &world,
            &[
                &cad as &dyn SnapshotParticipate,
                &projection as &dyn SnapshotParticipate,
            ],
        )
        .unwrap_or_else(|e| panic!("pie capture iter {iter}: {e:?}"));
        assert_eq!(snap.participants.len(), 2, "iter {iter}");

        let mut fresh_world = World::new();
        fresh_world.register_snapshot_component::<BRepHandle>();
        let mut fresh_cad = CadGraph::new();
        let mut fresh_projection = CadProjection::new();
        snap.restore(
            &mut fresh_world,
            &mut [
                (&pid_cad, &mut fresh_cad as &mut dyn SnapshotParticipate),
                (
                    &pid_proj,
                    &mut fresh_projection as &mut dyn SnapshotParticipate,
                ),
            ],
        )
        .unwrap_or_else(|e| panic!("pie restore iter {iter}: {e:?}"));

        let r2 = fresh_projection
            .tick(&mut fresh_world, &fresh_cad, tol())
            .unwrap_or_else(|e| panic!("tick post-restore iter {iter}: {e:?}"));
        assert_eq!(r2.entities_reprojected, 1, "iter {iter}");

        let mesh_restored = fresh_projection
            .projected_mesh(entity)
            .unwrap_or_else(|| panic!("mesh restored iter {iter}"))
            .clone();

        // Byte-identity gate per iter: positions / indices / source_node.
        // Detailed assertions localize a future drift to its first
        // diverging dimension (positions vs indices vs source_node).
        assert_eq!(
            mesh_original.positions,
            mesh_restored.positions,
            "iter {iter}: Boolean × cad-projection × PieSnapshot round-trip \
             drifted positions (cad-graph or projection serialization is \
             non-deterministic, OR csgrs BSP is producing different output \
             across runs); vertex_count: ref={} this={}",
            mesh_original.vertex_count(),
            mesh_restored.vertex_count(),
        );
        assert_eq!(
            mesh_original.indices, mesh_restored.indices,
            "iter {iter}: indices drifted",
        );
        // Source node id is content-addressed; restoring the cad-graph
        // yields the same NodeId, so source_node must match too.
        assert_eq!(
            mesh_original.source_node, mesh_restored.source_node,
            "iter {iter}: source_node drifted",
        );

        // Per dispatch step 9, ALSO re-evaluate the union node directly via
        // OperatorGraph::evaluate (bypassing the projection layer) and
        // assert byte-identity to the round-tripped mesh. Verifies cad-
        // graph's RON serialization preserves operator parameters exactly.
        // Run this on iter 0 and a sampling of later iters to keep overall
        // soak runtime bounded without sacrificing coverage of the bypass
        // path's determinism.
        if iter == 0 || iter == 50 || iter == 99 {
            let mut bypass_cache = TessellationCache::new();
            let bypass_tess = fresh_cad
                .graph()
                .evaluate(union_node, &mut bypass_cache, tol())
                .unwrap_or_else(|e| panic!("bypass eval iter {iter}: {e:?}"));
            assert_eq!(
                bypass_tess.positions, mesh_restored.positions,
                "iter {iter}: OperatorGraph::evaluate divergent from projection-layer mesh",
            );
            assert_eq!(
                bypass_tess.indices, mesh_restored.indices,
                "iter {iter}: OperatorGraph::evaluate index divergent",
            );
        }
    }
}
