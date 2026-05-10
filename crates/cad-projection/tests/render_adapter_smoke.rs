//! Render-backed face-selection sub-γ end-to-end smoke for
//! [`CadProjection::render_mesh_for`].
//!
//! These integration tests exercise the **adapter chain** that turns a
//! `cad-projection`-owned `ProjectedMesh` (cad-core-derived data with
//! `TopologyFaceId` face labels) into a `brep-render::RenderMesh`
//! (renderer-domain flat-shaded mesh with opaque `u64` face labels), and
//! verifies:
//!
//! * The triangle-count contract per D-projection-α (Cuboid) and
//!   D-projection-β (Extrude).
//! * **The chain-consistency invariant**: the renderer-side opaque
//!   `u64` label resolves to the SAME identity as the picker-side
//!   `BRepFaceId`. Without this, sub-α's opaque-buffer deviation could
//!   silently desync the two paths.

use rge_cad_core::{
    brep_face_ids_for_node, BRepFaceId, BRepOwnerId, CadGraph, CuboidOp, ExtrudeOp, OperatorNode,
    Polygon2D, Tolerance, TopologyFaceId,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::World;

const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Cuboid
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// `ENTITY_OWNER` post-spawn.
fn build_cuboid(
    width: f32,
    height: f32,
    depth: f32,
) -> (
    CadGraph,
    CadProjection,
    World,
    rge_kernel_ecs::EntityId,
    rge_kernel_graph_foundation::NodeId,
) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width,
            height,
            depth,
        }))
        .expect("add cuboid");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(cuboid_node)
        .expect("set root");
    graph.commit("cuboid").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity, cuboid_node)
}

/// **Test 1 — LOAD-BEARING: chain consistency between renderer-side opaque
/// `u64` labels and picker-side `BRepFaceId` resolution.**
///
/// Build a 1×1×1 Cuboid entity. For each of the 12 triangles, demonstrate
/// two parallel resolution paths converge to the same `BRepFaceId`:
///
/// 1. **Renderer path** — `render_mesh_for` returns `RenderMesh` with
///    opaque `u64` face labels; wrap each `u64` back into
///    `TopologyFaceId(label)` and resolve via the same
///    `brep_face_ids_for_node` machinery the picker uses internally.
/// 2. **Picker path** — `brep_face_id_for_triangle` returns the same
///    `BRepFaceId` directly.
///
/// Without this invariant, sub-α's opaque-buffer deviation
/// (`u64` instead of `TopologyFaceId` for renderer-tier consumption)
/// could silently desync the two paths. This test is the wire-format
/// proof that the round-trip works.
#[test]
fn render_mesh_face_labels_resolve_consistently_with_picker() {
    let (graph, projection, world, entity, source_node) = build_cuboid(1.0, 1.0, 1.0);

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for valid Cuboid entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("Cuboid is labeled per D-projection-α");
    assert_eq!(
        render_labels.len(),
        12,
        "Cuboid → 12 triangles → 12 face_labels"
    );

    // Build the resolver pair-list ONCE (matches the picker's internal
    // `brep_face_id_for_triangle` lookup pattern, but exposed inline here
    // so the test demonstrates the chain explicitly).
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), source_node, ENTITY_OWNER)
            .expect("Cuboid resolver must succeed");

    for tri_idx in 0..12 {
        // Renderer-side path: opaque u64 → TopologyFaceId → BRepFaceId.
        let render_label_u64 = render_labels[tri_idx];
        let topology_id = TopologyFaceId(render_label_u64);
        let render_resolved: BRepFaceId = pairs
            .iter()
            .find(|(t, _)| *t == topology_id)
            .map(|(_, brep_id)| *brep_id)
            .expect("renderer-side label must resolve to a BRepFaceId");

        // Picker-side path: existing CadProjection::brep_face_id_for_triangle.
        let picker_resolved: BRepFaceId = projection
            .brep_face_id_for_triangle(entity, tri_idx, &world, graph.graph())
            .expect("picker-side resolution must succeed for Cuboid triangle");

        assert_eq!(
            render_resolved, picker_resolved,
            "renderer-side BRepFaceId resolution (via u64 → TopologyFaceId → resolver) MUST \
             match picker-side BRepFaceId resolution at triangle {tri_idx}; otherwise the \
             opaque-u64 wire format silently desyncs the two paths"
        );
    }
}

/// **Test 2** — 1×1×1 Cuboid → RenderMesh with `positions.len() == 36`,
/// `normals.len() == 36`, `indices.len() == 36`, `face_labels.len() == 12`
/// (per D-projection-α contract: 12 triangles × 3 vertices = 36 due to
/// vertex tripling).
#[test]
fn cuboid_render_mesh_triangle_count_matches_d_projection_alpha_contract() {
    let (_graph, projection, world, entity, _node) = build_cuboid(1.0, 1.0, 1.0);

    let mesh = projection
        .render_mesh_for(entity, &world)
        .expect("must render");
    assert_eq!(
        mesh.positions.len(),
        36,
        "Cuboid: 12 triangles × 3 vertex-tripling = 36 positions"
    );
    assert_eq!(mesh.normals.len(), 36);
    assert_eq!(mesh.indices.len(), 36);
    let labels = mesh.face_labels.as_ref().expect("Cuboid is labeled");
    assert_eq!(
        labels.len(),
        12,
        "Cuboid: 12 input triangles → 12 face_labels"
    );
    // Sanity check: indices are dense [0..36].
    for (i, idx) in mesh.indices.iter().enumerate() {
        assert_eq!(*idx as usize, i, "indices must be dense [0, 1, ..., 35]");
    }
}

/// **Test 3** — square ExtrudeOp (n=4) → 4n-4 = 12 triangles → 36
/// positions, 36 normals, 36 indices, 12 face_labels (per D-projection-β
/// contract).
#[test]
fn extrude_square_render_mesh_triangle_count_matches_d_projection_beta_contract() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // 4-vertex square profile (CCW in the XY plane).
    let profile =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square");
    let extrude = ExtrudeOp::new(profile, 1.0).expect("extrude construction");
    let extrude_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("add extrude");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(extrude_node)
        .expect("set root");
    graph.commit("extrude square").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, extrude_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let mesh = projection
        .render_mesh_for(entity, &world)
        .expect("must render Extrude entity");
    // 4n-4 = 12 triangles for n=4: bottom (n-2=2) + top (n-2=2) + sides
    // (2*n=8) = 12 → 36 positions / normals / indices.
    assert_eq!(
        mesh.positions.len(),
        36,
        "Extrude n=4: 12 triangles × 3 = 36 positions per D-projection-β"
    );
    assert_eq!(mesh.normals.len(), 36);
    assert_eq!(mesh.indices.len(), 36);
    let labels = mesh.face_labels.as_ref().expect("Extrude is labeled");
    assert_eq!(
        labels.len(),
        12,
        "Extrude n=4: 12 input triangles → 12 face_labels per D-projection-β"
    );
}
