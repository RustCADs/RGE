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

use crate::lifecycle::{EditorShell, SaveSource};

/// The window title for the current save source + dirty state.
///
/// - `Some(name)` (the source's [`SaveSource::display_name`]) →
///   `"{name}{dirty} — RGE Editor"` (e.g. `level.rge-scene * — RGE Editor` for a
///   scene, or `my-game — RGE Editor` for an open project — its folder name).
/// - `None` (default demo / `--glb` / no source, or a non-UTF-8 name) →
///   `"RGE Editor{dirty}"`.
///
/// `dirty` is `" *"` when `is_dirty`, else `""`. Pure — the file-name /
/// folder-name extraction lives in [`SaveSource::display_name`]; the `set_title`
/// side-effect lives in [`EditorShell::sync_window_title`].
pub(crate) fn editor_window_title(display_name: Option<&str>, is_dirty: bool) -> String {
    let dirty = if is_dirty { " *" } else { "" };
    match display_name {
        Some(name) => format!("{name}{dirty} — RGE Editor"),
        None => format!("RGE Editor{dirty}"),
    }
}

impl EditorShell {
    /// Reconcile the winit window title with the live
    /// [`Self::save_source`] + the Command-Bus dirty flag.
    ///
    /// Computes [`editor_window_title`] (from the source's
    /// [`SaveSource::display_name`]) and pushes it to the window via `set_title`
    /// **only when it changed** since the last sync (tracked by
    /// `last_window_title`) — so a redraw whose title is unchanged costs no
    /// `set_title`. No-op when there is no window (headless `EditorShell::new()`
    /// / pre-`resumed`). Called once per frame from the
    /// `WindowEvent::RedrawRequested` branch of [`Self::window_event`].
    pub(crate) fn sync_window_title(&mut self) {
        // No window yet (headless / pre-`resumed`): nothing to retitle.
        let Some(window) = self.window.clone() else {
            return;
        };
        let title = editor_window_title(
            self.save_source().and_then(SaveSource::display_name),
            self.command_bus().is_dirty(),
        );
        if self.last_window_title.as_deref() != Some(title.as_str()) {
            window.set_title(&title);
            self.last_window_title = Some(title);
        }
    }
}
