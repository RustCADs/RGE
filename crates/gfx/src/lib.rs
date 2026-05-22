//! `rge-gfx` — minimal wgpu substrate per IMPLEMENTATION.md Phase 6.1.
//!
//! Failure class: recoverable
//!
//! Substrate-only: Instance/Adapter/Device/Queue, headless render-target factory,
//! trivial WGSL pipeline. GPU init failure or pipeline compile error is recoverable —
//! the editor can fall back to a software path or surface a diagnostic.
//!
//! **Phase 6.1 substrate modules (already shipped):**
//! - [`context`] — Instance/Adapter/Device/Queue
//! - [`target`] — headless RGBA8 render target with `COPY_SRC`
//! - [`pipeline`] — trivial red-triangle validation pipeline
//! - [`frame`] — frame recorder + CPU readback
//!
//! **Phase 6.1 mesh/transform modules:**
//! - [`vertex`] — minimal `position + color` vertex format
//! - [`buffer`] — typed `VertexBuffer` / `IndexBuffer` wrappers
//! - [`mesh`] — `Mesh` = vertex buffer + optional index buffer
//! - [`transform`] — `mat4x4` uniform buffer with bind group
//! - [`mesh_pipeline`] — render pipeline for mesh + transform
//!
//! **Phase 6 PBR-lite modules (this dispatch):**
//! - [`vertex_lit`] — `position + normal + uv` lit vertex format
//! - [`camera`] — `view_proj + normal_matrix` UBO at `@group(0)`
//! - [`light`] — `DirectionalLight` UBO at `@group(1)`
//! - [`material`] — base-colour UBO + texture + sampler at `@group(2)`
//! - [`lit_mesh_pipeline`] — Lambert+Phong render pipeline + `LitMesh` /
//!   `LitVertexBuffer` / `record_lit_mesh_pass`
//!
//! **Phase 6 frame-graph minimal substrate:**
//! - [`frame_graph`] — `Graph<PassNode, ()>` + per-resource lifetime
//!   analysis + transient aliasing groups + deterministic structural
//!   hash. Substrate-only; produces ordering/lifetime metadata an
//!   eventual GPU resource allocator (out of scope) consumes.
//!
//! **Phase 6.3 material-runtime PSO cache substrate:**
//! - [`pso_cache`] — `PipelineCache<T>` keyed on `(ShaderHash,
//!   VertexLayoutDescriptor, ColorFormat, Option<DepthStateKey>)`.
//!   Memoization substrate so N material instances of the same shader +
//!   vertex layout + color target + depth state share one cached
//!   pipeline allocation.
//! - [`intent_adapter`] — `MaterialDescriptor` (from `rge-material-runtime`)
//!   → `(PsoKey, Material)` realisation. [`intent_to_pso_key`] is the total
//!   mapping; [`build_pipeline_from_intent`] routes through `PipelineCache`
//!   so identical descriptors produce 1 insert + N-1 hits. This module
//!   closes the §6.3 "100 material instances share one PSO" exit gate.
//!
//! **NOT in this crate (follow-up dispatches):**
//! - Window/surface integration (winit)
//! - Render-snapshot separation (Phase 6.2 — folded per
//!   `SCENE_EXTRACTION_CONTRACT.md`; runtime/* still stubs)
//! - PBR-proper (BRDF / metallic-roughness / GGX)
//! - Frame-graph integration with `FrameRecorder` / `MeshPipeline` /
//!   `LitMeshPipeline` (substrate stands alone)
//! - Shader graph / Naga linking

#![forbid(unsafe_code)]

pub mod buffer;
pub mod camera;
pub mod context;
pub mod frame;
pub mod frame_graph;
pub mod intent_adapter;
pub mod light;
pub mod lit_mesh_pipeline;
pub mod material;
pub mod mesh;
pub mod mesh_pipeline;
pub mod pipeline;
pub mod plugin_adapter;
pub mod pso_cache;
pub mod surface;
pub mod target;
pub mod transform;
pub mod vertex;
pub mod vertex_lit;

