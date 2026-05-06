//! PBR material import — exit criterion #3.
//!
//! `pbr_material.glb` (procedural; see `common/mod.rs`) populates a
//! [`rge_io_gltf::MaterialAsset`] with a non-trivial PBR parameter set:
//!
//! - `base_color`, `metallic`, `roughness` round-trip,
//! - texture-index slots (`base_color_texture`, `normal_texture`,
//!   `metallic_roughness_texture`) round-trip when present,
//! - `alpha_mode`, `alpha_cutoff`, `double_sided`, `emissive` round-trip.

mod common;

use rge_io_gltf::{import_glb, AlphaMode, Cache, MemoryCache};

const EPS: f32 = 1e-5;

#[test]
fn pbr_material_round_trips_all_parameters() {
    let path = common::pbr_material_fixture_path();
    let mut cache = MemoryCache::new();
    let scene = import_glb(&path, &mut cache).expect("import pbr material");

    // The fixture has exactly one entity with a material.
    let entity = scene
        .entities
        .iter()
        .find(|e| e.material.is_some())
        .expect("entity with material");
    let mat = cache
        .get_material(&entity.material.expect("mat handle"))
        .expect("material in cache");

    // Base PBR.
    assert!((mat.base_color[0] - 0.97).abs() < EPS);
    assert!((mat.base_color[1] - 0.86).abs() < EPS);
    assert!((mat.base_color[2] - 0.32).abs() < EPS);
    assert!((mat.base_color[3] - 1.0).abs() < EPS);
    assert!((mat.metallic - 1.0).abs() < EPS);
    assert!((mat.roughness - 0.18).abs() < EPS);

    // Texture-index slots.
    assert_eq!(mat.base_color_texture, Some(0));
    assert_eq!(mat.normal_texture, Some(1));
    assert_eq!(mat.metallic_roughness_texture, Some(2));

    // Emissive + alpha.
    assert!((mat.emissive[0] - 0.05).abs() < EPS);
    assert!((mat.emissive[1] - 0.0).abs() < EPS);
    assert!((mat.emissive[2] - 0.0).abs() < EPS);
    assert_eq!(mat.alpha_mode, AlphaMode::Mask);
    assert!((mat.alpha_cutoff - 0.4).abs() < EPS);
    assert!(mat.double_sided);
}

#[test]
fn pbr_material_re_export_preserves_alpha_mode() {
    let path = common::pbr_material_fixture_path();
    let mut cache_a = MemoryCache::new();
    let scene_a = import_glb(&path, &mut cache_a).expect("import");
    let bytes = rge_io_gltf::export_glb(&scene_a, &cache_a).expect("export");

    let mut cache_b = MemoryCache::new();
    let scene_b = rge_io_gltf::import_glb_bytes(&bytes, &mut cache_b).expect("re-import");

    let mat_a = cache_a
        .get_material(&scene_a.entities[0].material.expect("a"))
        .expect("mat a");
    let mat_b = cache_b
        .get_material(&scene_b.entities[0].material.expect("b"))
        .expect("mat b");

    assert_eq!(mat_a.alpha_mode, mat_b.alpha_mode);
    assert!((mat_a.alpha_cutoff - mat_b.alpha_cutoff).abs() < EPS);
    assert_eq!(mat_a.double_sided, mat_b.double_sided);
    assert_eq!(mat_a.normal_texture, mat_b.normal_texture);
    assert_eq!(
        mat_a.metallic_roughness_texture,
        mat_b.metallic_roughness_texture
    );
}
