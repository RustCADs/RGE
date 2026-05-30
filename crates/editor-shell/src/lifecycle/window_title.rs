//! Window-title surfacing — reflect the open document + dirty state in the
//! winit title bar (EDITOR-WINDOW-TITLE).
//!
//! The in-app Save chain (SCENE-SAVE-SUBSTRATE → SCENE-SAVE-WIRING →
//! SCENE-SAVE-SOURCE-PATH → PROJECT-SAVE-WIRING) gave the editor a tracked
//! [`EditorShell::save_source`] (a `.rge-scene` or `.rge-project`) and a
//! Command-Bus dirty flag; this surface makes both visible in the OS window
//! title so the user can see which file `Ctrl+S` writes and whether there are
//! unsaved edits.
//!
//! - [`editor_window_title`] is the pure (no-I/O) title formatter — the only
//!   logic, fully unit-testable.
//! - [`EditorShell::sync_window_title`] reconciles the live title onto the winit
//!   window via `set_title`, only when it changed since the last sync (tracked
//!   by `last_window_title`). It runs once per frame from the
//!   `WindowEvent::RedrawRequested` branch of `window_event`, and no-ops on a
//!   windowless (headless) shell. Mirrors the pure-decision + reconcile shape of
//!   the binary's `glb_watcher_action` / `sync_glb_watcher`.

use std::path::Path;

use crate::lifecycle::EditorShell;

/// The window title for the current save source + dirty state.
///
/// - `Some(path)` with a readable file name →
///   `"{file_name}{dirty} — RGE Editor"` (e.g. `level.rge-scene * — RGE Editor`,
///   or `.rge-project — RGE Editor` for an open project).
/// - `None` (default demo / `--glb` / no source), or a path whose `file_name()`
///   is absent / non-UTF-8 → `"RGE Editor{dirty}"`.
///
/// `dirty` is `" *"` when `is_dirty`, else `""`. Pure — the `set_title`
/// side-effect lives in [`EditorShell::sync_window_title`].
pub(crate) fn editor_window_title(source_path: Option<&Path>, is_dirty: bool) -> String {
    let dirty = if is_dirty { " *" } else { "" };
    match source_path
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    {
        Some(name) => format!("{name}{dirty} — RGE Editor"),
        None => format!("RGE Editor{dirty}"),
    }
}

impl EditorShell {
    /// Reconcile the winit window title with the live
    /// [`Self::save_source`] + the Command-Bus dirty flag.
    ///
    /// Computes [`editor_window_title`] (from [`Self::save_source_path`]) and
    /// pushes it to the window via `set_title` **only when it changed** since
    /// the last sync (tracked by `last_window_title`) — so a redraw whose title
    /// is unchanged costs no `set_title`. No-op when there is no window (headless
    /// `EditorShell::new()` / pre-`resumed`). Called once per frame from the
    /// `WindowEvent::RedrawRequested` branch of [`Self::window_event`].
    pub(crate) fn sync_window_title(&mut self) {
        // No window yet (headless / pre-`resumed`): nothing to retitle.
        let Some(window) = self.window.clone() else {
            return;
        };
        let title = editor_window_title(self.save_source_path(), self.command_bus().is_dirty());
        if self.last_window_title.as_deref() != Some(title.as_str()) {
            window.set_title(&title);
            self.last_window_title = Some(title);
        }
    }
}