pub use buffer::{BufferError, IndexBuffer, VertexBuffer};
pub use camera::{Camera, CameraError};
pub use context::{GfxContext, GfxContextError};
pub use frame::{FrameError, FrameRecorder, ReadbackBuffer};
pub use frame_graph::{
    build_resource_map, AliasingGroup, AliasingGroupId, BufferDescriptor, BufferPool, CompileError,
    CompiledFrameGraph, FrameGraph, FrameGraphError, PassNode, ResourceClassDescriptor, ResourceId,
    ResourceLifetime, ResourceMap, ResourceMapError, ResourceUsage, TextureDescriptor, TexturePool,
};
pub use intent_adapter::{
    build_pipeline_from_intent, color_target_id_to_format, depth_intent_to_key, intent_to_pso_key,
    shader_id_to_hash, vertex_layout_id_to_descriptor, BuildIntentError, PipelineLayouts,
};
pub use light::{DirectionalLight, LightError};
pub use lit_mesh_pipeline::{
    record_lit_mesh_pass, LitMesh, LitMeshPipeline, LitMeshPipelineError, LitVertexBuffer,
};
pub use material::{upload_rgba8_srgb_2d, Material, MaterialError, TextureUploadError};
pub use mesh::Mesh;
pub use mesh_pipeline::{MeshPipeline, MeshPipelineError};
pub use pipeline::{PipelineError, TrianglePipeline};
pub use plugin_adapter::{GfxPlugin, GFX_PLUGIN_ID};
pub use pso_cache::{DepthStateKey, PipelineCache, PsoKey, ShaderHash, VertexLayoutDescriptor};
pub use surface::{SurfaceContext, SurfaceError};
pub use target::{HeadlessTarget, TargetError};
pub use transform::{Transform, TransformError};
pub use vertex::Vertex;
pub use vertex_lit::VertexLit;

#[cfg(test)]
pub(crate) mod test_lock {
    //! Shared serialization guard for GPU/wgpu-bearing unit tests in this
    //! library test binary.
    //!
    //! The canonical workspace verification gate (`cargo test --workspace
    //! --all-targets --no-fail-fast -j 1`) intermittently abnormally
    //! exited the `rge-gfx --lib` test binary with Windows
    //! `STATUS_ACCESS_VIOLATION (0xc0000005)` AFTER all 180 visible tests
    //! reported `ok`. The cargo test harness runs tests within one binary
    //! on a thread pool, so multiple `#[test]` functions that build a
    //! real `GfxContext` via `ctx_or_skip!()` were initialising and
    //! tearing down their own `wgpu::Device` / `wgpu::Instance` instances
    //! concurrently inside a single process. Concurrent device lifecycle
    //! is the failure source -- the access violation surfaces in the
    //! post-test teardown phase after the test results table has already
    //! printed.
    //!
    //! Tests acquire this guard with
    //! `let _gpu_lock = crate::test_lock::guard();` BEFORE invoking
    //! `ctx_or_skip!()`. Because Rust drops local bindings in reverse
    //! declaration order, the lock outlives the test's `GfxContext`,
    //! serialising both init AND teardown across the entire test binary.
    //!
    //! Any test added later that calls `GfxContext::new_headless()`
    //! (directly or via `ctx_or_skip!()`) MUST also acquire this guard,
    //! or the access violation pattern will re-emerge.
    //!
    //! Mirrors the `GPU_TEST_LOCK` pattern in `editor/rge-editor/src/
    //! main.rs` -- same root cause, same fix, scoped per test binary.

    use std::sync::{Mutex, MutexGuard};

    static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the GPU-test serialization guard. Poisoned mutexes (a
    /// prior panicking GPU test) are recovered so a single failure does
    /// not deadlock the remaining GPU tests in this binary.
    pub(crate) fn guard() -> MutexGuard<'static, ()> {
        GPU_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
}
