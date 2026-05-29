//! In-app "Open GLB" — Ctrl+O handler + open-dialog trait.
//!
//! Companion to `asset_reload.rs` (the R-key reload axis). This file
//! holds the **fourth keyboard axis**: `Ctrl+O` → prompt the user for a
//! `.glb` via a native file dialog, import it, and swap the GPU-side
//! mesh / material vecs via
//! [`crate::render_path::EditorShell::reload_render_assets`] — the same
//! atomic-swap machinery the R-key path uses.
//!
//! # Design
//!
//! - **Two-trait split.** The "pick a path" step
//!   ([`GlbOpenDialog::pick_glb_path`]) and the "load that path into
//!   render vecs" step ([`super::AssetReloadHook::reload_glb`]) are
//!   distinct traits with distinct binary-owned impls. The dialog impl
//!   owns the `rfd` dependency; the loader impl owns the `rge-io-gltf`
//!   dependency. editor-shell gains NEITHER — it holds only the boxed
//!   `dyn` trait objects and calls through them. This mirrors how the
//!   R-key path keeps the glTF loader edge inside `rge-editor`
//!   (`AssetReloadHook`), and is the standing rule from
//!   `.ai/dispatch.tasks.md` ("Loader stays in `rge-editor`; no
//!   `editor-shell → io-gltf` edge"). The dialog gets the same
//!   treatment so editor-shell never depends on `rfd` either.
//!
//! - **GLB-only.** Opening a `.scene` / `.ron` project would require a
//!   runtime `World`-swap surface on [`EditorShell`] that does not
//!   exist yet (the shell builds a `World` only at construction, via
//!   `with_world`). Inventing that surface is out of scope; scene-open
//!   is a deferred follow-up dispatch gated on a preflight that names
//!   the runtime-swap semantics.
//!
//! - **PIE-state gate.** Open only fires when the shell is in
//!   [`crate::PlayState::Editing`] — consistent with the R-key reload
//!   PIE gate. Pressing Ctrl+O during Playing or Paused warn-logs and
//!   no-ops; a mid-PIE asset swap would conflict with the
//!   snapshot/restore round-trip.
//!
//! - **Commit-after-success ordering.** Unlike the R-key path (which
//!   reloads a path already committed to [`EditorShell::glb_source_path`]),
//!   the Open handler must NOT commit the freshly-picked path until the
//!   load AND the GPU swap have both succeeded. The sequence is:
//!   pick → guard hook + PIE → `reload_glb` → `reload_render_assets` →
//!   only on `Ok(())` assign `glb_source_path`. A malformed picked GLB
//!   fails the swap, the previous frame is correctly retained, AND
//!   `glb_source_path` is left untouched — so a subsequent R-key reload
//!   still targets the last *good* file, never the rejected one. This
//!   is the load-bearing safety property the dispatch correction
//!   pinned (see the failing-candidate test).
//!
//! - **Failure mode.** Every error path (not Editing, no dialog, dialog
//!   cancelled, loader returned `Err`, swap returned `Err`) logs and
//!   returns without mutating render state. The GPU is only mutated by
//!   `reload_render_assets`'s atomic-swap step, which runs after both
//!   the new materials and new lit-meshes have been built; partial
//!   uploads cannot corrupt the live render.
//!
//! - **Watcher limitation.** The `--glb` hot-reload watcher is
//!   binary-owned (`rge-editor::EditorApp.watcher`, a `notify`-backed
//!   `GlbWatcher` rooted at the original `--glb` path); editor-shell
//!   has no watcher and no `notify` dependency. Committing
//!   `glb_source_path` here therefore re-points **only the manual
//!   R-key reload** at the newly opened file — the auto-watcher keeps
//!   observing the original `--glb` directory. Making the watcher
//!   follow an in-app-opened file is a deferred follow-up (it needs
//!   binary-side watcher re-rooting); this handler does NOT touch the
//!   watcher. See the comment at the commit site.

use std::path::PathBuf;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;

/// Loader-callback trait for the in-app "Open GLB" dialog.
///
/// The editor binary (`rge-editor::main`) impls this with `rfd`
/// (`rfd::FileDialog::new().add_filter(..).pick_file()`) and hands an
/// instance to [`EditorShell`] at construction via
/// [`EditorShell::with_glb_open_dialog`]. Keeping the impl in the
/// binary leaves editor-shell free of any `rfd` (or `rge-io-gltf`)
/// dependency — the shell holds only a `Box<dyn GlbOpenDialog>` and
/// calls [`Self::pick_glb_path`] when `Ctrl+O` is pressed. Mirrors the
/// [`super::AssetReloadHook`] split exactly.
///
/// `&self` (not `&mut self`) because the dialog is stateless — each
/// invocation spawns a fresh native dialog. A future stateful dialog
/// (last-directory memory, recent-files) can promote this to
/// `&mut self` without churning the single call site.
pub trait GlbOpenDialog {
    /// Prompt the user for a `.glb` file. Returns `Some(path)` when the
    /// user picked a file, `None` when the dialog was cancelled.
    ///
    /// The returned path is a *candidate* — the shell still imports it
    /// and swaps render assets before committing it as the new
    /// [`EditorShell::glb_source_path`]. A cancelled dialog (`None`)
    /// mutates no editor state.
    fn pick_glb_path(&self) -> Option<PathBuf>;
}

