//! Headless `SaveStatusSnapshot` producer smoke tests for
//! `EditorShell::save_status_snapshot()`. Sibling to
//! `inspector_snapshot_smoke.rs`.
//!
//! Pins: a fresh shell has no scene + is clean; an opened/Save-As scene
//! source surfaces as its bare *file name* (not the full path); the dirty
//! flag mirrors the Command Bus; building the snapshot is a pure read.

use std::path::PathBuf;

use rge_editor_shell::{EditorShell, SaveSource};
use rge_editor_state::SaveStatusSnapshot;

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn fresh_shell_has_no_scene_and_is_clean() {
    let shell = EditorShell::new();
    let s = shell.save_status_snapshot();
    assert_eq!(s.scene_file_name, None, "fresh shell has no scene source");
    assert!(!s.is_dirty, "fresh shell bus is clean");
}

// ---------------------------------------------------------------------------
// Scene-name extraction (file name, not full path)
// ---------------------------------------------------------------------------

#[test]
fn scene_source_surfaces_bare_file_name() {
    // A full path must surface only its file name in the snapshot — the
    // producer pre-extracts via `Path::file_name`.
    let path: PathBuf = PathBuf::from("projects")
        .join("demo")
        .join("level.rge-scene");
    let shell = EditorShell::new().with_save_source(SaveSource::Scene(path));
    let s = shell.save_status_snapshot();
    assert_eq!(
        s.scene_file_name.as_deref(),
        Some("level.rge-scene"),
        "snapshot must carry the file name, not the directory path"
    );
    assert!(!s.is_dirty);
}

#[test]
fn scene_source_matches_save_source_path_accessor() {
    // The snapshot file name must equal what `save_source_path()` reports
    // as a file name — single source of truth.
    let path: PathBuf = PathBuf::from("a").join("b").join("scene.rge-scene");
    let shell = EditorShell::new().with_save_source(SaveSource::Scene(path));
    let from_accessor = shell
        .save_source_path()
        .and_then(std::path::Path::file_name)
        .and_then(|n| n.to_str())
        .map(str::to_string);
    assert_eq!(shell.save_status_snapshot().scene_file_name, from_accessor);
}

// ---------------------------------------------------------------------------
// Dirty-flag interaction
// ---------------------------------------------------------------------------

#[test]
fn dirty_flag_reflects_bus_submit_and_mark_saved() {
    // `set_time_scale` is a real production bus submit source; one non-no-op
    // submit flips is_dirty, and `mark_saved_command()` clears it — the
    // snapshot must follow both transitions.
    let mut shell =
        EditorShell::new().with_save_source(SaveSource::Scene(PathBuf::from("level.rge-scene")));
    assert!(!shell.save_status_snapshot().is_dirty);

    shell.set_time_scale(2.5);
    let dirty = shell.save_status_snapshot();
    assert!(dirty.is_dirty, "non-no-op submit must flip is_dirty");
    assert_eq!(
        dirty.scene_file_name.as_deref(),
        Some("level.rge-scene"),
        "scene name persists across edits"
    );

    shell.mark_saved_command();
    assert!(
        !shell.save_status_snapshot().is_dirty,
        "mark_saved must clear is_dirty in the snapshot"
    );
}

#[test]
fn dirty_without_scene_source_is_a_real_state() {
    // Editing a blank / demo world (no `.rge-scene` source) and dirtying the
    // bus → scene_file_name None + is_dirty true. The formatter renders this
    // as "No scene *".
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.5);
    let s = shell.save_status_snapshot();
    assert_eq!(s.scene_file_name, None);
    assert!(s.is_dirty);
}

// ---------------------------------------------------------------------------
// Purity + trait bounds
// ---------------------------------------------------------------------------

#[test]
fn save_status_snapshot_is_pure_read() {
    let shell =
        EditorShell::new().with_save_source(SaveSource::Scene(PathBuf::from("x.rge-scene")));
    let s1 = shell.save_status_snapshot();
    let s2 = shell.save_status_snapshot();
    assert_eq!(s1, s2, "back-to-back snapshots must be equal (pure read)");
}

#[test]
fn save_status_snapshot_is_clone_send_sync() {
    // Compile-time trait-bound smoke test. SaveStatusSnapshot is Clone (NOT
    // Copy — it carries an owned String) + Send + Sync (so an Arc of it can
    // cross the handoff).
    fn assert_clone_send_sync<T: Clone + Send + Sync + 'static>() {}
    assert_clone_send_sync::<SaveStatusSnapshot>();
}

#[test]
fn save_status_snapshot_default_is_no_scene_clean() {
    let s = SaveStatusSnapshot::default();
    assert_eq!(s.scene_file_name, None);
    assert!(!s.is_dirty);
}
