//! D-projection-γ end-to-end smoke for cad-projection face-ID integration —
//! mode-driven topology consumer (RevolveOp).
//!
//! Sub-α (D-projection-α) shipped face-ID propagation through projection for
//! Cuboid (fixed topology); sub-β extended to Extrude (variable-N profile).
//! Sub-γ extends to Revolve — the only operator whose **face count and label
//! repetition both change with a categorical mode flip** (Full vs Partial).
//! The API surface (`ProjectedMesh.face_labels`, `BRepHandle.brep_owner`,
//! `CadProjection::brep_face_id_for_triangle`) is byte-identical to sub-α/β;
//! this test suite validates that lazy resolution generalizes to mode-driven
//! topology.
//!
//! These tests prove:
//!
//! 1. Each projected triangle of a Full-mode Revolve resolves to one of the
//!    `n` stable [`BRepFaceId`]s minted by the upstream's [`BRepProvider`].
//! 2. Each projected triangle of a Partial-mode Revolve resolves to one of
//!    the `n + 2` stable face IDs (n side faces + start cap + end cap).
//! 3. **LOAD-BEARING — angle changes within Partial mode preserve face IDs.**
//!    Per D-7.2-γ, angle is NOT in the Side face tag's BLAKE3 input; same
//!    profile + same segments + Partial mode = same face IDs across angle
//!    changes.
//! 4. **LOAD-BEARING — Full ↔ Partial mode change breaks face IDs.** Mode
//!    is in the Side tag, so Full and Partial Side IDs are disjoint.
//! 5. **LOAD-BEARING — segment-count change breaks face IDs.** segment_count
//!    is in the Side tag.
//! 6. Out-of-bounds triangle indices and `None` brep_owner return `None`.
//! 7. Distinct owners produce disjoint face IDs.

use std::f32::consts::PI;

use rge_cad_core::{
    BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, OperatorNode, Polygon2D, RevolveOp, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0xed; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

fn ring_profile() -> Polygon2D {
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ring")
}

fn pentagon_ring() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [2.0, 0.5],
        [2.5, 1.5],
        [1.5, 2.0],
        [1.0, 1.0],
    ])
    .expect("pentagon ring")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Full-mode
/// Revolve committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn so `brep_face_id_for_triangle` resolves against
/// a known owner-seed.
fn build_revolve_full_projection(
    profile: Polygon2D,
    segments: u32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let revolve = RevolveOp::new(profile, segments).expect("revolve full");
    let revolve_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(revolve))
        .expect("add revolve");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(revolve_node)
        .expect("set root");
    graph.commit("test revolve full").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, revolve_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Mirror of [`build_revolve_full_projection`] for Partial-mode Revolve via
/// `RevolveOp::partial`.
fn build_revolve_partial_projection(
    profile: Polygon2D,
    segments: u32,
    angle: f32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let revolve = RevolveOp::partial(profile, segments, angle).expect("revolve partial");
    let revolve_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(revolve))
        .expect("add revolve");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(revolve_node)
        .expect("set root");
    graph.commit("test revolve partial").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, revolve_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Each of the `2 * n * segments = 64` triangles in a Full-mode square Revolve
/// (n=4, segments=8) resolves to one of the `n=4` stable `BRepFaceId`s minted
/// by the upstream's `BRepProvider::brep_face_ids` impl.
#[test]
fn revolve_full_projection_query_returns_brep_face_id_for_each_triangle() {
    let (graph, projection, world, entity) = build_revolve_full_projection(ring_profile(), 8);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // Full mode: 2*n*segments = 2*4*8 = 64 triangles.
    assert_eq!(mesh.triangle_count(), 64);

    let revolve_for_compare = RevolveOp::new(ring_profile(), 8).unwrap();
    let direct_pairs = revolve_for_compare.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 4); // n side faces in Full mode, no caps
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    for tri in 0..64 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| panic!("face id for triangle {tri}"));
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// Partial-mode Revolve (n=4, segments=8, angle=π) projects to
/// `2*n*segments + 2*(n-2) = 64 + 4 = 68` triangles, each resolving to one
/// of the `n + 2 = 6` face IDs (n=4 side faces + StartCap + EndCap).
#[test]
fn revolve_partial_projection_query_returns_face_ids_for_sides_and_caps() {
    let (graph, projection, world, entity) =
        build_revolve_partial_projection(ring_profile(), 8, PI);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // Partial: 2*n*segments + 2*(n-2) = 64 + 4 = 68 triangles.
    assert_eq!(mesh.triangle_count(), 68);

    let revolve_for_compare = RevolveOp::partial(ring_profile(), 8, PI).unwrap();
    let direct_pairs = revolve_for_compare.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 6); // n sides + 2 caps
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    for tri in 0..68 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| panic!("face id for triangle {tri}"));
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// **LOAD-BEARING — angle changes within Partial mode preserve face IDs.**
///
/// Per the D-7.2-γ contract, `RevolveFaceTag::Side`'s BLAKE3 input is
/// `(side_index, profile_count, segment_count, mode)` — angle is NOT in
/// the tag. Same profile + same segments + Partial mode = same face IDs
/// across rebuilds with different angles. This is the cad-projection
/// consumer-pressure test for the topology-preserving rebuild axis of the
/// mode-driven substrate.
#[test]
fn revolve_face_ids_stable_across_angle_changes_within_partial_mode() {
    let (mut graph, mut projection, mut world, entity) =
        build_revolve_partial_projection(ring_profile(), 8, PI / 2.0);
    let initial_ids: Vec<BRepFaceId> = (0..68)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(initial_ids.len(), 68);

    // Rebuild with angle=π (still partial).
    graph.begin_operation().expect("begin");
    let new_revolve = RevolveOp::partial(ring_profile(), 8, PI).unwrap();
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(new_revolve))
        .expect("rebuild");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("root");
    graph.commit("rebuild angle=pi").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let rebuilt_ids: Vec<BRepFaceId> = (0..68)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across angle changes within Partial mode"
    );
}

