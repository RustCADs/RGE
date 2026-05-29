//! Path-resolver tests for [`rge_scene_loader::load_scene_world_from_path`].
//!
//! Exercises the `.rge-project` / `.rge-scene` **path** entry point — extension
//! dispatch + project-scene resolution — without a GPU or winit context. The
//! `Scene -> World` component translation itself is covered by
//! `tests/simple_scene.rs`; these tests pin the surrounding path layer that
//! moved out of the `rge-editor` binary (SCENE-WORLD-BRIDGE dispatch).

use rge_scene_loader::{load_scene_world_from_path, SceneWorldLoadError};

#[test]
fn golden_project_path_yields_two_entities() {
    // Loads the tracked golden simple-scene `.rge-project` through the public
    // path resolver and asserts exactly two entities (Camera + KeyLight),
    // matching the invariant in `tests/simple_scene.rs`. The relative path
    // resolves from `crates/rge-scene-loader` (two levels under the repo root,
    // same as the editor's former test location).
    let project_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project");
    assert!(
        project_path.exists(),
        "tracked golden project must exist at {}",
        project_path.display()
    );
    let world = load_scene_world_from_path(&project_path).expect("load golden simple-scene");
    assert_eq!(
        world.entity_count(),
        2,
        "golden simple-scene must load exactly two entities (Camera + KeyLight)"
    );
}

#[test]
fn unsupported_extension_returns_error() {
    // A bare filename with an unrelated extension exercises the
    // UnsupportedExtension branch without touching the filesystem.
    let result = load_scene_world_from_path(std::path::Path::new("not-a-scene.txt"));
    match result {
        Err(SceneWorldLoadError::UnsupportedExtension(p)) => {
            assert_eq!(p, std::path::PathBuf::from("not-a-scene.txt"));
        }
        other => panic!("expected UnsupportedExtension, got {other:?}"),
    }
}
