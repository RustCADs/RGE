//! D-projection-δ end-to-end smoke for cad-projection face-ID integration —
//! two-profile topology consumer (LoftOp).
//!
//! Sub-α (D-projection-α) shipped face-ID propagation through projection for
//! Cuboid (fixed topology); sub-β extended to Extrude (variable-N profile);
//! sub-γ extended to Revolve (mode-driven topology). Sub-δ extends to Loft —
//! the two-profile variant whose face structure is structurally identical to
//! Extrude's (Bottom + Top + N Sides) but whose label/identity is provenance-
//! distinct (LoftFaceTag, not ExtrudeFaceTag). The API surface
//! (`ProjectedMesh.face_labels`, `BRepHandle.brep_owner`,
//! `CadProjection::brep_face_id_for_triangle`) is byte-identical to sub-α/β/γ.
//!
//! These tests prove:
//!
//! 1. Each projected triangle of a square×square Loft (n=4) resolves to one of
//!    the `n + 2 = 6` stable [`BRepFaceId`]s minted by the upstream's
//!    [`BRepProvider`].
//! 2. The mapping follows the canonical face-emission order (Bottom 0-1, Top
//!    2-3, Sides 4-11), structurally identical to ExtrudeOp's mapping.
//! 3. **LOAD-BEARING — rebuild stability across length changes.** Length is
//!    topology-preserving per D-7.2-δ. Same face IDs across rebuilds.
//! 4. **LOAD-BEARING — rebuild stability across coordinate changes (same N).**
//!    Same vertex count + same vertex order = same topology even with
//!    different coordinates.
//! 5. **LOAD-BEARING — profile-count change breaks Side IDs.** LoftOp enforces
//!    equal profile counts so changing topology means changing BOTH
//!    (sq×sq → pen×pen). Categorical Bottom/Top match across; Side IDs
//!    disjoint per LoftFaceTag's design from sub-7.2-δ.
//! 6. Out-of-bounds triangle indices and `None` brep_owner return `None`.
//! 7. Distinct owners produce disjoint face IDs.

use rge_cad_core::{
    BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, LoftOp, OperatorNode, Polygon2D, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0xed; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

fn larger_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]).expect("larger square")
}

fn pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("pentagon")
}

fn larger_pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [2.0, 0.0],
        [0.618, 1.902],
        [-1.618, 1.176],
        [-1.618, -1.176],
        [0.618, -1.902],
    ])
    .expect("scaled pentagon")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Loft
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn so `brep_face_id_for_triangle` resolves against
/// a known owner-seed.
fn build_loft_projection(
    profile_a: Polygon2D,
    profile_b: Polygon2D,
    length: f32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let loft = LoftOp::new(profile_a, profile_b, length).expect("loft");
    let loft_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(loft))
        .expect("add loft");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(loft_node)
        .expect("set root");
    graph.commit("test loft").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, loft_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Each of the 12 triangles in a square×square Loft (n=4) projected mesh
/// resolves to one of the 6 stable `BRepFaceId`s minted by the upstream's
/// `BRepProvider::brep_face_ids` impl.
#[test]
fn loft_projection_query_returns_brep_face_id_for_each_triangle() {
    let (graph, projection, world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // n=4 ⇒ 4n - 4 = 12 triangles.
    assert_eq!(mesh.triangle_count(), 12);

    let loft_for_compare = LoftOp::new(unit_square(), larger_square(), 1.0).unwrap();
    let direct_pairs = loft_for_compare.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 6); // n+2 faces
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    for tri in 0..12 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| panic!("face id for triangle {tri}"));
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// The triangle → `BRepFaceId` mapping for a square×square Loft (n=4) follows
/// the canonical face emission order documented in `LoftOp::evaluate` and
/// `impl BRepProvider for LoftOp`:
///
/// * Triangles 0-1 → Bottom (face 0)
/// * Triangles 2-3 → Top (face 1)
/// * Triangles 4-5 → Side(0)
/// * Triangles 6-7 → Side(1)
/// * Triangles 8-9 → Side(2)
/// * Triangles 10-11 → Side(3)
#[test]
fn loft_projection_query_canonical_face_order_for_squares() {
    let (graph, projection, world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
    let loft_for_compare = LoftOp::new(unit_square(), larger_square(), 1.0).unwrap();
    let direct_ids: Vec<BRepFaceId> = loft_for_compare
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    for tri in 0..2 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[0], "triangle {tri} should be Bottom");
    }
    for tri in 2..4 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[1], "triangle {tri} should be Top");
    }
    for i in 0..4 {
        let face_idx = 2 + i;
        for offset in 0..2 {
            let tri = 4 + 2 * i + offset;
            let id = projection
                .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
                .expect("face id");
            assert_eq!(
                id, direct_ids[face_idx],
                "triangle {tri} should be Side({i})"
            );
        }
    }
}

