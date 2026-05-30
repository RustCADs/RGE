//! Editor bottom status-bar widget over the shared
//! [`rge_editor_state::SaveStatusSnapshot`] observation aggregator.
//!
//! Sibling to [`crate::widgets::inspector`], shipped in two layers:
//!
//! - [`save_status_line`] — a pure formatter that turns a snapshot into the
//!   single display string. Testable without an egui `Context`; pinned by the
//!   unit tests in `crates/editor-ui/tests/save_status_widget_smoke.rs`.
//! - [`ui`] — the egui render function. Renders the line as a single label.
//!   Pure function, no widget state.
//!
//! # Consistency with the OS window title
//!
//! The dirty marker is `" *"` — the SAME marker
//! `rge_editor_shell`'s `editor_window_title` appends to the OS window title,
//! so the in-app status bar and the title bar read consistently (a scene with
//! unsaved edits shows `level.rge-scene *` in both places).

use rge_editor_state::SaveStatusSnapshot;

/// Build the bottom status-bar display string from the snapshot. Pure
/// function; the same snapshot always produces the same string.
///
/// - `Some(name)` → `"{name}{dirty}"` (e.g. `"level.rge-scene *"`).
/// - `None`       → `"No scene{dirty}"` (e.g. `"No scene *"` — unsaved edits
///   in a blank / demo / `.glb` / `.rge-project` context, where there is no
///   `.rge-scene` silent-save source yet).
///
/// `dirty` is `" *"` when `is_dirty`, else `""`. No leading/trailing
/// whitespace beyond the marker; no embedded newlines — suitable for a single
/// egui label.
#[must_use]
pub fn save_status_line(snapshot: &SaveStatusSnapshot) -> String {
    let dirty = if snapshot.is_dirty { " *" } else { "" };
    match snapshot.scene_file_name.as_deref() {
        Some(name) => format!("{name}{dirty}"),
        None => format!("No scene{dirty}"),
    }
}

/// Render the status line into an egui scope as a single label. Walks
/// [`save_status_line`] and renders the result with `ui.label`. Pure
/// function, no widget state. Mirrors [`crate::widgets::inspector::ui`].
pub fn ui(snapshot: &SaveStatusSnapshot, ui: &mut egui::Ui) {
    ui.label(save_status_line(snapshot));
}
