//! In-app "Save" — Ctrl+S handler + save-dialog / scene-write traits.
//!
//! The save-axis companion to [`open_request`](super::open_request) (the
//! `Ctrl+O` Open axis). `Ctrl+S` reaches [`EditorShell::handle_save_request`]
//! via the [`MarkSaved`](super::EditorKeyCommand::MarkSaved) arm of
//! [`EditorShell::handle_key_command`]: the physical-key decode already maps
//! `Ctrl+S → MarkSaved`, and SCENE-SAVE-WIRING repoints that arm from a pure
//! `mark_saved` bookmark to a real Save-to-disk.
//!
//! # Design (mirrors the Open hook split)
//!
//! - **True Save with Save-As fallback, routed by [`SaveSource`].** `Ctrl+S`
//!   dispatches on [`EditorShell::save_source`]:
//!   - [`SaveSource::Scene`] (a `.rge-scene` was opened / launched, or a prior
//!     Save-As committed one) → write the live `World` straight back to it via
//!     the binary-owned [`SceneSaveHook`]
//!     (`rge_scene_loader::save_scene_world_to_path`) with **no dialog** (silent
//!     overwrite).
//!   - [`SaveSource::Project`] (a literal `.rge-project` was opened / launched)
//!     → write the world back to it via the binary-owned [`ProjectSaveHook`]
//!     (`rge_scene_loader::save_project_world_to_path` — overwrite the first
//!     scene + re-write the manifest) with **no dialog**.
//!   - `None` (blank / demo / `.glb`) → the binary-owned [`SceneSaveDialog`]
//!     prompts (Save-As) and the picked path is committed as a new
//!     [`SaveSource::Scene`] on success.
//!
//!   Either way the Command-Bus saved point is marked
//!   ([`EditorShell::mark_saved_command`]) only on a successful write, clearing
//!   `is_dirty()`. (Save-As to a *new* `.rge-project` tree is still a follow-up;
//!   the dialog produces a `.rge-scene` source.)
//!
//! - **editor-shell owns no file-system / loader edge.** The dialog impl owns
//!   the `rfd` dependency; the scene + project writer impls own
//!   `rge-scene-loader`. editor-shell holds only the boxed `dyn` trait objects
//!   and calls through them — it never gains an `rfd`, `rge-scene-loader`, or
//!   `rge-data` dependency. Mirrors [`GlbOpenDialog`](super::GlbOpenDialog) /
//!   [`SceneOpenHook`](super::SceneOpenHook) exactly.
//!
//! - **Editing-gated.** Save only fires in [`PlayState::Editing`], mirroring the
//!   `Ctrl+O` / R-key reload PIE gate: a mid-Play Save would persist the
//!   transient play-state world, not the edit world.
//!
//! - **Mark-saved only on success.** Cancel, missing dialog, missing hook, and
//!   write-error paths all log and leave `command_bus().is_dirty()` untouched.

use std::path::PathBuf;

use super::SaveSource;
use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;

/// Save-destination dialog for in-app Save (`Ctrl+S`) — the save-axis
/// companion to [`GlbOpenDialog`](super::GlbOpenDialog).
///
/// The editor binary (`rge-editor::main`) impls this with `rfd`
/// (`rfd::FileDialog::new().add_filter(..).save_file()`) and hands an instance
/// to [`EditorShell`] at construction via
/// [`EditorShell::with_scene_save_dialog`]. Keeping the impl in the binary
/// leaves editor-shell free of any `rfd` dependency — the shell holds only a
/// `Box<dyn SceneSaveDialog>` and calls [`Self::pick_save_path`] when `Ctrl+S`
/// is pressed.
///
/// `&self` (not `&mut self`) because the dialog is stateless — each invocation
/// spawns a fresh native dialog. A future stateful dialog (last-directory
/// memory) can promote this to `&mut self` without churning the call site.
pub trait SceneSaveDialog {
    /// Prompt the user for a `*.rge-scene` save destination. Returns
    /// `Some(path)` when the user chose a file, `None` when the dialog was
    /// cancelled (in which case the handler mutates no editor state).
    fn pick_save_path(&self) -> Option<PathBuf>;
}

