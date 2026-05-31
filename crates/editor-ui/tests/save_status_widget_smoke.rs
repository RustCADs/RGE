//! Headless smoke tests for the bottom status-bar formatter
//! [`rge_editor_ui::widgets::save_status::save_status_line`]. Pure function —
//! no egui `Context` required. Sibling to `inspector_widget_smoke.rs`.

use rge_editor_state::SaveStatusSnapshot;
use rge_editor_ui::widgets::save_status::save_status_line;

fn snapshot(source: Option<&str>, dirty: bool) -> SaveStatusSnapshot {
    SaveStatusSnapshot {
        source_name: source.map(str::to_string),
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
fn no_source_clean_shows_no_file() {
    let s = snapshot(None, false);
    assert_eq!(save_status_line(&s), "No file");
}

#[test]
fn no_source_dirty_appends_marker() {
    // A real state: unsaved edits in a blank / demo / `.glb` context, where
    // there is no save source yet (an open `.rge-scene` / `.rge-project`
    // surfaces its name instead).
    let s = snapshot(None, true);
    assert_eq!(save_status_line(&s), "No file *");
}

#[test]
fn default_snapshot_renders_no_file() {
    // The handoff empty-state default — the status bar must read "No file"
    // from frame 1 (before any publish).
    assert_eq!(save_status_line(&SaveStatusSnapshot::default()), "No file");
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
    for source in [Some("x.rge-scene"), None] {
        for dirty in [false, true] {
            let line = save_status_line(&snapshot(source, dirty));
            assert!(!line.contains('\n'), "status line must be single-line");
            assert_eq!(line, line.trim_start(), "no leading whitespace");
        }
    }
}
