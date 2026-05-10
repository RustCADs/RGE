//! Sub-δ.2 + sub-ε click-handling / selection-highlight path for
//! [`crate::EditorShell`].
//!
//! Split out from `lifecycle.rs` as a pure structural refactor on
//! 2026-05-11 (post Render-backed face-selection chapter close-out).
//! All methods live in `impl EditorShell { … }` blocks here; no new
//! types, no new public API, no visibility changes — Rust resolves
//! the methods across files at compile time.
//!
//! Contents:
//!
//! - [`EditorShell::handle_left_click`] — sub-δ.2 click → picker →
//!   `coord.face_selection` wiring, plus the sub-ε overlay rebuild
//!   hook.
//! - [`EditorShell::rebuild_highlight_overlay`] — sub-ε
//!   `IndexBuffer` rebuild from the first `FaceSelection` in
//!   `coord.face_selection`.

use rge_gfx::IndexBuffer;

use crate::lifecycle::EditorShell;

impl EditorShell {
    /// Handle a left-click event (sub-δ.2 + sub-ε). Composes the most
    /// recent cursor position + current viewport size + the editor camera
    /// into a click ray, picks a face, routes the resulting
    /// [`crate::coord::FaceSelection`] into [`crate::coord::EditorCoord`],
    /// and rebuilds the sub-ε highlight overlay `IndexBuffer` for the
    /// picked face (cleared to `None` on no-hit).
    ///
    /// **v0 single-select clear-on-miss semantics**: a click clears the
    /// existing face_selection set unconditionally and adds the new
    /// selection iff the picker resolves a hit. This matches the
    /// standard CAD convention (Fusion 360, Onshape, FreeCAD) where a
    /// bare click selects exactly one face and a click in empty space
    /// clears the selection. Multi-select via shift / ctrl is a future
    /// dispatch.
    ///
    /// **Sub-ε highlight rebuild**: after the selection lands, the click
    /// handler invokes [`rge_cad_projection::CadProjection::face_triangle_indices`]
    /// for the first selection (deterministic — `FaceSelectionSet` is
    /// backed by a `BTreeSet`) and rebuilds `self.highlight_index_buffer`
    /// from the returned dense `[3i, 3i+1, 3i+2]` indices. Empty index
    /// set (e.g. `face_labels = None` on FilletOp output) clears the
    /// buffer to `None`. Multi-select rendering is parked — only the
    /// first `FaceSelection` is rendered.
    ///
    /// No-op when:
    ///
    /// * `cursor_pos` is `None` (no `CursorMoved` event observed yet),
    /// * `surface_ctx` is `None` (render path not yet initialised — e.g.
    ///   the W03 PIE-only test paths that never enter `resumed`'s
    ///   render-path branch), OR
    /// * `cad_world` / `projection` / `cad_graph` is `None` (no CAD
    ///   scene attached — same condition guarding `init_render_state`).
    ///
    /// Tracing target: `rge::editor-shell::pick`.
    pub(crate) fn handle_left_click(&mut self) {
        // Defensive guards — if any required state is absent, no-op.
        let Some(cursor) = self.cursor_pos else {
            return;
        };
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return;
        };
        let Some(projection) = self.projection.as_ref() else {
            return;
        };
        let Some(cad_world) = self.cad_world.as_ref() else {
            return;
        };
        let Some(cad_graph) = self.cad_graph.as_ref() else {
            return;
        };

        let viewport = [
            surface_ctx.config().width as f32,
            surface_ctx.config().height as f32,
        ];
        let camera_view = self.editor_camera.to_camera_view(viewport);

        // Compute the selection (immutable borrows of self.* end after
        // this binding; the mutable `self.coord` borrow that follows
        // is then unconflicted).
        let selection = crate::camera::pick_face_at(
            &camera_view,
            cursor,
            projection,
            cad_world,
            cad_graph.graph(),
        );

        // v0 single-select clear-on-miss for the picked-face state.
        self.coord.face_selection.clear();
        match selection {
            Some(sel) => {
                self.coord.face_selection.add(sel);
                tracing::info!(
                    target: "rge::editor-shell::pick",
                    "click at ({:.1}, {:.1}): picked entity={:?} face_id={:?}",
                    cursor[0],
                    cursor[1],
                    sel.entity,
                    sel.face_id,
                );
            }
            None => {
                tracing::info!(
                    target: "rge::editor-shell::pick",
                    "click at ({:.1}, {:.1}): no hit; selection cleared",
                    cursor[0],
                    cursor[1],
                );
            }
        }

        // Sub-ε — rebuild the highlight `IndexBuffer` against the picked
        // face. Deterministic first-selection wins; multi-select rendering
        // is parked.
        self.rebuild_highlight_overlay();
    }

    /// Rebuild the sub-ε highlight overlay [`IndexBuffer`] from the first
    /// entry in `self.coord.face_selection` (deterministic — backed by a
    /// `BTreeSet`). Sets `self.highlight_index_buffer = None` when:
    ///
    /// * the selection set is empty (no-hit click), OR
    /// * `face_triangle_indices` returns an empty `Vec` (face_labels None
    ///   / no triangle matched).
    ///
    /// `N >= 2` selections fall through to "first wins" — rendering N
    /// overlays at once is parked. A `tracing::debug!` notes the
    /// deferred case.
    pub(crate) fn rebuild_highlight_overlay(&mut self) {
        // Empty-selection → clear the overlay; no-op otherwise.
        let Some(first) = self.coord.face_selection.iter().next().copied() else {
            self.highlight_index_buffer = None;
            return;
        };
        let n = self.coord.face_selection.len();
        if n >= 2 {
            tracing::debug!(
                target: "rge::editor-shell::pick",
                "highlight: {n} selections held; rendering only the first \
                 (multi-select rendering parked)",
            );
        }

        // Resolve the required render-path state. Missing any piece is a
        // silent no-op — the render path simply skips the overlay.
        let Some(projection) = self.projection.as_ref() else {
            self.highlight_index_buffer = None;
            return;
        };
        let Some(cad_world) = self.cad_world.as_ref() else {
            self.highlight_index_buffer = None;
            return;
        };
        let Some(cad_graph) = self.cad_graph.as_ref() else {
            self.highlight_index_buffer = None;
            return;
        };
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            self.highlight_index_buffer = None;
            return;
        };

        let indices = projection.face_triangle_indices(
            first.entity,
            cad_world,
            cad_graph.graph(),
            first.face_id,
        );
        if indices.is_empty() {
            tracing::info!(
                target: "rge::editor-shell::pick",
                "highlight: no overlay (face_labels None or no triangles match face_id={:?})",
                first.face_id,
            );
            self.highlight_index_buffer = None;
            return;
        }

        match IndexBuffer::new(gfx_ctx, &indices) {
            Ok(ib) => {
                tracing::info!(
                    target: "rge::editor-shell::pick",
                    "highlight: {n_tri} triangles ({m} indices) for entity={:?} face_id={:?}",
                    first.entity,
                    first.face_id,
                    n_tri = indices.len() / 3,
                    m = indices.len(),
                );
                self.highlight_index_buffer = Some(ib);
            }
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::pick",
                    "highlight: IndexBuffer build failed: {e:?}; clearing overlay",
                );
                self.highlight_index_buffer = None;
            }
        }
    }
}
