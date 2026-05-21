//! Dispatch N2 — headless visual-test harness.
//!
//! Single public function: [`render_one_frame_to_readback`]. Hands a
//! mutable [`EditorShell`] through the headless render flow (init →
//! depth-view acquire → encode + submit → CPU readback) and returns
//! the pixel buffer. Internally calls the existing `pub(crate)`
//! `init_render_state_headless` + `acquire_depth_view` +
//! `render_frame_to_target` methods — does NOT promote them; the
//! harness is the only externally-visible verb.
//!
//! # Scope
//!
//! - Gated by `#[cfg(any(test, feature = "test-harness"))]` so the
//!   production public API stays unchanged. External crates pull the
//!   feature only via `[dev-dependencies]` (e.g. `rge-editor`'s
//!   `cargo test` enables it; `cargo build` of the bin does not).
//! - Returns `Result<ReadbackBuffer, String>` (not panic-on-error) so
//!   callers can detect no-GPU CI environments via the error message
//!   prefix and skip gracefully — matches the gfx-test `ctx_or_skip!`
//!   posture, exposed as a function instead of a macro.
//! - Dispatch-N1's `visual_smoke.rs` is the canonical internal user;
//!   the dispatch-N2 end-to-end tests in `rge-editor/src/main.rs` are
//!   the canonical external user.
//!
//! # NON-GOALS
//!
//! - No multi-frame harness. One frame, one readback. Multi-frame
//!   acceptance (e.g. animation playback) is a future dispatch.
//! - No present / surface acquire. Headless only.
//! - No golden-image regression. Caller asserts variance / hue;
//!   pixel-byte equality is not the harness's job.
//! - No camera / framing override. Caller-supplied shell drives the
//!   camera state through its construction path (CAD path → cuboid
//!   demo eye; glTF path → `compute_aabb_union` auto-frame).
//! - No `ctx_or_skip!`-style macro export — the `Result` shape lets
//!   each test decide its skip pattern.

use rge_gfx::{HeadlessTarget, ReadbackBuffer};

use crate::lifecycle::EditorShell;
use crate::render_path::DepthViewOutcome;

/// Drive a single headless render frame end-to-end and return the
/// CPU-readback pixel buffer.
///
/// Sequence:
///
/// 1. `shell.init_render_state_headless(format, width, height)` —
///    builds the offscreen `GfxContext` + pipeline + per-mesh
///    materials + uploaded `LitMesh`es. Requires the shell to have
///    been constructed with either a CAD scene
///    (`with_world_projection_graph`) OR a non-empty prebuilt mesh
///    vec (`with_render_meshes` /
///    `with_render_meshes_and_base_colors` /
///    `with_render_meshes_and_base_colors_and_textures`).
/// 2. `HeadlessTarget::new(gfx_ctx, width, height)` — allocates the
///    offscreen color attachment (`Rgba8Unorm`,
///    `RENDER_ATTACHMENT | COPY_SRC`).
/// 3. `shell.acquire_depth_view()` — rotates the transient depth
///    texture pool and builds the resource map. Returns
///    `Err("acquire_depth_view: ...")` if the pool fails or the
///    render state is uninitialized.
/// 4. `shell.render_frame_to_target(target.view(), &depth_view)` —
///    encodes the main pass (no egui pass) and submits.
/// 5. `ReadbackBuffer::from_target(gfx_ctx, &target)` — copies
///    texture to staging buffer, maps, strips row padding, returns
///    tight-packed RGBA8 pixels.
///
/// `GfxContext` is not `Clone`; the body scopes the `&shell.gfx_ctx`
/// borrows around the `&mut shell` calls so the borrow checker keeps
/// init / acquire-depth (mut) and target-create / readback (shared)
/// disjoint.
///
/// # Errors
///
/// Returns `Err(String)` on any failure path with a prefix the caller
/// can pattern-match:
/// - `"init_render_state_headless: ..."` — typically "no GPU adapter"
///   on headless CI; caller skips.
/// - `"HeadlessTarget: ..."` — invalid dimensions (zero / oversized).
/// - `"acquire_depth_view: RecoverableSkip"` — transient texture pool
///   `build_resource_map` failed; caller likely re-runs.
/// - `"acquire_depth_view: Uninitialized"` — render state missing
///   despite successful `init_render_state_headless`; caller bug.
/// - `"render_frame_to_target returned false"` — required render-state
///   fields missing (pipeline / camera / light / materials / meshes).
///   Caller bug.
/// - `"ReadbackBuffer: ..."` — staging-buffer map failed.
pub fn render_one_frame_to_readback(
    shell: &mut EditorShell,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<ReadbackBuffer, String> {
    shell
        .init_render_state_headless(format, width, height)
        .map_err(|e| format!("init_render_state_headless: {e}"))?;

    // Scope 1 — allocate the offscreen target via a temporary shared
    // borrow on `shell.gfx_ctx`. Drops before the mut borrow below.
    let target = {
        let gfx_ctx = shell
            .gfx_ctx
            .as_ref()
            .ok_or("init_render_state_headless succeeded but gfx_ctx is None")?;
        HeadlessTarget::new(gfx_ctx, width, height).map_err(|e| format!("HeadlessTarget: {e:?}"))?
    };

    // Mut borrow — rotates the transient texture pool and builds the
    // resource map.
    let depth_view = match shell.acquire_depth_view() {
        DepthViewOutcome::Acquired(view) => view,
        DepthViewOutcome::RecoverableSkip => {
            return Err("acquire_depth_view: RecoverableSkip".into());
        }
        DepthViewOutcome::Uninitialized => {
            return Err("acquire_depth_view: Uninitialized".into());
        }
    };

    if !shell.render_frame_to_target(target.view(), &depth_view) {
        return Err("render_frame_to_target returned false".into());
    }

    // Scope 2 — readback. `acquire_depth_view`'s mut borrow has
    // ended; `target` outlives both scopes (texture is COPY_SRC, the
    // pixels are submitted in the queue and visible to the readback's
    // second command buffer).
    let gfx_ctx = shell
        .gfx_ctx
        .as_ref()
        .ok_or("gfx_ctx vanished between render and readback")?;
    ReadbackBuffer::from_target(gfx_ctx, &target).map_err(|e| format!("ReadbackBuffer: {e:?}"))
}
