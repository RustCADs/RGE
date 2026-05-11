//! The snapshot-handoff boundary between sim/editor state and the
//! render path (Gate C prerequisite dispatch 1) + the owned /
//! Sendable variant + the latest-only handoff primitive (Gate C
//! prerequisite dispatch 4 per ADR-117).
//!
//! [`RenderInput`] names the non-GPU, sim/editor-side inputs the
//! render path reads per frame OR per resize. The shape is borrowed
//! and read-only — it names the boundary without committing to wire
//! format, threading mechanism, or final ownership.
//!
//! [`RenderInputOwned`] is the `Send + 'static` companion that
//! gets *published* across the sim → render boundary; render
//! consumers re-borrow it through [`RenderInput`] when calling
//! render-path functions.
//!
//! [`RenderHandoff`] is the latest-only immutable snapshot slot per
//! ADR-117 sub-decision 1: sim publishes `Arc<RenderInputOwned>` via
//! [`RenderHandoff::publish`]; render reads the most recently
//! published snapshot via [`RenderHandoff::acquire`]. Older
//! un-acquired snapshots drop to `Arc` strong-count zero on the next
//! publish. The substrate is **synchronization-only**; it does NOT
//! spawn a render thread or change `GfxContext`.
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
//! signatures. Dispatch 1 shipped [`RenderInput`]. Dispatch 4 (this
//! file's extension) ships [`RenderInputOwned`] + [`RenderHandoff`].
//!
//! # Ownership status — load-bearing
//!
//! Per-field ownership of what goes into [`RenderInputOwned`]
//! (camera, light state, projected meshes, material handles, …)
//! beyond the two anchor fields and `editor_camera` is intentionally
//! NOT decided in this dispatch — that is the wire-format ADR's
//! concern (ADR-117 explicit non-decision §1 + future-work §3).
//!
//! # Constraints honored
//!
//! - **No `wgpu::*` types** anywhere in this file: GPU-backed state
//!   stays on [`crate::EditorShell`] (pipeline, materials, surface,
//!   mesh). The snapshot is non-GPU; render-thread GPU state is
//!   downstream of the snapshot.
//! - **No `unsafe`**: `RenderInputOwned: Send + 'static` and
//!   `RenderHandoff: Send + Sync` are satisfied via std primitives
//!   alone (`Arc<Mutex<Option<Arc<_>>>>` + `AtomicU64`).
//! - **No new dependencies**: std-only safe-Rust composition per
//!   ADR-117 sub-decision 5.
//! - **No renderer-thread spawn**: today's renderer continues to run
//!   inline on `WindowEvent::RedrawRequested`. The handoff is
//!   forward-shaped for a future render thread without changing this
//!   module's API.
//! - **No `SnapshotParticipate` impl**: the handoff is in-process
//!   per-frame; PIE participants are per-tick / cross-process and
//!   orthogonal (ADR-117 alternatives table row 6).
//!
//! # See also
//!
//! - PLAN.md §1.5.2 (`(ECS_tick_N, CadCheckpointId_N)` immutability)
//! - PLAN.md §13.6 (Gate C measurability)
//! - `docs/adr/ADR-117-render-handoff-mechanism.md` — the binding
//!   handoff semantics this dispatch implements.
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

// ============================================================
// Owned / Sendable companion — `RenderInputOwned`
// ============================================================

/// Owned, `Send + 'static` snapshot of every non-GPU sim/editor-side
/// input the render path consumes per frame.
///
/// Companion to [`RenderInput`] (borrowed view). `RenderInputOwned`
/// is what gets *published* across the sim → render boundary via
/// [`RenderHandoff`]; render consumers re-borrow it through
/// [`RenderInput`] when calling render-path functions
/// ([`RenderInputOwned::as_render_input`]).
///
/// # Anchor fields (LOAD-BEARING per ADR-117 sub-decision 3)
///
/// - [`Self::ecs_tick`] = value of the kernel-ecs tick at publish-time
/// - [`Self::checkpoint_id`] = value of the cad-projection
///   `CheckpointId` at publish-time
///
/// Together they form the immutable identity pair from PLAN §1.5.2;
/// render-thread immutability is anchored on them. Cross-architecture
/// coherence per PLAN §13.2 / SCENE_EXTRACTION_CONTRACT.md §5.4 is
/// anchored on this pair.
///
/// # Payload field
///
/// `editor_camera` is the only sim/editor-side payload field today
/// (matches the borrowed [`RenderInput`] field set). Expansion lands
/// per-field as new sim fields arrive and is the wire-format ADR's
/// concern (ADR-117 explicit non-decision §1).
///
/// # Thread safety
///
/// All fields are `Send + 'static` (`u64` and `EditorCameraState`
/// which is `Copy`). The `Send + 'static` bound on the whole struct
/// is satisfied automatically — see the compile-time assertion in the
/// boundary test for the proof.
///
/// # Clone cost
///
/// `Clone` is `derive`-cheap (all fields are `Copy`). The typical
/// usage path is `Arc<RenderInputOwned>`, where the snapshot is
/// constructed once per sim publish and shared via reference-count
/// bumps; explicit `Clone` is only needed for ad-hoc sim-side
/// derivations.
#[derive(Clone, Debug)]
pub struct RenderInputOwned {
    /// Value of the kernel-ecs tick at publish-time (anchor field).
    pub ecs_tick: u64,
    /// Value of the cad-projection `CheckpointId` at publish-time
    /// (anchor field).
    pub checkpoint_id: u64,
    /// Editor camera state (sim/editor-side payload).
    pub editor_camera: EditorCameraState,
}

