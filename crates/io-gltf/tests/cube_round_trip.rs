//! Cube round-trip — exit criterion #1.
//!
//! `cube.glb` (procedurally generated, see `common/mod.rs`) is imported,
//! re-exported, then imported again. The resulting [`Scene`] must match the
//! original within tolerance:
//!
//! - vertex count + triangle count exact,
//! - material PBR factors within `1e-5`,
//! - root-entity translation within `1e-5`.

mod common;

use rge_io_gltf::{import_glb, Cache, MemoryCache};

const EPS: f32 = 1e-5;

#[test]
fn cube_glb_round_trips() {
    let cube_path = common::cube_fixture_path();

    // First import — the canonical scene.
    let mut cache_a = MemoryCache::new();
    let scene_a = import_glb(&cube_path, &mut cache_a).expect("first import");

    // Verify the imported scene matches what we built.
    assert_eq!(scene_a.entities.len(), 1, "one cube entity");
    let cube = &scene_a.entities[0];
    assert_eq!(cube.name, "cube");
    let mh = cube.mesh.expect("mesh handle on cube entity");
    let mat_h = cube.material.expect("material handle on cube entity");
    let mesh = cache_a.get_mesh(&mh).expect("mesh in cache");
    let mat = cache_a.get_material(&mat_h).expect("material in cache");
    assert_eq!(mesh.vertex_count(), 24, "cube = 6 faces × 4 verts");
    assert_eq!(mesh.triangle_count(), 12, "cube = 6 faces × 2 tris");
    assert!((mat.base_color[0] - 0.4).abs() < EPS);
    assert!((mat.metallic - 0.1).abs() < EPS);
    assert!((mat.roughness - 0.7).abs() < EPS);
    assert!((cube.transform.translation[0] - 1.0).abs() < EPS);
    assert!((cube.transform.translation[1] - 2.0).abs() < EPS);
    assert!((cube.transform.translation[2] - 3.0).abs() < EPS);

    // Re-export.
    let bytes_b = rge_io_gltf::export_glb(&scene_a, &cache_a).expect("re-export");

    // Second import — must be equivalent within tolerance.
    let mut cache_b = MemoryCache::new();
    let scene_b = rge_io_gltf::import_glb_bytes(&bytes_b, &mut cache_b).expect("second import");

    assert_eq!(scene_b.entities.len(), scene_a.entities.len());
    let cube_b = &scene_b.entities[0];
    let mesh_b = cache_b
        .get_mesh(&cube_b.mesh.expect("mesh-b handle"))
        .expect("mesh-b in cache");
    let mat_b = cache_b
        .get_material(&cube_b.material.expect("mat-b handle"))
        .expect("mat-b in cache");

    assert_eq!(mesh.vertex_count(), mesh_b.vertex_count());
    assert_eq!(mesh.triangle_count(), mesh_b.triangle_count());
    for i in 0..4 {
        assert!(
            (mat.base_color[i] - mat_b.base_color[i]).abs() < EPS,
            "base_color[{i}] drifted: {} vs {}",
            mat.base_color[i],
            mat_b.base_color[i]
        );
    }
    assert!((mat.metallic - mat_b.metallic).abs() < EPS);
    assert!((mat.roughness - mat_b.roughness).abs() < EPS);
    for i in 0..3 {
        assert!((cube.transform.translation[i] - cube_b.transform.translation[i]).abs() < EPS);
    }

    // Mesh content-hashes must dedupe across the two caches — same bytes
    // hash to the same handle. (Asset cache is content-addressed.)
    assert_eq!(mh, mesh_b.content_hash(), "mesh content-hash drift");
}

#[test]
fn cube_export_is_byte_stable() {
    // Same scene exported twice must produce identical bytes — content-
    // addressable export (the export path uses ordered BTreeMaps + entity-
    // ordered iteration so BIN/JSON layout is deterministic).
    let cube_path = common::cube_fixture_path();
    let mut cache = MemoryCache::new();
    let scene = import_glb(&cube_path, &mut cache).expect("import");
    let a = rge_io_gltf::export_glb(&scene, &cache).expect("export 1");
    let b = rge_io_gltf::export_glb(&scene, &cache).expect("export 2");
    assert_eq!(a, b);
}
