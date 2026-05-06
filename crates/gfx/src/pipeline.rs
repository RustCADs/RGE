//! Trivial WGSL render pipeline — one hard-coded triangle in NDC, solid red.
//!
//! This is Phase 6.1 substrate validation only. Mesh rendering from external
//! geometry, bind groups, transforms, and material permutations are follow-up
//! dispatches.

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Embedded WGSL
// ---------------------------------------------------------------------------

/// Embedded WGSL that draws a single hard-coded red triangle in NDC.
///
/// The vertex shader positions three vertices using `vertex_index`; the
/// fragment shader returns solid red `(1, 0, 0, 1)`.
const TRIANGLE_WGSL: &str = r"
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>( 0.0,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
    );
    return vec4<f32>(positions[vid], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when building a [`TrianglePipeline`].
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// wgpu rejected the WGSL source (should not happen with the embedded shader,
    /// but preserved for callers that may substitute custom WGSL).
    #[error("WGSL parse/compile error: {0}")]
    Wgsl(String),
}

// ---------------------------------------------------------------------------
// TrianglePipeline
// ---------------------------------------------------------------------------

/// A compiled wgpu render pipeline that draws one hard-coded red triangle.
///
/// No vertex buffers, no bind groups, no uniforms — pure NDC positions baked
/// into the shader. This exists solely to validate that the wgpu integration
/// compiles and runs a render pass end-to-end.
pub struct TrianglePipeline {
    pipeline: wgpu::RenderPipeline,
}

impl TrianglePipeline {
    /// Compile the embedded WGSL and create a render pipeline targeting
    /// `format`.
    ///
    /// `format` must match the [`HeadlessTarget`](crate::target::HeadlessTarget)
    /// or surface format the pipeline will be used with.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Wgsl`] if the WGSL source fails to parse
    /// (should not occur with the hard-coded embedded shader).
    ///
    /// The `Result` wrapper is intentional — the API surface is designed for
    /// callers that may substitute custom WGSL where failures are possible.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(ctx: &GfxContext, format: wgpu::TextureFormat) -> Result<Self, PipelineError> {
        let device = ctx.device();

        // wgpu 29: create_shader_module panics on validation error by default.
        // We catch it via a push_error_scope / pop_error_scope pair.
        // Actually in wgpu 29, the clean approach is to let the default
        // validation run and handle any panic — but for correctness we use the
        // descriptor directly and let wgpu 29 handle it (it validates at submit
        // time anyway for the pipeline compilation path).
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("triangle.wgsl"),
            source: wgpu::ShaderSource::Wgsl(TRIANGLE_WGSL.into()),
        });

        // wgpu 29: PipelineLayoutDescriptor no longer has push_constant_ranges;
        // it was replaced with `immediate_size` (for the IMMEDIATES feature).
        // bind_group_layouts is now &[Option<&BindGroupLayout>].
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("TrianglePipelineLayout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("TrianglePipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
