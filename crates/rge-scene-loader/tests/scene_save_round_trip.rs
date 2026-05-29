//! Save-side tests for `rge-scene-loader`: `extract_scene_from_world` +
//! `save_scene_world_to_path` (SCENE-SAVE-SUBSTRATE dispatch).
//!
//! The `World -> Scene -> World` round trip against the existing loader is the
//! spec-conformance anchor: the loader + the `rge-data` schema are the
//! de-facto reference for the `.rge-scene` format, so a save that re-loads to
//! the same typed components is correct by construction. Worlds are seeded from
//! the tracked golden simple-scene fixture (Camera + KeyLight, covering all
//! four supported components) so the tests never hand-author component RON or
//! couple to component field layouts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_kernel_ecs::{Component, World};
use rge_scene_loader::{
    extract_scene_from_world, load_scene_into_world, load_scene_world_from_path,
    save_scene_world_to_path, SceneWorldSaveError,
};

/// The `(raw ULID, value)` set for each supported component type, so two worlds
/// can be compared by value independent of RON formatting or storage order.
/// (Aliased to keep `clippy::type_complexity` quiet at the helper signature.)
type ComponentSets = (
    BTreeMap<u128, Transform>,
    BTreeMap<u128, Camera>,
    BTreeMap<u128, Light>,
    BTreeMap<u128, Visibility>,
);

/// Path to the tracked golden simple-scene `.rge-project`. Resolves from
/// `crates/rge-scene-loader` (two levels under the repo root), matching
/// `tests/scene_path_loader.rs`.
fn golden_project_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

/// Load the golden simple-scene into a `World` via the public path resolver.
fn golden_world() -> World {
    load_scene_world_from_path(&golden_project_path()).expect("load golden simple-scene")
}

/// Collect the supported-component value sets of `world`, keyed by raw ULID.
fn component_sets(world: &World) -> ComponentSets {
    (
        world
            .query::<Transform>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Camera>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Light>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Visibility>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
    )
}

/// A unique `*.rge-scene` path under the OS temp dir. The process id + a
/// caller-supplied label keep concurrent test threads and concurrent
/// `cargo test` processes from colliding (the crate has no `tempfile`
/// dev-dependency to lean on).
fn unique_temp_scene(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rge_scene_save_{}_{label}.rge-scene",
        std::process::id()
    ))
}

#[test]
fn world_scene_world_value_round_trip() {
    let world_a = golden_world();

    let scene_1 = extract_scene_from_world(&world_a, "round-trip").expect("extract");
    // The extracted scene must re-load — its component `data` is valid RON for
    // each target type and its `type_id`s match the loader's allowlist.
    let world_b = load_scene_into_world(&scene_1).expect("reload extracted scene");

    assert_eq!(
        component_sets(&world_a),
        component_sets(&world_b),
        "World -> Scene -> World must preserve entity ids and the four supported component values"
    );

    // Extract is a fixed point through load (round-trip determinism).
    let scene_2 = extract_scene_from_world(&world_b, "round-trip").expect("re-extract");
    assert_eq!(
        scene_1, scene_2,
        "extract must be a fixed point through load"
    );
}

#[test]
fn extract_is_deterministic_and_byte_stable() {
    let world = golden_world();

    let s1 = extract_scene_from_world(&world, "det").expect("extract 1");
    let s2 = extract_scene_from_world(&world, "det").expect("extract 2");
    assert_eq!(s1, s2, "two extractions of the same world must be equal");

    let r1 = ron::ser::to_string_pretty(&s1, ron::ser::PrettyConfig::default()).expect("ser 1");
    let r2 = ron::ser::to_string_pretty(&s2, ron::ser::PrettyConfig::default()).expect("ser 2");
    assert_eq!(
        r1, r2,
        "pretty-RON of two extractions must be byte-identical (sorted entities, fixed component order)"
    );
}

#[test]
fn empty_world_extracts_empty_scene() {
    let scene = extract_scene_from_world(&World::new(), "blank").expect("extract empty");
    assert!(scene.entities.is_empty(), "empty world yields no entities");
    assert!(
        scene.root_entities.is_empty(),
        "empty world yields no roots"
    );
    assert_eq!(scene.name, "blank", "scene name is caller-supplied");
    assert_eq!(
        scene.version,
        rge_data::SchemaVersion::V0_1_0,
        "scene version is stamped to the current schema"
    );
}

/// A component outside the four-string allowlist, used to prove the v0 union
/// enumeration emits neither component-less nor unsupported-only entities.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Marker(u32);
impl Component for Marker {}

#[test]
fn entities_without_supported_components_are_skipped() {
    let mut world = World::new();
    // (a) a truly component-less entity, and
    let _bare = world.spawn();
    // (b) an entity carrying only an out-of-allowlist component.
    let marked = world.spawn();
    world.insert(marked, Marker(7));
    assert_eq!(world.entity_count(), 2, "both entities are live");

    let scene = extract_scene_from_world(&world, "skip").expect("extract");
    assert!(
        scene.entities.is_empty(),
        "v0 union enumeration emits neither component-less nor unsupported-only entities"
    );
}

#[test]
fn fidelity_contract_emits_empty_names_relations_roots() {
    let world = golden_world();
    let scene = extract_scene_from_world(&world, "fidelity").expect("extract");

    assert!(
        !scene.entities.is_empty(),
        "golden world has supported-component entities to check"
    );
    for entity in &scene.entities {
        assert_eq!(entity.name, "", "v0 does not recover entity names");
        assert!(entity.relations.is_empty(), "v0 does not recover relations");
    }
    assert!(
        scene.root_entities.is_empty(),
        "v0 does not recover root entities"
    );
}

#[test]
fn save_then_load_round_trips_via_disk() {
    let world_a = golden_world();
    let path = unique_temp_scene("save_round_trip");

    save_scene_world_to_path(&world_a, &path, "disk").expect("save .rge-scene");
    let world_c = load_scene_world_from_path(&path).expect("reload saved .rge-scene");

    assert_eq!(
        component_sets(&world_a),
        component_sets(&world_c),
        "save -> load must preserve entity ids and supported component values"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn unsupported_extension_is_rejected() {
    let world = World::new();

    // The literal `.rge-project` the loader accepts is rejected on save in v0
    // (project write + manifest update is a follow-up).
    let proj = std::env::temp_dir().join(".rge-project");
    assert!(
        matches!(
            save_scene_world_to_path(&world, &proj, "x"),
            Err(SceneWorldSaveError::UnsupportedExtension(_))
        ),
        "literal .rge-project must be rejected on save"
    );

    // A wrong extension is rejected before any file is written.
    let txt =
        std::env::temp_dir().join(format!("rge_scene_save_{}_reject.txt", std::process::id()));
    assert!(
        matches!(
            save_scene_world_to_path(&world, &txt, "x"),
            Err(SceneWorldSaveError::UnsupportedExtension(_))
        ),
        "non-.rge-scene extension must be rejected"
    );
    assert!(
        !txt.exists(),
        "a rejected save must not create a file (extension check precedes I/O)"
    );
}
