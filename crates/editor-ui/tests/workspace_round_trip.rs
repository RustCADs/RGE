//! W09 exit-criteria gate — RON round-trip is byte-identical.
//!
//! Loads each of the four vendored default workspaces (Default, Animation,
//! Sculpt, Code), serializes them back via `serialize_workspace`, deserializes
//! the result, and asserts the second serialization is byte-identical to the
//! first. This is the canonical-form-stability gate.
//!
//! Note: byte-identity is guaranteed *after one normalising round-trip*. The
//! hand-authored RON in `assets/defaults/*.ron` is human-readable but not
//! necessarily in canonical pretty form; the gate is "load → write → load →
//! write produces the same bytes both times".

use std::path::PathBuf;

use rge_editor_ui::layout::{
    deserialize_workspace, serialize_workspace, write_workspace, DefaultWorkspace,
    CURRENT_WORKSPACE_VERSION,
};

#[test]
fn all_four_default_workspaces_load() {
    for which in DefaultWorkspace::all() {
        let ws = which.load().unwrap_or_else(|e| {
            panic!(
                "default workspace `{}` failed to load: {e}",
                which.filename()
            )
        });
        assert_eq!(
            ws.version,
            CURRENT_WORKSPACE_VERSION,
            "{} declares unexpected version after migration",
            which.filename()
        );
        ws.validate()
            .unwrap_or_else(|e| panic!("workspace `{}` failed validate: {e}", which.filename()));
    }
}

#[test]
fn round_trip_is_byte_identical_for_all_defaults() {
    for which in DefaultWorkspace::all() {
        let ws = which.load().expect("load default");
        let text1 = serialize_workspace(&ws).expect("serialize 1");
        let ws2 = deserialize_workspace(&text1, which.filename()).expect("deserialize 1");
        let text2 = serialize_workspace(&ws2).expect("serialize 2");
        assert_eq!(
            text1,
            text2,
            "round-trip not byte-identical for `{}`",
            which.filename()
        );
    }
}

#[test]
fn write_then_read_preserves_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    for which in DefaultWorkspace::all() {
        let ws = which.load().expect("load");
        let path: PathBuf = dir.path().join(which.filename());
        write_workspace(&ws, &path).expect("write");
        let ws_loaded = rge_editor_ui::layout::read_workspace(&path).expect("read");
        // After one normalising round-trip via disk, the document must be equal.
        assert_eq!(
            ws_loaded,
            ws,
            "disk round-trip lost data for {}",
            which.filename()
        );
    }
}

#[test]
fn id_index_covers_all_named_panes_in_default() {
    let ws = DefaultWorkspace::Default.load().unwrap();
    let ids: Vec<_> = ws.id_index().iter().map(|(id, _)| id.0.clone()).collect();
    // Default workspace has 5 stable ids: root, scene, viewport_split, viewport, inspector.
    assert!(ids.contains(&"root".to_string()));
    assert!(ids.contains(&"scene".to_string()));
    assert!(ids.contains(&"viewport".to_string()));
    assert!(ids.contains(&"inspector".to_string()));
}
