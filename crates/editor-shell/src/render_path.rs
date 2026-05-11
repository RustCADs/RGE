//! Sub-δ.1.B + sub-ε render path for [`crate::EditorShell`].
//!
//! Split out from `lifecycle.rs` as a pure structural refactor on
//! 2026-05-11 (post Render-backed face-selection chapter close-out).
//! All methods live in `impl EditorShell { … }` blocks here; no new
//! types, no new public API, no visibility changes — Rust resolves
//! the methods across files at compile time.
//!
//! Contents:
//!
//! - The `DEFAULT_CLEAR` / `WHITE_1X1_RGBA` / `HIGHLIGHT_COLOR` /
//!   `HIGHLIGHT_PHONG` constants + `default_light_direction()` helper.
//! - [`EditorShell::init_render_state`] — wgpu/Surface/Pipeline/Material/
//!   LitMesh/Light/Camera GPU init triggered from `resumed`.
//! - [`EditorShell::render_frame`] — the per-frame encode → set_pipeline
//!   → set_bind_groups → draw_indexed → submit → present sequence
//!   (including the sub-ε overlay's second `draw_indexed`).
//! - [`EditorShell::resize_render_path`] — surface reconfigure on
//!   `WindowEvent::Resized`.

use std::sync::Arc;

use rge_gfx::{
    Camera as GfxCamera, DirectionalLight, GfxContext, LitMesh, LitMeshPipeline, Material,
    SurfaceContext,
};
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;

use crate::lifecycle::EditorShell;
use crate::render_input::RenderInput;

/// Default render-path background color (R, G, B, A) used as the
/// `LoadOp::Clear` value on the surface texture's color attachment.
/// Dark neutral gray — high enough contrast that a Lambert+Phong-shaded
/// cuboid is visible without overpowering its brightness range.
const DEFAULT_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.12,
    g: 0.12,
    b: 0.14,
    a: 1.0,
};

/// Default directional light direction (sub-δ.1.B). Light travels
/// toward `(-1, -1, -1)` (normalised); illuminates the +X / +Y / +Z
/// faces of a cuboid at the origin with distinct shading variations
/// from the camera at `(3, 3, 3)`.
fn default_light_direction() -> glam::Vec3 {
    glam::Vec3::new(-1.0, -1.0, -1.0).normalize()
}

/// Default 1×1 white texture (4 bytes RGBA8Unorm, single texel) used as
/// the placeholder texture for the [`Material`]. The Lambert+Phong
/// shader samples this texture but the default base color is white,
/// so the shading variation comes entirely from the light/normal
/// dot product (no texturing in sub-δ.1.B).
const WHITE_1X1_RGBA: [u8; 4] = [255, 255, 255, 255];

/// Selection-highlight tint for sub-ε visual feedback. Orange. Applied to
/// the second `Material` via [`Material::update_color`] so the overlay
/// `draw_indexed` over the main cuboid uses this color through the existing
/// `LitMeshPipeline`'s Lambert+Phong shader (no shader, no pipeline, and
/// no `Material` struct changes).
///
/// Hard-coded for the first visual-feedback pass; theme / config integration
/// is out of scope for sub-ε.
pub(crate) const HIGHLIGHT_COLOR: glam::Vec4 = glam::Vec4::new(1.0, 0.6, 0.0, 1.0);

/// Phong factors for the highlight material — same shape as `Material::new`'s
/// default `(ambient, diffuse, specular, shininess)` so the shading
/// continuity with the main cuboid is preserved.
const HIGHLIGHT_PHONG: glam::Vec4 = glam::Vec4::new(0.1, 1.0, 0.5, 32.0);