impl EditorShell {
    /// `Ctrl+O` handler — fires from the `WindowEvent::KeyboardInput`
    /// branch in [`Self::window_event`]. Prompts via the
    /// [`GlbOpenDialog`] stashed by [`Self::with_glb_open_dialog`],
    /// imports the picked file via the [`super::AssetReloadHook`]
    /// stashed by [`Self::attach_glb_reload_source`], and hands the
    /// result to
    /// [`crate::render_path::EditorShell::reload_render_assets`].
    ///
    /// # Commit-after-success ordering
    ///
    /// [`Self::glb_source_path`] is assigned **only** after the load
    /// and the swap have both returned `Ok`. This deliberately does NOT
    /// delegate to [`Self::handle_asset_reload`] (which would require
    /// pre-setting `glb_source_path` before the swap, leaving it
    /// pointing at a rejected file on failure). See the module-level
    /// "Commit-after-success ordering" note.
    ///
    /// All failure paths log and no-op (render state + `glb_source_path`
    /// untouched, previous frame retained):
    /// - `play_state() != Editing` — Open is disallowed during PIE
    ///   (warn-log; consistent with the R-key gate).
    /// - `open_dialog` is `None` — no dialog was attached (warn-log;
    ///   defensive — the binary attaches one in every launch mode).
    /// - `pick_glb_path()` returned `None` — the user cancelled the
    ///   dialog (info-log; NO mutation).
    /// - `reload_hook` is `None` — no loader was attached (warn-log;
    ///   defensive).
    /// - Hook returned `Err` — the picked file is malformed / missing;
    ///   the previous frame stays and `glb_source_path` is UNCHANGED.
    /// - `reload_render_assets` returned `Err` — a length-invariant
    ///   violation or GPU-upload failure; previous frame stays
    ///   (atomic swap) and `glb_source_path` is UNCHANGED.
    ///
    /// Public so headless tests can drive Open without synthesizing a
    /// winit `KeyEvent`; production usage routes through the
    /// `WindowEvent::KeyboardInput` branch.
    pub fn handle_open_request(&mut self) {
        // (a) PIE gate — Open only fires in Editing, mirroring the
        //     R-key reload gate.
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::open_request",
                play_state = %self.play_state(),
                "Ctrl+O ignored: PIE active, open only fires in Editing"
            );
            return;
        }

        // (b) Dialog presence — defensive; the binary attaches a
        //     dialog in every launch mode.
        let Some(dialog) = self.open_dialog.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::open_request",
                "Ctrl+O ignored: no open_dialog attached (missing with_glb_open_dialog)"
            );
            return;
        };

        // (c) Prompt the user. `None` == cancelled → no mutation.
        let Some(candidate) = dialog.pick_glb_path() else {
            tracing::info!(
                target: "rge::editor-shell::open_request",
                "open cancelled (dialog returned no path); editor state unchanged"
            );
            return;
        };

        // (d) Loader presence + import. Borrow `reload_hook` and run
        //     the import inside a scoped block so the immutable borrow
        //     ends before the `&mut self` calls below — mirroring how
        //     `handle_asset_reload` scopes its borrows. `candidate` is
        //     already owned (the dialog returned it by value), so it
        //     survives the borrow boundary without an extra clone.
        let hook_result = {
            let Some(hook) = self.reload_hook.as_ref() else {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    "Ctrl+O ignored: no reload_hook attached"
                );
                return;
            };
            hook.reload_glb(&candidate)
        };

        let (meshes, base_colors, textures) = match hook_result {
            Ok(triple) => triple,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    error = %e,
                    "hook.reload_glb failed; retaining previous frame, glb_source_path unchanged"
                );
                return;
            }
        };

        // (e) Swap render assets, then commit ONLY on success. The
        //     immutable `reload_hook` borrow above has ended, so the
        //     `&mut self` swap call is unambiguous.
        let mesh_count = meshes.len();
        match self.reload_render_assets(meshes, base_colors, textures) {
            Ok(()) => {
                // Commit the new source path — and ONLY now. R-key
                // reloads henceforth target the newly opened file.
                //
                // NOTE: the binary-owned `--glb` `notify` watcher still
                // targets the ORIGINAL `--glb` path; editor-shell has
                // no watcher to re-point. Making the auto-watcher
                // follow an in-app-opened file is a deferred follow-up
                // (it needs binary-side watcher re-rooting); only the
                // manual R-key reload follows `glb_source_path` here.
                self.glb_source_path = Some(candidate.clone());
                tracing::info!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    mesh_count,
                    "open OK; render assets swapped and glb_source_path committed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    error = %e,
                    "reload_render_assets failed; retaining previous frame, glb_source_path unchanged"
                );
            }
        }
    }
}
