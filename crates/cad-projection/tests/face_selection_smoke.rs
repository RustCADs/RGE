//! Sub-ζ smoke integration test — CHAPTER CLOSE-OUT for the
//! Render-backed face-selection chapter.
//!
//! Exercises the load-bearing chain end-to-end at the projection layer:
//!
//! ```text
//! Ray -> CadProjection::pick_face -> CadProjection::face_triangle_indices
//! ```
//!
//! Screen-to-ray is included via an inline helper that mirrors
//! [`editor-shell::camera::CameraView::screen_to_world_ray`]. We cannot
//! depend on `editor-shell` here because `editor-shell` already depends on
//! `cad-projection` in production — a dev-dep cycle is rejected by cargo.
//! The canonical implementation is unit-tested in `editor-shell`'s own
//! test suite (`crates/editor-shell/src/camera.rs::tests`); this mirror is
//! kept synced manually given its small surface (~15 lines of glam
//! Mat4 inversion + NDC unprojection).
//!
//! Sub-α through sub-ε land the production chain; this dispatch (sub-ζ)
//! locks the chain down with CI coverage at the cad-projection seam:
//!
//! * Sub-α: `brep-render::RenderMesh` (CPU-side flat-shaded mesh with
//!   `face_labels: Option<Vec<u64>>`).
//! * Sub-β: `editor-shell::camera::CameraView` + `screen_to_world_ray`
//!   (CPU-side unproject primitive).
//! * Sub-γ: `cad-projection::render_adapter::CadProjection::render_mesh_for`
//!   (game-domain → renderer-tier adapter).
//! * Sub-δ.1.B / sub-δ.2: editor-shell wiring (`EditorShell::with_cad_world` +
//!   left-click → `pick_face_at` → `coord.face_selection`).
//! * Sub-ε: `CadProjection::face_triangle_indices` + editor-shell highlight
//!   overlay (`HIGHLIGHT_COLOR` / `rebuild_highlight_overlay`).
//! * Sub-ζ: this file — smoke integration locking the chain in CI.

use std::collections::HashSet;

use glam::{Mat4, Vec3, Vec4};
use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::picking::Ray;
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