impl RenderInputOwned {
    /// Borrow this owned snapshot as a [`RenderInput<'_>`] for
    /// render-path consumption.
    ///
    /// The returned [`RenderInput`] carries a shared borrow of
    /// [`Self::editor_camera`]; render-path functions that today
    /// consume `&RenderInput<'_>` (e.g. `resize_render_path`) accept
    /// the borrowed view unchanged.
    #[must_use]
    pub fn as_render_input(&self) -> RenderInput<'_> {
        RenderInput {
            editor_camera: &self.editor_camera,
        }
    }
}

// ============================================================
// Latest-only handoff slot — `RenderHandoff`
// ============================================================

/// Latest-only immutable render-input handoff slot per ADR-117.
///
/// Sim-side calls [`Self::publish`] to install a new
/// `Arc<RenderInputOwned>`; render-side calls [`Self::acquire`] to
/// receive the most recently published snapshot. Older un-acquired
/// snapshots are dropped on publish (their `Arc` strong count goes
/// to zero once render releases the previously-acquired snapshot).
///
/// # Semantics (ADR-117 sub-decision 1)
///
/// - **Latest-only**: [`Self::publish`] *replaces* rather than
///   queues. If sim publishes K snapshots between two render frames,
///   the first K-1 drop; render reads only the Kth at next acquire.
/// - **Immutable from publish**: `Arc<RenderInputOwned>` exposes
///   only `&RenderInputOwned`; sim has no path to mutate after
///   publish.
/// - **Non-blocking on both sides**: render NEVER blocks sim; sim
///   NEVER blocks render beyond the trivial mutex-protected swap of
///   a single `Arc` reference (uncontended on the steady-state hot
///   path). Workspace `unsafe_code = "forbid"` policy forecloses the
///   manual `AtomicPtr<_>` variant requiring `Box::from_raw`;
///   `Mutex<Option<Arc<_>>>` is the std-only safe-Rust composition
///   recommended by ADR-117 sub-decision 5.
/// - **Anchored**: each snapshot carries `(ecs_tick, checkpoint_id)`
///   as concrete fields; render reads them off the held snapshot to
///   feed cross-architecture coherence.
///
/// # `generation()` — O(1) "did sim publish?"
///
/// [`Self::generation`] returns a monotonically advancing `u64`
/// incremented on each publish. Render can poll it without taking
/// the slot mutex to decide whether to re-acquire. The counter is
/// NOT the same as `ecs_tick` or `checkpoint_id` — it is an opaque
/// handoff-internal identifier whose only job is to let render
/// answer "did sim publish since I last looked?" in O(1) without
/// locking.
///
/// # Non-decisions deferred per ADR-117
///
/// This primitive is **synchronization-only**; it does NOT spawn a
/// render thread or change `GfxContext`. Today's renderer continues
/// to run inline on `WindowEvent::RedrawRequested`. A future
/// dispatch can install the actual render thread without changing
/// this API.
pub struct RenderHandoff {
    slot: std::sync::Mutex<Option<std::sync::Arc<RenderInputOwned>>>,
    generation: std::sync::atomic::AtomicU64,
}

impl RenderHandoff {
    /// Construct an empty handoff with no published snapshot and a
    /// generation counter of `0`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: std::sync::Mutex::new(None),
            generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Publish a new snapshot. Drops any previously-published-but-
    /// not-acquired snapshot (latest-only / drop-old semantics per
    /// ADR-117 sub-decision 4). Increments the generation counter
    /// after the slot is updated so an observer that read the
    /// counter sees a slot that is already up-to-date when it
    /// re-acquires.
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (i.e. a previous holder
    /// panicked while holding the lock). Poisoning is treated as a
    /// hard-stop bug; the handoff is single-publisher / single-
    /// consumer v0 (ADR-117 mitigation 1) so poisoning can only come
    /// from a deeper invariant break.
    pub fn publish(&self, snapshot: std::sync::Arc<RenderInputOwned>) {
        let mut guard = self.slot.lock().expect("RenderHandoff slot mutex poisoned");
        *guard = Some(snapshot);
        // Increment AFTER replacing so a render-side observer that
        // reads `generation` and then re-acquires is guaranteed to
        // see the just-published snapshot.
        drop(guard);
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Acquire the most recently published snapshot, or `None` if
    /// nothing has been published yet.
    ///
    /// The slot retains its `Arc` reference so subsequent acquires
    /// within the same generation are cheap (each clones the
    /// `Arc`). Latest-only / drop-old semantics fire only on the
    /// *next* [`Self::publish`] call.
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (see [`Self::publish`]).
    #[must_use]
    pub fn acquire(&self) -> Option<std::sync::Arc<RenderInputOwned>> {
        let guard = self.slot.lock().expect("RenderHandoff slot mutex poisoned");
        guard.clone()
    }

    /// Current generation counter (monotonically advancing on each
    /// publish). Use for cheap "should I re-acquire?" reads without
    /// locking the slot.
    ///
    /// Ordering: `Acquire`. Pairs with the `Release` increment in
    /// [`Self::publish`] so that on a successful re-read of the new
    /// generation, the slot's contents are visible.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Default for RenderHandoff {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RenderHandoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid taking the slot lock from `Debug` to keep the impl
        // panic-free under poisoned-lock conditions. Report only the
        // generation; the slot's contents are inspectable via
        // `acquire()` at the call-site.
        f.debug_struct("RenderHandoff")
            .field("generation", &self.generation())
            .finish_non_exhaustive()
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