impl EditorShell {
    /// Build the GPU-side render state on first `resumed`.
    ///
    /// Composes (in order):
    /// 1. `winit::Window` → `Arc<Window>`
    /// 2. `GfxContext::new_headless()` (instance / adapter / device / queue)
    /// 3. `SurfaceContext::new(&ctx, Arc<Window>)` (configure + surface)
    /// 4. `Material::new(white 1×1)` / `DirectionalLight::new` / `GfxCamera::new`
    /// 5. `LitMeshPipeline::new(...)` against the surface's color format
    /// 6. `RenderMesh` from `projection.render_mesh_for(entity)` →
    ///    `LitMesh::from_render_mesh`
    /// 7. Update camera UBO with the editor camera's first view*proj
    ///
    /// Returns `Err(...)` only if the GPU-side initialisation fails (no
    /// adapter, no compatible surface format, surface create_surface,
    /// pipeline compile, buffer allocation). The error is propagated up
    /// to `resumed` which logs and continues with a placeholder banner —
    /// existing W03 behaviour is preserved when `cad_world == None`.
    pub(crate) fn init_render_state(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        // Sub-δ.1.B is single-cuboid: bail with a no-op when no CAD scene
        // was attached. This keeps the existing W03 tests' behaviour
        // (resumed is a no-op apart from the ready banner).
        if self.cad_world.is_none() || self.cad_entity.is_none() {
            return Ok(());
        }

        // Step 1 — winit window.
        let attrs = WindowAttributes::default()
            .with_title("RGE Editor")
            .with_inner_size(LogicalSize::new(1024_u32, 768_u32));
        let window = event_loop
            .create_window(attrs)
            .map_err(|e| format!("create_window: {e}"))?;
        let window = Arc::new(window);

        // Step 2 — GfxContext.
        let gfx_ctx = GfxContext::new_headless().map_err(|e| format!("gfx ctx: {e}"))?;

        // Step 3 — SurfaceContext.
        let surface_ctx = SurfaceContext::new(&gfx_ctx, Arc::clone(&window))
            .map_err(|e| format!("surface: {e}"))?;
        let format = surface_ctx.config().format;
        let width = surface_ctx.config().width;
        let height = surface_ctx.config().height;
        let aspect = (width.max(1) as f32) / (height.max(1) as f32);

        // Step 4 — bind groups (camera UBO + material + light).
        let gfx_camera = GfxCamera::new(&gfx_ctx).map_err(|e| format!("gfx camera: {e:?}"))?;
        gfx_camera.update(
            &gfx_ctx,
            self.editor_camera.view_proj(aspect),
            glam::Mat4::IDENTITY,
        );
        let material = Material::new(&gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
            .map_err(|e| format!("material: {e:?}"))?;
        // sub-ε: a second `Material` for the highlight overlay. Same
        // bind-group layout as the main material (so the existing
        // `LitMeshPipeline` accepts it at @group(2)); the UBO is then
        // refreshed with `HIGHLIGHT_COLOR` via `update_color`.
        let highlight_material = Material::new(&gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
            .map_err(|e| format!("highlight material: {e:?}"))?;
        highlight_material.update_color(&gfx_ctx, HIGHLIGHT_COLOR, HIGHLIGHT_PHONG);
        let light = DirectionalLight::new(&gfx_ctx).map_err(|e| format!("light: {e:?}"))?;
        light.update(&gfx_ctx, default_light_direction(), glam::Vec3::ONE);

        // Step 5 — pipeline against the surface's color format.
        let pipeline = LitMeshPipeline::new(
            &gfx_ctx,
            gfx_camera.bind_group_layout(),
            light.bind_group_layout(),
            material.bind_group_layout(),
            format,
        )
        .map_err(|e| format!("pipeline: {e:?}"))?;

        // Step 6 — RenderMesh → LitMesh for the cuboid entity.
        let entity = self.cad_entity.expect("checked above");
        let projection = self.projection.as_ref().expect("checked above");
        let cad_world = self.cad_world.as_ref().expect("checked above");
        let render_mesh = projection
            .render_mesh_for(entity, cad_world)
            .ok_or_else(|| "render_mesh_for returned None for the cuboid entity".to_string())?;
        let cuboid_mesh = LitMesh::from_render_mesh(&gfx_ctx, &render_mesh)
            .map_err(|e| format!("LitMesh::from_render_mesh: {e:?}"))?;

        // Step 7 — stash everything.
        self.window = Some(window);
        self.gfx_ctx = Some(gfx_ctx);
        self.surface_ctx = Some(surface_ctx);
        self.pipeline = Some(pipeline);
        self.gfx_camera = Some(gfx_camera);
        self.material = Some(material);
        self.highlight_material = Some(highlight_material);
        self.light = Some(light);
        self.cuboid_mesh = Some(cuboid_mesh);

        // Kick off the first redraw so the cuboid appears.
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }

        Ok(())
    }

    /// Render one frame on `WindowEvent::RedrawRequested` (sub-δ.1.B).
    ///
    /// Acquires the next surface texture, records a single render pass
    /// that clears to [`DEFAULT_CLEAR`] and draws the cuboid mesh with
    /// the [`LitMeshPipeline`] + camera/light/material bind groups,
    /// presents, and schedules the next redraw.
    ///
    /// Returns `false` when the render path is not initialised (e.g.
    /// `cad_world == None`); caller should fall through to existing
    /// W03 behaviour.
    pub(crate) fn render_frame(&mut self) -> bool {
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return false;
        };
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return false;
        };
        let Some(pipeline) = self.pipeline.as_ref() else {
            return false;
        };
        let Some(gfx_camera) = self.gfx_camera.as_ref() else {
            return false;
        };
        let Some(light) = self.light.as_ref() else {
            return false;
        };
        let Some(material) = self.material.as_ref() else {
            return false;
        };
        let Some(mesh) = self.cuboid_mesh.as_ref() else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };

        // Acquire the next surface texture. Skip the frame on
        // Timeout/Occluded/Outdated/Lost/Validation; request another
        // redraw so the resize handler / wgpu reconfigure can recover.
        // wgpu 29's `get_current_texture` returns the enum
        // `CurrentSurfaceTexture` (NOT `Result<…>`); see
        // wgpu-29.0.3/src/api/surface_texture.rs:55.
        let frame = match surface_ctx.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    "skip frame: {other:?}"
                );
                window.request_redraw();
                return true;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            gfx_ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rge-editor.frame.encoder"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rge-editor.frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(DEFAULT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(pipeline.pipeline());
            pass.set_bind_group(0, gfx_camera.bind_group(), &[]);
            pass.set_bind_group(1, light.bind_group(), &[]);
            pass.set_bind_group(2, material.bind_group(), &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer().buffer().slice(..));

            if let Some(ib) = mesh.index_buffer() {
                pass.set_index_buffer(ib.buffer().slice(..), ib.index_format());
                pass.draw_indexed(0..ib.index_count(), 0, 0..1);
            } else {
                pass.draw(0..mesh.vertex_buffer().vertex_count(), 0..1);
            }

            // Sub-ε — selection highlight overlay. Reuses the same
            // `LitMeshPipeline` + camera/light bind groups + vertex
            // buffer; only swaps the @group(2) material bind group and
            // the index buffer. Purely additive: when either field is
            // `None`, the if-let skips the overlay and the main cuboid
            // renders unchanged.
            //
            // NOTE for future depth-buffer landing: this overlay shares
            // the cuboid's vertex positions (Z-fight at the source-mesh
            // level). When a depth attachment is added, this pipeline
            // will need its `DepthStencilState` set to `LessEqual` with
            // `depth_write_enabled = false` — out of sub-ε scope.
            if let (Some(highlight_mat), Some(highlight_ib)) = (
                self.highlight_material.as_ref(),
                self.highlight_index_buffer.as_ref(),
            ) {
                pass.set_bind_group(2, highlight_mat.bind_group(), &[]);
                pass.set_index_buffer(highlight_ib.buffer().slice(..), highlight_ib.index_format());
                pass.draw_indexed(0..highlight_ib.index_count(), 0, 0..1);
            }
        }

        gfx_ctx.queue().submit(std::iter::once(encoder.finish()));
        frame.present();
        window.request_redraw();
        true
    }

    /// Reconfigure the render-path surface on `WindowEvent::Resized`
    /// (sub-δ.1.B). Updates the camera UBO with a new view*proj matrix
    /// for the new aspect ratio. No-op when render path is not
    /// initialised.
    ///
    /// `render_input` carries the sim/editor-side inputs the render
    /// path consumes on resize — today exactly [`EditorCameraState`].
    /// GPU-backed state (surface, gfx_ctx, gfx_camera UBO) is read /
    /// mutated via `&mut self` as before. See
    /// [`crate::render_input::RenderInput`] for the snapshot-handoff
    /// boundary rationale.
    pub(crate) fn resize_render_path(
        &mut self,
        render_input: &RenderInput<'_>,
        new_w: u32,
        new_h: u32,
    ) {
        if new_w == 0 || new_h == 0 {
            return;
        }
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return;
        };
        if let Some(surface_ctx) = self.surface_ctx.as_mut() {
            surface_ctx.resize(gfx_ctx, new_w, new_h);
        }
        let aspect = (new_w as f32) / (new_h as f32);
        let view_proj = render_input.editor_camera.view_proj(aspect);
        if let Some(camera) = self.gfx_camera.as_ref() {
            camera.update(gfx_ctx, view_proj, glam::Mat4::IDENTITY);
        }
    }
}
