//! Render pipeline for [`Mesh`] rendering with a [`Transform`] UBO.
//!
//! [`MeshPipeline`] compiles the embedded WGSL shader and wires up:
//! - `@group(0) @binding(0)` — the transform `mat4x4<f32>` uniform
//! - vertex buffers with the [`Vertex`] layout (`position` + `color`)
//!
//! Use [`record_mesh_pass`] to record a render pass into an existing
//! [`wgpu::CommandEncoder`] without touching [`FrameRecorder`].

use crate::context::GfxContext;
use crate::mesh::Mesh;
use crate::target::HeadlessTarget;
use crate::transform::Transform;
use crate::vertex::Vertex;

// ---------------------------------------------------------------------------
// Embedded WGSL
// ---------------------------------------------------------------------------

/// WGSL shader for the mesh pipeline.
///
/// - Vertex stage: multiplies position by the transform matrix, passes color through.
/// - Fragment stage: outputs the interpolated color as opaque RGBA.
///
/// `mat4x4<f32>` is column-major in WGSL, matching [`glam::Mat4::to_cols_array`].
const MESH_WGSL: &str = r"
struct TransformUbo {
    matrix: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> u_transform: TransformUbo;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
};

struct VsOut {
    @builtin(position) clip:  vec4<f32>,
    @location(0)       color: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip  = u_transform.matrix * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when building a [`MeshPipeline`].
#[derive(Debug, thiserror::Error)]
pub enum MeshPipelineError {
    /// The WGSL source failed to parse or compile.
    #[error("WGSL parse error: {0}")]
    Wgsl(String),
}

// ---------------------------------------------------------------------------
// MeshPipeline
// ---------------------------------------------------------------------------

/// A compiled wgpu render pipeline for mesh rendering.
///
/// Takes the [`Vertex`] vertex layout and one bind group at `@group(0)` for
/// the [`Transform`] uniform buffer.
pub struct MeshPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl MeshPipeline {
    /// Compile the embedded WGSL and create the render pipeline.
    ///
    /// `transform_layout` must be the layout returned by
    /// [`Transform::bind_group_layout`].  `color_format` must match the render
    /// target the pipeline will draw into.
    ///
    /// # Errors
    ///
    /// Returns [`MeshPipelineError::Wgsl`] if the embedded WGSL fails to parse
    /// (should not occur with the built-in shader).
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(
        ctx: &GfxContext,
        transform_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
    ) -> Result<Self, MeshPipelineError> {
        let device = ctx.device();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh.wgsl"),
            source: wgpu::ShaderSource::Wgsl(MESH_WGSL.into()),
        });

        // Pipeline layout: one bind group (the transform UBO).
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MeshPipelineLayout"),
            bind_group_layouts: &[Some(transform_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("MeshPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Ok(Self { pipeline })
    }

    /// Borrow the compiled [`wgpu::RenderPipeline`].
    #[must_use]
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

// ---------------------------------------------------------------------------
// record_mesh_pass
// ---------------------------------------------------------------------------

/// Record a single render pass that clears `target` and draws `mesh` with
/// `pipeline` + `transform`, then ends the pass (does **not** submit).
///
/// `clear` is the background colour applied via `LoadOp::Clear`.
///
/// This is a free function so that [`frame::FrameRecorder`] can remain
/// untouched by this phase of work.
///
/// [`frame::FrameRecorder`]: crate::frame::FrameRecorder
pub fn record_mesh_pass(
    encoder: &mut wgpu::CommandEncoder,
    target: &HeadlessTarget,
    pipeline: &MeshPipeline,
    transform: &Transform,
    mesh: &Mesh,
    clear: wgpu::Color,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("MeshPass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    pass.set_pipeline(pipeline.pipeline());
    pass.set_bind_group(0, transform.bind_group(), &[]);
    pass.set_vertex_buffer(0, mesh.vertex_buffer().buffer().slice(..));

    if let Some(ib) = mesh.index_buffer() {
        pass.set_index_buffer(ib.buffer().slice(..), ib.index_format());
        pass.draw_indexed(0..ib.index_count(), 0, 0..1);
    } else {
        pass.draw(0..mesh.vertex_buffer().vertex_count(), 0..1);
    }
}
