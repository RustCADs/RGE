//! Save-side tests for `save_project_world_to_path` (PROJECT-SAVE-SUBSTRATE
//! dispatch) — the `.rge-project` writer.
//!
//! The round-trip anchor mirrors `scene_save_round_trip.rs`: a write followed
//! by `load_scene_world_from_path` against the SAME `.rge-project` must
//! reproduce the world. The writer mirrors the reader's resolution (exact
//! `.rge-project` name; first scene; project-parent-relative path), so a
//! re-load is correct by construction. Worlds are seeded from the tracked
//! golden simple-scene (Camera + KeyLight, all four supported components) so
//! the tests never hand-author component RON.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_kernel_ecs::World;
use rge_scene_loader::{
    load_scene_world_from_path, save_project_world_to_path, ProjectWorldSaveError,
};

type ComponentSets = (
    BTreeMap<u128, Transform>,
    BTreeMap<u128, Camera>,
    BTreeMap<u128, Light>,
    BTreeMap<u128, Visibility>,
);

/// Path to the tracked golden simple-scene `.rge-project` (two levels under the
/// repo root), matching `scene_save_round_trip.rs`.
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

/// A unique temp directory to host a throwaway `.rge-project` + its scene. The
/// process id + a caller label keep concurrent test threads / `cargo test`
/// processes from colliding (the crate has no `tempfile` dev-dependency).
fn unique_temp_project_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rge_project_save_{}_{label}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp project dir");
    dir
}

/// Build a minimal `.rge-project` referencing a single relative scene path, and
/// write it (RON) into `dir/.rge-project`. The scene file itself need not
/// pre-exist — the writer creates it.
fn write_manifest(dir: &std::path::Path, scenes: Vec<&str>) -> (PathBuf, rge_data::Project) {
    let project = rge_data::Project {
        version: rge_data::SchemaVersion::V0_1_0,
        name: "demo".into(),
        description: "project-save round-trip fixture".into(),
        target_tiers: vec![rge_data::TargetTier::Desktop],
        plugins: Vec::new(),
        scenes: scenes
            .into_iter()
            .map(|s| rge_data::ScenePath(s.to_string()))
            .collect(),
    };
    let path = dir.join(".rge-project");
    let text = ron::ser::to_string_pretty(&project, ron::ser::PrettyConfig::default())
        .expect("serialize manifest");
    std::fs::write(&path, text).expect("write manifest");
    (path, project)
}

#[test]
fn save_then_load_project_round_trips_via_disk() {
    let world_a = golden_world();
    let dir = unique_temp_project_dir("round_trip");
    let (project_path, _) = write_manifest(&dir, vec!["level.rge-scene"]);

    save_project_world_to_path(&world_a, &project_path).expect("save .rge-project");

    // The writer created the resolved scene file (project-parent-relative).
    assert!(
        dir.join("level.rge-scene").exists(),
        "save must write the resolved first-scene file"
    );

    // Round-trip: load the project back (its first scene) → world.
    let world_b = load_scene_world_from_path(&project_path).expect("reload saved .rge-project");
    assert_eq!(
        component_sets(&world_a),
        component_sets(&world_b),
        "save -> load must preserve entity ids and the four supported component values"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn manifest_is_preserved_across_save() {
    let world_a = golden_world();
    let dir = unique_temp_project_dir("manifest");
    let (project_path, original) = write_manifest(&dir, vec!["level.rge-scene"]);

    save_project_world_to_path(&world_a, &project_path).expect("save .rge-project");

    let raw = std::fs::read_to_string(&project_path).expect("read manifest back");
    let back: rge_data::Project = ron::from_str(&raw).expect("parse manifest back");
    assert_eq!(
        back, original,
        "manifest must round-trip unchanged (name / scenes / tiers preserved, version V0_1_0)"
    );
    assert_eq!(back.version, rge_data::SchemaVersion::V0_1_0);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn non_project_name_is_rejected_before_io() {
    let world = World::new();
    // `foo.rge-project` is NOT the exact `.rge-project` name the reader accepts.
    let path = std::env::temp_dir().join(format!(
        "rge_project_save_{}_foo.rge-project",
        std::process::id()
    ));
    assert!(
        matches!(
            save_project_world_to_path(&world, &path),
            Err(ProjectWorldSaveError::UnsupportedExtension(_))
        ),
        "only the exact `.rge-project` file name is accepted"
    );
    assert!(
        !path.exists(),
        "a rejected save must not create a file (name check precedes I/O)"
    );
}

#[test]
fn empty_scenes_manifest_is_rejected() {
    let dir = unique_temp_project_dir("empty");
    let (project_path, _) = write_manifest(&dir, vec![]);

    assert!(
        matches!(
            save_project_world_to_path(&World::new(), &project_path),
            Err(ProjectWorldSaveError::EmptyProjectScenes(_))
        ),
        "a manifest with no scenes has nowhere to write the world"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_manifest_is_read_error() {
    let dir = unique_temp_project_dir("missing");
    let project_path = dir.join(".rge-project"); // intentionally not written

    assert!(
        matches!(
            save_project_world_to_path(&World::new(), &project_path),
            Err(ProjectWorldSaveError::Read { .. })
        ),
        "saving into a project whose manifest does not exist is a Read error"
    );

    std::fs::remove_dir_all(&dir).ok();
}