/// **LOAD-BEARING — Full ↔ Partial mode change breaks face IDs.**
///
/// Per the sub-7.2-γ contract, `mode` is in the Side face tag's BLAKE3
/// input, so Full and Partial Side IDs are disjoint by construction (and
/// the cap IDs only exist in Partial mode). The projection consumer
/// surface preserves this distinction.
#[test]
fn revolve_face_ids_change_when_mode_changes() {
    let (graph_full, projection_full, world_full, entity_full) =
        build_revolve_full_projection(ring_profile(), 8);
    let full_ids: Vec<BRepFaceId> = (0..64)
        .filter_map(|t| {
            projection_full.brep_face_id_for_triangle(
                entity_full,
                t,
                &world_full,
                graph_full.graph(),
            )
        })
        .collect();

    let (graph_partial, projection_partial, world_partial, entity_partial) =
        build_revolve_partial_projection(ring_profile(), 8, PI);
    let partial_ids: Vec<BRepFaceId> = (0..68)
        .filter_map(|t| {
            projection_partial.brep_face_id_for_triangle(
                entity_partial,
                t,
                &world_partial,
                graph_partial.graph(),
            )
        })
        .collect();

    assert_eq!(full_ids.len(), 64);
    assert_eq!(partial_ids.len(), 68);

    // Sub-7.2-γ contract: mode is in the Side tag's BLAKE3 input, so Full
    // and Partial Side IDs are disjoint.
    let full_unique: std::collections::HashSet<_> = full_ids.iter().collect();
    let partial_unique: std::collections::HashSet<_> = partial_ids.iter().collect();
    for id in full_unique.iter() {
        assert!(
            !partial_unique.contains(id),
            "full-mode face ID leaked into partial-mode space: {id:?}"
        );
    }
}

/// **LOAD-BEARING — segment-count change breaks face IDs.**
///
/// Per the sub-7.2-γ contract, `segment_count` is in the Side face tag's
/// BLAKE3 input, so Side IDs at segments=8 are disjoint from those at
/// segments=16 even with identical profile + mode.
#[test]
fn revolve_face_ids_change_when_segment_count_changes() {
    let (graph_8, projection_8, world_8, entity_8) =
        build_revolve_full_projection(ring_profile(), 8);
    let ids_8: Vec<BRepFaceId> = (0..64)
        .filter_map(|t| {
            projection_8.brep_face_id_for_triangle(entity_8, t, &world_8, graph_8.graph())
        })
        .collect();

    let (graph_16, projection_16, world_16, entity_16) =
        build_revolve_full_projection(ring_profile(), 16);
    let ids_16: Vec<BRepFaceId> = (0..128) // 2*4*16 = 128 tris
        .filter_map(|t| {
            projection_16.brep_face_id_for_triangle(entity_16, t, &world_16, graph_16.graph())
        })
        .collect();

    assert_eq!(ids_8.len(), 64);
    assert_eq!(ids_16.len(), 128);

    let ids_8_unique: std::collections::HashSet<_> = ids_8.iter().collect();
    for id in &ids_16 {
        assert!(
            !ids_8_unique.contains(id),
            "segments=16 face ID leaked into segments=8 space"
        );
    }
}

/// An out-of-bounds triangle index returns `None` rather than panicking.
#[test]
fn revolve_query_returns_none_for_out_of_bounds_triangle() {
    let (graph, projection, world, entity) = build_revolve_full_projection(ring_profile(), 8);
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 9999, &world, graph.graph()),
        None
    );
}

/// An entity whose `BRepHandle.brep_owner` is `None` (the legacy default)
/// returns `None` even when the projected mesh has `face_labels`.
#[test]
fn revolve_query_returns_none_when_brep_owner_is_none() {
    let (graph, projection, mut world, entity) = build_revolve_full_projection(ring_profile(), 8);
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 0, &world, graph.graph()),
        None
    );
}

/// Distinct `BRepOwnerId` seeds produce disjoint `BRepFaceId` spaces even
/// when the geometry is byte-identical. Owner-seeded identity is preserved
/// through the projection consumer surface for mode-driven topology.
#[test]
fn distinct_owners_produce_disjoint_face_ids_through_revolve_projection() {
    let owner_y = BRepOwnerId::from_bytes([0xab; 16]);
    assert_ne!(TEST_OWNER, owner_y, "owners must be distinct for this test");

    let (graph, projection, mut world, entity) =
        build_revolve_partial_projection(pentagon_ring(), 8, PI / 2.0);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // n=5 partial: 2*5*8 + 2*3 = 80 + 6 = 86 triangles.
    let tri_count = mesh.triangle_count();

    let ids_x: Vec<BRepFaceId> = (0..tri_count)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner_y);
        }
    }
    let ids_y: Vec<BRepFaceId> = (0..tri_count)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(ids_x.len(), tri_count);
    assert_eq!(ids_y.len(), tri_count);

    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-x face id leaked into owner-y space"
        );
    }
}