/// Writer-callback for in-app Save — the save-axis companion to
/// [`SceneOpenHook`](super::SceneOpenHook).
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::save_scene_world_to_path` and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_scene_save_hook`]. Keeping the impl
/// in the binary leaves editor-shell free of any `rge-scene-loader` /
/// `rge-data` dependency — the shell holds only a `Box<dyn SceneSaveHook>` and
/// calls [`Self::save_scene_world`] when the user saves. The hook owns
/// `Scene.name` derivation (v0: the chosen file stem).
///
/// `&self` (not `&mut self`) because the writer is stateless — every save
/// re-extracts from the live world and writes afresh.
pub trait SceneSaveHook {
    /// Write `world` to `path` as a `.rge-scene`.
    ///
    /// On any extension / serialize / I/O failure, return `Err(message)`: the
    /// `Ctrl+S` handler warn-logs it and does NOT mark the bus saved. On `Ok`,
    /// the handler marks the Command-Bus saved point (clearing `is_dirty()`).
    fn save_scene_world(
        &self,
        world: &rge_kernel_ecs::World,
        path: &std::path::Path,
    ) -> Result<(), String>;
}

/// Writer-callback for in-app Save of a `.rge-project` — the project-axis
/// companion to [`SceneSaveHook`].
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::save_project_world_to_path` and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_project_save_hook`]. Keeping the impl
/// in the binary leaves editor-shell free of any `rge-scene-loader` /
/// `rge-data` dependency — the shell holds only a `Box<dyn ProjectSaveHook>` and
/// calls [`Self::save_project_world`] when the user saves an open
/// `.rge-project`.
///
/// `&self` (not `&mut self`) because the writer is stateless — every save
/// re-extracts from the live world and writes afresh.
pub trait ProjectSaveHook {
    /// Write `world` back to the project at `project_path` (overwrite its first
    /// scene + re-write the manifest).
    ///
    /// On any failure, return `Err(message)`: the `Ctrl+S` handler warn-logs it
    /// and does NOT mark the bus saved. On `Ok`, the handler marks the
    /// Command-Bus saved point (clearing `is_dirty()`).
    fn save_project_world(
        &self,
        world: &rge_kernel_ecs::World,
        project_path: &std::path::Path,
    ) -> Result<(), String>;
}

