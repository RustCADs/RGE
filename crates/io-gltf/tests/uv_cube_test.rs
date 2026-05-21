//! Dispatch M1 — UV-cube fixture round-trip integration tests.
//!
//! Pairs with `tests/common::make_uv_cube_glb`. Validates that
//! `MeshAsset.texcoords` round-trips through import → export → import
//! correctly (the existing `extract_meshes` already reads `TEXCOORD_0`;
//! the exporter writes it; this test pins both behaviours behind a
//! committed fixture).

mod common;

use rge_io_gltf::{import_glb_bytes, Cache, MemoryCache};

#[test]
fn uv_cube_fixture_imports_with_non_empty_texcoords() {
    let path = common::uv_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read uv_cube.glb");
    let mut cache = MemoryCache::new();
    let scene = import_glb_bytes(&bytes, &mut cache).expect("import");
    assert_eq!(scene.entities.len(), 1);
    let mh = scene.entities[0]
        .mesh
        .expect("uv_cube scene's root entity carries a mesh");
    let mesh = cache.get_mesh(&mh).expect("mesh in cache");
    assert_eq!(
        mesh.positions.len(),
        24,
        "uv_cube has 24 verts (6 faces × 4 quad verts)"
    );
    assert_eq!(
        mesh.texcoords.len(),
        24,
        "uv_cube has 24 UVs (one per vertex)"
    );
    // First face's first vertex must be at UV (0, 0).
    assert_eq!(mesh.texcoords[0], [0.0, 0.0]);
    // Each face's 4 UVs cover the unit square in (0,0)→(1,0)→(1,1)→(0,1) order.
    assert_eq!(mesh.texcoords[1], [1.0, 0.0]);
    assert_eq!(mesh.texcoords[2], [1.0, 1.0]);
    assert_eq!(mesh.texcoords[3], [0.0, 1.0]);
}

#[test]
fn uv_cube_fixture_round_trip_preserves_texcoords() {
    // Build → export → re-import. Texcoords on the re-imported mesh
    // must be identical to those on the original.
    let bytes_a = common::make_uv_cube_glb();
    let mut cache_a = MemoryCache::new();
    let scene_a = import_glb_bytes(&bytes_a, &mut cache_a).expect("import a");
    let mh_a = scene_a.entities[0].mesh.expect("mesh");
    let mesh_a = cache_a.get_mesh(&mh_a).expect("get_mesh a").clone();

    // Re-export and re-import.
    let bytes_b = rge_io_gltf::export_glb(&scene_a, &cache_a).expect("re-export");
    let mut cache_b = MemoryCache::new();
    let scene_b = import_glb_bytes(&bytes_b, &mut cache_b).expect("import b");
    let mh_b = scene_b.entities[0].mesh.expect("mesh");
    let mesh_b = cache_b.get_mesh(&mh_b).expect("get_mesh b");

    assert_eq!(mesh_a.texcoords, mesh_b.texcoords);
    assert_eq!(mesh_a.positions, mesh_b.positions);
    assert_eq!(mesh_a.indices, mesh_b.indices);
}

#[test]
fn cube_fixture_has_empty_texcoords() {
    // Regression: the existing cube.glb fixture does NOT carry UVs.
    // This test pins that contract so a future PR that adds UVs to
    // `make_cube_glb` updates both the fixture and any consumer
    // expecting empty texcoords.
    let path = common::cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read cube.glb");
    let mut cache = MemoryCache::new();
    let scene = import_glb_bytes(&bytes, &mut cache).expect("import");
    let mh = scene.entities[0].mesh.expect("mesh");
    let mesh = cache.get_mesh(&mh).expect("get_mesh");
    assert!(
        mesh.texcoords.is_empty(),
        "cube.glb is not UV-mapped at v0; got {} texcoords",
        mesh.texcoords.len()
    );
}