/// Owner seed shared by every test in this file. Caller-supplied opaque
/// 16-byte token; explicitly NOT derived from anything content-addressed
/// (same convention as other cad-projection test files —
/// `face_picking_smoke.rs::ENTITY_OWNER`, `lib.rs::tests::TEST_OWNER`).
const TEST_OWNER: rge_cad_core::BRepOwnerId = rge_cad_core::BRepOwnerId::from_bytes([0x42; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

// ---------------------------------------------------------------------------
// MIRROR OF editor-shell::camera::CameraView::screen_to_world_ray
// ---------------------------------------------------------------------------
//
// Kept in sync manually; see module-level docs above. Canonical version
// lives in `crates/editor-shell/src/camera.rs` (look for the doc comment
// "Convert a screen-space pixel position to a world-space [`Ray`]"). The
// editor-shell unit tests (`identity_view_proj_screen_center_yields_ray_through_origin`,
// `viewport_y_flip_correctness`, `near_plane_z_is_zero_per_wgpu_convention`,
// `degenerate_zero_view_proj_returns_none`, etc.) pin the math; this
// helper is a transparent wrapper to enable the same end-to-end chain to
// be exercised at the projection layer without crossing the editor-shell
// dep boundary.
//
// NDC convention: wgpu / Vulkan / D3D — clip-space Z ∈ [0.0, 1.0]; near at 0,
// far at 1. Y is flipped on the way in (winit top-left → wgpu bottom-left
// NDC).
fn screen_to_world_ray(
    view_proj: Mat4,
    viewport_size: [f32; 2],
    screen_pos: [f32; 2],
) -> Option<Ray> {
    let [vw, vh] = viewport_size;
    let ndc_x = 2.0 * (screen_pos[0] / vw) - 1.0;
    let ndc_y = 1.0 - 2.0 * (screen_pos[1] / vh);

    let inv = view_proj.inverse();
    for col in inv.to_cols_array() {
        if !col.is_finite() {
            return None;
        }
    }

    let near_clip = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far_clip = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

    let near_world = inv * near_clip;
    let far_world = inv * far_clip;

    if near_world.w == 0.0 || far_world.w == 0.0 {
        return None;
    }
    let near = near_world.truncate() / near_world.w;
    let far = far_world.truncate() / far_world.w;
    let dir: Vec3 = far - near;

    Some(Ray {
        origin: [near.x, near.y, near.z],
        direction: [dir.x, dir.y, dir.z],
    })
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Build a `(graph, projection, world, entity)` tuple with a single
/// 1×1×1 Cuboid committed and projected under [`TEST_OWNER`]. Mirrors the
/// `build_cuboid` setup pattern shared across cad-projection tests (see
/// `face_picking_smoke.rs::build_cuboid` and
/// `lib.rs::tests::build_cuboid_entity`).
fn setup_cuboid_scene() -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
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
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");
    (graph, projection, world, entity)
}

/// `(view_proj, viewport_size)` for a perspective camera at `(4, 3, 4)`
/// looking at the world origin with `+Y` up. The 1×1×1 cuboid at the
/// origin presents three visible faces (+X, +Y, +Z) to this camera so the
/// triplet of screen positions in [`cuboid_face_clicks_pick_and_yield_six_highlight_indices`]
/// resolves three distinct faces.
///
/// FOV is 45°, aspect 4/3, near 0.1, far 100.0 — `Mat4::perspective_rh`
/// (wgpu / Vulkan / D3D NDC convention, Z ∈ `[0.0, 1.0]`) matches the
/// LOAD-BEARING convention assumption on `screen_to_world_ray`.
fn setup_camera() -> (Mat4, [f32; 2]) {
    let viewport: [f32; 2] = [800.0, 600.0];
    let view = Mat4::look_at_rh(
        Vec3::new(4.0, 3.0, 4.0),
        Vec3::ZERO,
        Vec3::new(0.0, 1.0, 0.0),
    );
    let proj = Mat4::perspective_rh(
        std::f32::consts::FRAC_PI_4,
        viewport[0] / viewport[1],
        0.1,
        100.0,
    );
    (proj * view, viewport)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// **Smoke-1** — three screen positions chosen to land on distinct visible
/// faces of the cuboid. For each click:
///
/// 1. `screen_to_world_ray` produces a `Ray`.
/// 2. `CadProjection::pick_face` returns a `FacePick` with a stable
///    `BRepFaceId`.
/// 3. `CadProjection::face_triangle_indices` returns exactly 6 dense vertex
///    indices in the `[3i, 3i+1, 3i+2]` shape, all within the cuboid's
///    36-vertex flat-shaded buffer.
///
/// After the three clicks, the picker has resolved at least 2 distinct
/// `BRepFaceId`s — the camera at `(4, 3, 4)` makes the +X / +Y / +Z faces
/// visible, and the three screen positions are placed so each picks a
/// different face.
#[test]
fn cuboid_face_clicks_pick_and_yield_six_highlight_indices() {
    let (graph, projection, world, _entity) = setup_cuboid_scene();
    let (view_proj, viewport) = setup_camera();

    // Three screen positions deliberately placed on different regions of
    // the cuboid's projected silhouette. Under the `(4, 3, 4)` looking-at-
    // origin camera, the cuboid's three visible faces (+X, +Y, +Z) occupy
    // distinct screen-space sectors; each position is biased toward one of
    // them. Test must assert outcomes (count + shape + distinct-face),
    // NOT specific face_ids — the projection of an unrotated cuboid to a
    // diagonal-camera viewport is sensitive to viewport ratio and FOV,
    // and pinning specific face_ids would couple the test to the camera
    // setup arithmetic rather than the chain semantics.
    // Cuboid at origin (bbox [-0.5..0.5]^3), camera at (4, 3, 4) with FOV 45°,
    // aspect 4/3, viewport 800×600. The cuboid projects to a small footprint
    // near the viewport center; positions chosen empirically by reasoning
    // about the projected silhouette of the three visible faces (+X / +Y /
    // +Z) — center hits +Z, slight upper-center hits +Y, slight lower-right
    // hits +X.
    let screen_positions = [
        [400.0, 300.0], // viewport center — biased toward the front-facing face
        [400.0, 250.0], // upper-center within silhouette — biased toward the top face (+Y)
        [450.0, 320.0], // lower-right within silhouette — biased toward the right face (+X)
    ];

    let mut hits: Vec<rge_cad_core::BRepFaceId> = Vec::new();
    for screen in screen_positions {
        let ray = screen_to_world_ray(view_proj, viewport, screen)
            .expect("ray must construct for a non-degenerate camera");

        let pick = projection
            .pick_face(&ray, &world, graph.graph())
            .unwrap_or_else(|| {
                panic!(
                    "ray from screen {screen:?} must hit the cuboid (visible from camera at (4,3,4) looking at origin)",
                )
            });
        hits.push(pick.face_id);

        let indices =
            projection.face_triangle_indices(pick.entity, &world, graph.graph(), pick.face_id);
        assert_eq!(
            indices.len(),
            6,
            "cuboid face at screen {screen:?} must yield 2 triangles × 3 indices = 6 vertices; got {} indices",
            indices.len()
        );
        for &idx in &indices {
            assert!(
                (idx as usize) < 36,
                "vertex index {idx} out of range [0, 36) for cuboid's flat-shaded 36-vertex buffer",
            );
        }
        // Triple-shape check: indices come in groups of 3 of the form
        // [3i, 3i+1, 3i+2]. This mirrors the dense vertex-tripled layout
        // assertion in `lib.rs::tests::face_triangle_indices_cuboid_returns_six_indices_per_face`.
        for chunk in indices.chunks(3) {
            assert_eq!(chunk.len(), 3, "indices must be a multiple of 3 in length");
            assert_eq!(
                chunk[0] % 3,
                0,
                "vertex-tripled shape: triangle base must be a multiple of 3; got {}",
                chunk[0]
            );
            assert_eq!(
                chunk[1],
                chunk[0] + 1,
                "vertex-tripled shape: second index must follow first",
            );
            assert_eq!(
                chunk[2],
                chunk[0] + 2,
                "vertex-tripled shape: third index must follow first",
            );
        }
    }

    // The three screen positions were chosen to hit different faces. If
    // they all collapsed to one face_id, our camera or screen positions
    // are degenerate — the test would not be exercising the chain end-to-
    // end. Cuboid has 6 faces; 3 are visible from any non-degenerate
    // diagonal camera angle.
    let unique_count = hits.iter().collect::<HashSet<_>>().len();
    assert!(
        unique_count >= 2,
        "expected at least 2 distinct face_ids across 3 click positions; \
         got {unique_count} from {hits:?}",
    );
}

/// **Smoke-2** — a screen position aimed at empty space (a corner of the
/// viewport that the camera frame puts well outside the cuboid's
/// silhouette) yields a `Ray` that resolves to `None` from
/// `CadProjection::pick_face`. The miss path is part of the chain
/// contract — the click-handler must distinguish "hit no face" from
/// "the pick is `None` for some other reason".
#[test]
fn empty_space_click_misses() {
    let (graph, projection, world, _entity) = setup_cuboid_scene();
    let (view_proj, viewport) = setup_camera();

    // Top-left corner of the viewport — well outside the cuboid's
    // projected silhouette under the camera at `(4, 3, 4)`. Mirrors the
    // pattern from `editor-shell::camera::pick_face_at_returns_none_for_off_axis_screen_pos_far_from_cuboid`.
    let miss_ray = screen_to_world_ray(view_proj, viewport, [50.0, 50.0])
        .expect("ray must construct for a non-degenerate camera");
    let pick = projection.pick_face(&miss_ray, &world, graph.graph());
    assert!(
        pick.is_none(),
        "ray aimed at empty space must not produce a pick; got {pick:?}",
    );
}

/// **Smoke-3** — same ray, called twice, returns byte-identical results.
///
/// Both the `face_id` (from `pick_face`) and the `Vec<u32>` (from
/// `face_triangle_indices`) must be stable across repeated invocations on
/// the same projection / world / graph snapshot. This is the substrate-
/// honest "no hidden state" contract that downstream caching code can
/// rely on — every call re-runs the same enumeration (see
/// `lib.rs::face_triangle_indices` doc comment "Neither caches; each call
/// runs the full enumeration").
#[test]
fn pick_then_highlight_indices_consistent_for_same_ray() {
    let (graph, projection, world, _entity) = setup_cuboid_scene();
    let (view_proj, viewport) = setup_camera();

    let ray = screen_to_world_ray(view_proj, viewport, [400.0, 300.0])
        .expect("ray must construct for a non-degenerate camera");

    let p1 = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("center-screen ray must hit the cuboid");
    let p2 = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("second pick on the same ray must also hit");
    assert_eq!(
        p1.face_id, p2.face_id,
        "same-ray pick_face must return identical face_id across calls",
    );
    assert_eq!(
        p1.entity, p2.entity,
        "same-ray pick_face must return identical entity across calls",
    );

    let i1 = projection.face_triangle_indices(p1.entity, &world, graph.graph(), p1.face_id);
    let i2 = projection.face_triangle_indices(p1.entity, &world, graph.graph(), p1.face_id);
    assert_eq!(
        i1, i2,
        "face_triangle_indices must be byte-identical across repeated calls for the same face_id",
    );
    assert_eq!(
        i1.len(),
        6,
        "cuboid face highlight must be 6 dense vertex indices; got {}",
        i1.len()
    );
}