impl EditorShell {
    /// `Ctrl+S` handler — invoked from the
    /// [`MarkSaved`](super::EditorKeyCommand::MarkSaved) arm of
    /// [`Self::handle_key_command`]. Routes the live `World` to disk by the open
    /// [`SaveSource`] and marks the Command-Bus saved point **only** on a
    /// successful write.
    ///
    /// Routing on [`Self::save_source`]:
    /// - [`SaveSource::Scene`] → silent overwrite via the [`SceneSaveHook`]
    ///   stashed by [`Self::with_scene_save_hook`] (**no dialog**).
    /// - [`SaveSource::Project`] → silent write via the [`ProjectSaveHook`]
    ///   stashed by [`Self::with_project_save_hook`] (overwrite the first scene +
    ///   re-write the manifest; **no dialog**).
    /// - `None` → Save-As: the [`SceneSaveDialog`] stashed by
    ///   [`Self::with_scene_save_dialog`] prompts for a `.rge-scene`
    ///   destination; on a successful write the picked path is committed as a
    ///   new [`SaveSource::Scene`] so the next `Ctrl+S` overwrites silently.
    ///
    /// All failure paths log and no-op (the bus saved point / `is_dirty()` and
    /// `save_source` are left untouched):
    /// - `play_state() != Editing` — Save is disallowed during PIE (warn-log;
    ///   consistent with the `Ctrl+O` / R-key gate).
    /// - Save-As with no `save_dialog` attached — warn-log (defensive — the
    ///   binary attaches one in every launch mode).
    /// - `pick_save_path()` returned `None` — the user cancelled (info-log; NO
    ///   mutation).
    /// - The matching writer hook is `None` — none attached (warn-log;
    ///   defensive).
    /// - Hook returned `Err` — the path was rejected / serialize / I/O failed;
    ///   the bus is NOT marked saved and no source is committed.
    ///
    /// Public so headless tests can drive Save without synthesizing a winit
    /// `KeyEvent`; production usage routes through the `Ctrl+S` →
    /// [`MarkSaved`](super::EditorKeyCommand::MarkSaved) → `handle_key_command`
    /// path.
    pub fn handle_save_request(&mut self) {
        // (a) PIE gate — Save only fires in Editing, mirroring the Ctrl+O gate.
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                play_state = %self.play_state(),
                "Ctrl+S ignored: PIE active, save only fires in Editing"
            );
            return;
        }

        // (b) Route by the open SaveSource. Clone it so the `&self` read ends
        //     before the `&mut self` mark / commit below. Scene + Project are
        //     silent writes; None is Save-As (commits a new Scene on success).
        //     The `write_*` helpers warn-log a missing hook / a write error and
        //     return `false`; this router only logs the success + marks saved.
        match self.save_source.clone() {
            Some(SaveSource::Scene(path)) => {
                if self.write_scene_world(&path) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %path.display(),
                        "save OK; .rge-scene overwritten and bus marked saved (is_dirty cleared)"
                    );
                    self.mark_saved_command();
                }
            }
            Some(SaveSource::Project(path)) => {
                if self.write_project_world(&path) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %path.display(),
                        "save OK; .rge-project written (first scene + manifest) and bus marked saved (is_dirty cleared)"
                    );
                    self.mark_saved_command();
                }
            }
            None => {
                // Save-As — no tracked source. Prompt for a `.rge-scene`
                // destination; on a successful write commit it as a new
                // `SaveSource::Scene` so the next `Ctrl+S` overwrites silently.
                let Some(dialog) = self.save_dialog.as_ref() else {
                    tracing::warn!(
                        target: "rge::editor-shell::save_request",
                        "Ctrl+S ignored: no save source and no save_dialog attached (missing with_scene_save_dialog)"
                    );
                    return;
                };
                let Some(picked) = dialog.pick_save_path() else {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        "save cancelled (dialog returned no path); editor state unchanged"
                    );
                    return;
                };
                if self.write_scene_world(&picked) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %picked.display(),
                        "Save-As OK; .rge-scene written, source committed, bus marked saved (is_dirty cleared)"
                    );
                    self.save_source = Some(SaveSource::Scene(picked));
                    self.mark_saved_command();
                }
            }
        }
    }

    /// Write the live world to a `.rge-scene` at `path` via the
    /// [`SceneSaveHook`]. Scopes the `&self` borrows (hook + world) so they end
    /// before any `&mut self` commit in the caller. Returns `true` on a
    /// successful write; `false` (with a warn-log) when no `scene_save_hook` is
    /// attached or the hook returned `Err`. Does NOT mark the bus saved — the
    /// caller owns the success-side mark + source commit.
    fn write_scene_world(&self, path: &std::path::Path) -> bool {
        let Some(hook) = self.scene_save_hook.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                path = %path.display(),
                "Ctrl+S ignored: no scene_save_hook attached (missing with_scene_save_hook)"
            );
            return false;
        };
        match hook.save_scene_world(self.world.kernel(), path) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    error = %e,
                    "scene save failed; bus NOT marked saved, editor state unchanged"
                );
                false
            }
        }
    }

    /// Write the live world back to the `.rge-project` at `path` via the
    /// [`ProjectSaveHook`] (overwrite first scene + manifest). Mirrors
    /// [`Self::write_scene_world`]: returns `true` on success; `false` (with a
    /// warn-log) when no `project_save_hook` is attached or the hook returned
    /// `Err`. Does NOT mark the bus saved.
    fn write_project_world(&self, path: &std::path::Path) -> bool {
        let Some(hook) = self.project_save_hook.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                path = %path.display(),
                "Ctrl+S ignored: no project_save_hook attached (missing with_project_save_hook)"
            );
            return false;
        };
        match hook.save_project_world(self.world.kernel(), path) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    error = %e,
                    "project save failed; bus NOT marked saved, editor state unchanged"
                );
                false
            }
        }
    }
}
