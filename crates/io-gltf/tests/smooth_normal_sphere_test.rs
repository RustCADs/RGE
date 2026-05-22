//! ISSUE-87 — smooth-normal sphere fixture: materialise + property checks.
//!
//! Pairs with `tests/common::make_smooth_normal_sphere_glb`. Running this
//! test materialises `tests/fixtures/smooth_normal_sphere.glb` (the
//! committed binary) and re-imports it to pin the substrate-correctness
//! properties the rge-editor end-to-end visual test relies on:
//!
//! - exactly one drawable mesh in the scene;
//! - per-vertex normals 1:1 aligned with positions before render-mesh
//!   vertex tripling;
//! - unit-length normals on every vertex (smooth = normalized radial);
//! - at least one triangle whose three input normals are meaningfully
//!   non-coplanar with each other, so vertex-tripled smooth normals
//!   produce different per-fragment lighting than a single per-
//!   triangle cross-product flat normal.

mod common;

use rge_io_gltf::{import_glb_bytes, Cache, MemoryCache};

#[test]
fn smooth_normal_sphere_fixture_imports_with_smooth_per_vertex_normals() {
    let path = common::smooth_normal_sphere_fixture_path();
    let bytes = std::fs::read(&path).expect("read smooth_normal_sphere.glb");
    let mut cache = MemoryCache::new();
    let scene = import_glb_bytes(&bytes, &mut cache).expect("import");

    assert_eq!(
        scene.entities.len(),
        1,
        "smooth_normal_sphere.glb carries exactly one entity"
    );
    let mh = scene.entities[0]
        .mesh
        .expect("smooth_normal_sphere entity carries a mesh");
    let mesh = cache.get_mesh(&mh).expect("mesh in cache");

    assert!(
        !mesh.normals.is_empty(),
        "fixture must carry NORMAL accessor"
    );
    assert_eq!(
        mesh.positions.len(),
        mesh.normals.len(),
        "positions / normals must be 1:1 before render-mesh vertex tripling"
    );
    assert_eq!(
        mesh.indices.len() % 3,
        0,
        "indices.len() must be a multiple of 3"
    );

    // Every normal is unit-length — smooth = normalized radial vector.
    for (i, n) in mesh.normals.iter().enumerate() {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-4,
            "normal[{i}] = {n:?} must be unit-length; got len={len}"
        );
    }

    // At least one triangle's three vertex normals must differ from each
    // other by a meaningful angle so vertex-tripled smooth normals
    // produce per-fragment lighting different from the single
    // cross-product flat normal a from_buffers_with_attributes(..., None,
    // ...) construction would emit. We use max channel delta > 0.1
    // (≈ 5.7°) as the distinguishing threshold — large enough that
    // Lambert+Phong shading shows visible variation, small enough to
    // tolerate the discrete latitude/longitude sampling.
    let mut found_smooth_triangle = false;
    for tri in mesh.indices.chunks_exact(3) {
        let n0 = mesh.normals[tri[0] as usize];
        let n1 = mesh.normals[tri[1] as usize];
        let n2 = mesh.normals[tri[2] as usize];
        let max_pair_delta =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0_f32, f32::max);
        if max_pair_delta(n0, n1) > 0.1
            || max_pair_delta(n0, n2) > 0.1
            || max_pair_delta(n1, n2) > 0.1
        {
            found_smooth_triangle = true;
            break;
        }
    }
    assert!(
        found_smooth_triangle,
        "smooth_normal_sphere must contain at least one triangle with \
         meaningfully different per-vertex normals; otherwise imported \
         smooth normals would be indistinguishable from flat recompute"
    );
}