/// **LOAD-BEARING — rebuild stability across length changes.**
///
/// Length is topology-preserving per the D-7.2-δ contract (the per-face tag
/// for `Bottom`/`Top`/`Side` does NOT include length). Same profiles + same
/// vertex order ⇒ same face IDs across rebuilds with different lengths.
#[test]
fn loft_face_ids_stable_across_length_changes() {
    let (mut graph, mut projection, mut world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
    let initial_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(initial_ids.len(), 12);

    graph.begin_operation().expect("begin");
    let new_loft = LoftOp::new(unit_square(), larger_square(), 2.5).unwrap();
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(new_loft))
        .expect("rebuild");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("root");
    graph.commit("rebuild len=2.5").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let rebuilt_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across length rebuilds"
    );
}

/// **LOAD-BEARING — rebuild stability across coordinate changes (same N).**
///
/// Same profile vertex count + same vertex order = same topology. Coordinates
/// change but `BRepFaceId`s are stable per D-7.2-δ (the `Side` tag includes
/// only `edge_index`, `profile_a_count`, `profile_b_count`, none of which
/// vary with vertex coordinates).
#[test]
fn loft_face_ids_stable_across_coordinate_changes_with_same_n() {
    let (mut graph, mut projection, mut world, entity) =
        build_loft_projection(unit_square(), unit_square(), 1.0);
    let initial_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();

    graph.begin_operation().expect("begin");
    // Different coordinates, same n=4 for both profiles.
    let new_loft = LoftOp::new(unit_square(), larger_square(), 1.0).unwrap();
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(new_loft))
        .expect("rebuild");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("root");
    graph.commit("rebuild larger profile_b").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let rebuilt_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across coordinate-only changes (same N)"
    );
}

/// **LOAD-BEARING — profile-count change breaks Side IDs.**
///
/// LoftOp validates equal profile counts at evaluate time, so a topology
/// change requires changing BOTH profiles together: square×square (n=4) →
/// pentagon×pentagon (n=5). Per D-7.2-δ:
///
/// * Bottom and Top IDs are **categorical** — their tag does NOT include
///   profile counts, so caps' IDs match across the topology change.
/// * Side IDs include `profile_a_count` AND `profile_b_count` in the tag,
///   so EVERY square Side ID must be disjoint from every pentagon Side ID.
#[test]
fn loft_face_ids_change_when_profile_count_changes() {
    let (graph_sq, projection_sq, world_sq, entity_sq) =
        build_loft_projection(unit_square(), unit_square(), 1.0);
    let sq_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| {
            projection_sq.brep_face_id_for_triangle(entity_sq, tri, &world_sq, graph_sq.graph())
        })
        .collect();

    let (graph_pen, projection_pen, world_pen, entity_pen) =
        build_loft_projection(pentagon(), larger_pentagon(), 1.0);
    let pen_mesh = projection_pen.projected_mesh(entity_pen).expect("mesh");
    let pen_tri_count = pen_mesh.triangle_count();
    let pen_ids: Vec<BRepFaceId> = (0..pen_tri_count)
        .filter_map(|tri| {
            projection_pen.brep_face_id_for_triangle(entity_pen, tri, &world_pen, graph_pen.graph())
        })
        .collect();

    assert_eq!(sq_ids.len(), 12);
    assert_eq!(pen_ids.len(), 16);

    // Bottom and Top IDs are categorical (no profile_count in tag) — they
    // SHOULD match across profile shape changes per D-7.2-δ.
    assert_eq!(
        sq_ids[0], pen_ids[0],
        "Bottom is categorical, should match across n=4 → n=5"
    );
    assert_eq!(
        sq_ids[2], pen_ids[3],
        "Top is categorical, should match across n=4 → n=5"
    );

    // Side IDs must be disjoint — `profile_a_count` AND `profile_b_count`
    // (4 vs 5) are in the Side tag.
    let sq_sides: Vec<&BRepFaceId> = sq_ids[4..].iter().collect();
    let pen_sides: Vec<&BRepFaceId> = pen_ids[6..].iter().collect();
    for sq_side in &sq_sides {
        for pen_side in &pen_sides {
            assert_ne!(
                sq_side, pen_side,
                "side IDs must not collide across profile-count change"
            );
        }
    }
}

/// An out-of-bounds triangle index returns `None` rather than panicking.
#[test]
fn loft_query_returns_none_for_out_of_bounds_triangle() {
    let (graph, projection, world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 99, &world, graph.graph()),
        None
    );
}

/// An entity whose `BRepHandle.brep_owner` is `None` (the legacy default)
/// returns `None` even when the projected mesh has `face_labels`.
#[test]
fn loft_query_returns_none_when_brep_owner_is_none() {
    let (graph, projection, mut world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
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
/// through the projection consumer surface for two-profile topology.
#[test]
fn distinct_owners_produce_disjoint_face_ids_through_loft_projection() {
    let owner_y = BRepOwnerId::from_bytes([0xab; 16]);
    assert_ne!(TEST_OWNER, owner_y, "owners must be distinct for this test");

    let (graph, projection, mut world, entity) =
        build_loft_projection(unit_square(), larger_square(), 1.0);
    let ids_x: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner_y);
        }
    }
    let ids_y: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(ids_x.len(), 12);
    assert_eq!(ids_y.len(), 12);

    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-x face id leaked into owner-y space"
        );
    }
}
