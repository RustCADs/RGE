//! Headless smoke tests for the bottom status-bar formatter
//! [`rge_editor_ui::widgets::save_status::save_status_line`]. Pure function —
//! no egui `Context` required. Sibling to `inspector_widget_smoke.rs`.

use rge_editor_state::SaveStatusSnapshot;
use rge_editor_ui::widgets::save_status::save_status_line;

fn snapshot(scene: Option<&str>, dirty: bool) -> SaveStatusSnapshot {
    SaveStatusSnapshot {
        scene_file_name: scene.map(str::to_string),
        is_dirty: dirty,
    }
}

#[test]
fn with_scene_clean_shows_bare_name() {
    let s = snapshot(Some("level.rge-scene"), false);
    assert_eq!(save_status_line(&s), "level.rge-scene");
}

#[test]
fn with_scene_dirty_appends_marker() {
    let s = snapshot(Some("level.rge-scene"), true);
    assert_eq!(save_status_line(&s), "level.rge-scene *");
}

#[test]
fn no_scene_clean_shows_no_scene() {
    let s = snapshot(None, false);
    assert_eq!(save_status_line(&s), "No scene");
}

#[test]
fn no_scene_dirty_appends_marker() {
    // A real state: unsaved edits in a blank / demo / `.glb` / `.rge-project`
    // context, where there is no `.rge-scene` silent-save source yet.
    let s = snapshot(None, true);
    assert_eq!(save_status_line(&s), "No scene *");
}

#[test]
fn default_snapshot_renders_no_scene() {
    // The handoff empty-state default — the status bar must read "No scene"
    // from frame 1 (before any publish).
    assert_eq!(save_status_line(&SaveStatusSnapshot::default()), "No scene");
}

#[test]
fn marker_matches_window_title_convention() {
    // The dirty marker is the SAME " *" the OS window title appends, so the
    // two save-state surfaces read consistently. Pin the exact suffix.
    let dirty = save_status_line(&snapshot(Some("a.rge-scene"), true));
    let clean = save_status_line(&snapshot(Some("a.rge-scene"), false));
    assert_eq!(dirty, format!("{clean} *"));
}

#[test]
fn output_is_single_line_without_leading_whitespace() {
    for scene in [Some("x.rge-scene"), None] {
        for dirty in [false, true] {
            let line = save_status_line(&snapshot(scene, dirty));
            assert!(!line.contains('\n'), "status line must be single-line");
            assert_eq!(line, line.trim_start(), "no leading whitespace");
        }
    }
}
