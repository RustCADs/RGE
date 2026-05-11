//! The snapshot-handoff boundary between sim/editor state and the
//! render path (Gate C prerequisite dispatch 1).
//!
//! [`RenderInput`] names the non-GPU, sim/editor-side inputs the
//! render path reads per frame OR per resize. The shape is borrowed
//! and read-only — it names the boundary without committing to wire
//! format, threading mechanism, or final ownership.
//!
//! # Why this exists
//!
//! Phase 6 §6.3 Gate C measures "topology mutation during frame
//! doesn't invalidate the render thread" (PLAN.md §13.6, anchored by
//! §1.5.2's `(ECS_tick_N, CadCheckpointId_N)` immutability
//! requirement). To produce that measurement, the snapshot-handoff
//! boundary between sim-side state and render-side state must be
//! structurally enforceable: a Sendable owned variant must be
//! producible from sim-side state without changing render-path call
//! signatures. This file ships the borrowed view-type that *names*
//! the boundary; an owned/Sendable variant is a later dispatch.
//!
//! # Ownership status — load-bearing
//!
//! Field ownership is intentionally undecided. `editor_camera` lives
//! in [`crate::EditorShell`] today and is described here as
//! "current render-coordination input"; whether it ultimately
//! becomes sim-side state, render-thread-coordination state, or gets
//! split is a separate design call deferred to the threading-
//! mechanism ADR (Gate C prerequisite dispatch 3). The presence of
//! a field in this view-type does NOT determine its ownership in the
//! final cross-thread design.
//!
//! # Constraints honored
//!
//! - **No `wgpu::*` types**: GPU-backed state stays on
//!   [`crate::EditorShell`] (pipeline, materials, surface, mesh).
//! - **No mutation**: every field is a shared borrow (`&T`).
//! - **No `Send`/`Sync` discipline**: borrows are not `Send` and
//!   this view-type makes no claim about thread safety. The owned
//!   Sendable variant (future dispatch) will.
//! - **No wire-format trait** (`Serialize`, `Encode`, etc.).
//! - **No threading mechanism** (`Arc`, `Mutex`, channel, lockfree
//!   ring) introduced here.
//!
//! # See also
//!
//! - PLAN.md §1.5.2 (`(ECS_tick_N, CadCheckpointId_N)` immutability)
//! - PLAN.md §13.6 (Gate C measurability)
//! - `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` — render-tier
//!   ingestion contract that this boundary will eventually feed.
//! - `docs/§18/GFX_RENDER_TIER.md` — render-tier authority.

use crate::camera::EditorCameraState;
use crate::lifecycle::EditorShell;

/// Borrowed view of all non-GPU, sim/editor-side inputs the render
/// path consumes today (per frame or per resize).
///
/// Construct via [`RenderInput::from_editor_shell`]. Pass
/// `&RenderInput<'_>` (NOT `&EditorShell`) into render-path
/// functions whose sim-side reads belong on the snapshot side of
/// the boundary. GPU-backed state continues to live on
/// [`EditorShell`] and is accessed via `&self` / `&mut self`.
///
/// # Field set
///
/// Grounded in the actual per-frame + per-resize reads in
/// `crates/editor-shell/src/render_path.rs`:
///
/// - `render_frame` reads NO sim-side fields per frame (all reads
///   are GPU-backed `Option<wgpu::*>` / `Option<rge_gfx::*>`
///   handles).
/// - `resize_render_path` reads exactly one sim-side field —
///   `editor_camera` — to recompute `view*proj` for the new aspect
///   ratio.
/// - `init_render_state` is one-shot and not on the per-frame
///   boundary; its sim-side reads (`cad_world`, `projection`,
///   `cad_entity`) are not part of this view-type's scope.
///
/// As additional per-frame or per-resize sim-side reads are
/// introduced, they should be added here so the boundary stays
/// the single locus of "what crosses the sim/render seam".
///
/// # Lifetimes
///
/// The lifetime parameter `'a` ties this view to the
/// [`EditorShell`] it was constructed from. Construct it ad-hoc at
/// call sites — do not store it.
#[derive(Debug)]
pub struct RenderInput<'a> {
    /// Editor camera — current render-coordination input.
    ///
    /// **Ownership status (LOAD-BEARING, intentionally undecided)**:
    /// today the camera lives on [`EditorShell`] and is read on
    /// resize to compute the `view*proj` matrix for the new aspect
    /// ratio. Whether the camera becomes sim-state, render-thread-
    /// coordination state, or gets split (e.g. authoritative pose
    /// on sim, snapshotted projection on render) is deferred to the
    /// threading-mechanism ADR (Gate C prerequisite dispatch 3). Do
    /// NOT infer ownership from its presence in this view-type.
    pub editor_camera: &'a EditorCameraState,
}

impl<'a> RenderInput<'a> {
    /// Build a [`RenderInput`] view from an [`EditorShell`]
    /// reference.
    ///
    /// The caller is responsible for treating the shell as
    /// read-only for the lifetime of the returned view (Rust's
    /// borrow checker enforces this — `&EditorShell` cannot
    /// coexist with `&mut EditorShell`).
    #[must_use]
    pub fn from_editor_shell(shell: &'a EditorShell) -> Self {
        Self {
            editor_camera: &shell.editor_camera,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_editor_shell_constructs_cleanly_from_default_shell() {
        // Structural test — the boundary view-type exists and is
        // constructible from the default EditorShell. This pins the
        // shape; field-content assertions live in the boundary test.
        let shell = EditorShell::default();
        let _input = RenderInput::from_editor_shell(&shell);
    }
}
