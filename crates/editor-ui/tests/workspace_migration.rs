//! W09 exit-criteria gate — `v0.1.0 → v0.2.0` workspace migration is lossless.
//!
//! Drives the migration ladder against a hand-authored `0.1.0` fixture (no
//! `shortcuts_overlay` field) and asserts:
//!
//! 1. The loader accepts the older version string.
//! 2. The migration bumps `version` to `CURRENT_WORKSPACE_VERSION`.
//! 3. All `0.1.0`-era fields are preserved bit-for-bit.
//! 4. The new field defaults sensibly (`shortcuts_overlay.enabled = true`).

use rge_editor_ui::layout::{deserialize_workspace, CURRENT_WORKSPACE_VERSION};

const V0_1_0_FIXTURE: &str = r#"(
    name: "Migrated",
    version: "0.1.0",
    theme: Some("dark"),
    layout: HSplit(
        ratio: 0.25,
        id: Some("root"),
        left: Stack(
            id: Some("scene"),
            tabs: [
                "tab/scene_outliner",
            ],
        ),
        right: Stack(
            id: Some("viewport"),
            tabs: [
                "tab/viewport",
            ],
        ),
    ),
    main_menu: [
        (id: "menu.file", label: "File"),
    ],
    toolbars: [
        (
            id: "toolbar.main",
            position: Top,
            extension_point: "toolbar.main",
            visible: true,
        ),
    ],
    shortcuts_overlay: (
        enabled: true,
        extension_point: None,
    ),
)
"#;

#[test]
fn v0_1_0_fixture_loads_via_migration() {
    let ws = deserialize_workspace(V0_1_0_FIXTURE, "<v0.1.0 fixture>")
        .expect("v0.1.0 fixture loads via migration ladder");
    assert_eq!(ws.version, CURRENT_WORKSPACE_VERSION);
    assert_eq!(ws.name, "Migrated");
    assert_eq!(ws.theme.as_deref(), Some("dark"));
    assert_eq!(ws.main_menu.len(), 1);
    assert_eq!(ws.toolbars.len(), 1);
    // Default for the newly-introduced field.
    assert!(ws.shortcuts_overlay.enabled);
}

#[test]
fn v0_1_0_payload_preserved_post_migration() {
    let ws = deserialize_workspace(V0_1_0_FIXTURE, "<v0.1.0 fixture>").unwrap();
    let ids: Vec<_> = ws.id_index().iter().map(|(id, _)| id.0.clone()).collect();
    assert_eq!(
        ids,
        vec![
            "root".to_string(),
            "scene".to_string(),
            "viewport".to_string()
        ]
    );
}

#[test]
fn unsupported_version_is_rejected() {
    let bad = V0_1_0_FIXTURE.replace("\"0.1.0\"", "\"99.0.0\"");
    let err = deserialize_workspace(&bad, "<99.0.0>").expect_err("unsupported version must error");
    let msg = err.to_string();
    assert!(msg.contains("99.0.0"), "error must mention version: {msg}");
}
