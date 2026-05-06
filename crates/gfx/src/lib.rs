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
//! **NOT in this crate (follow-up dispatches):**
//! - Window/surface integration (winit)
//! - Frame-graph (transient resource lifetimes computed at frame begin)
//! - Material registry / pipeline cache (Phase 6.3)
//! - Render-snapshot separation (Phase 6.2)
//! - PBR-proper (BRDF / metallic-roughness / GGX)

#![forbid(unsafe_code)]

pub mod buffer;
pub mod camera;
pub mod context;
pub mod frame;
pub mod light;
pub mod lit_mesh_pipeline;
pub mod material;
pub mod mesh;
pub mod mesh_pipeline;
pub mod pipeline;
pub mod target;
pub mod transform;
pub mod vertex;
pub mod vertex_lit;

pub use buffer::{BufferError, IndexBuffer, VertexBuffer};
pub use camera::{Camera, CameraError};
pub use context::{GfxContext, GfxContextError};
pub use frame::{FrameError, FrameRecorder, ReadbackBuffer};
pub use light::{DirectionalLight, LightError};
pub use lit_mesh_pipeline::{
    record_lit_mesh_pass, LitMesh, LitMeshPipeline, LitMeshPipelineError, LitVertexBuffer,
};
pub use material::{upload_rgba8_srgb_2d, Material, MaterialError, TextureUploadError};
pub use mesh::Mesh;
pub use mesh_pipeline::{MeshPipeline, MeshPipelineError};
pub use pipeline::{PipelineError, TrianglePipeline};
pub use target::{HeadlessTarget, TargetError};
pub use transform::{Transform, TransformError};
pub use vertex::Vertex;
pub use vertex_lit::VertexLit;
