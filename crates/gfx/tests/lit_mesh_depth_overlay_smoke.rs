//! Phase 6 sub-β visual harness — pixel-level Z-fight-prevention
//! claim graduation at the GFX pipeline + render-pass layer.
//!
//! **Partial claim graduation**: this test proves the post-sub-α
//! `LitMeshPipeline::new_with_depth(Some(DepthStateKey { Depth24Plus,
//! depth_write_enabled: false, LessEqual }))` configuration combined
//! with the sub-β-style `depth_stencil_attachment: Some(...)` produces
//! the expected visible rendering behavior under depth attachment for
//! a cuboid + overlay at shared positions — overlay drawn second wins
//! on shared-position pixels; cuboid color remains visible elsewhere.
//! It does NOT exercise the editor-shell production `render_frame`
//! path (winit + `SurfaceContext` + `FrameGraph` +
//! `build_resource_map` substrate ceremony) — that end-to-end
//! verification would require non-winit headless-editor architecture
//! which is explicitly out of scope per the visual-harness inspection
//! dispatch's user guardrail "don't build a broad headless editor
//! architecture just to prove one visual claim."
//!
//! **What this test certifies**:
//! - Pipeline + render-pass behavior at the sub-α/β depth-state
//!   configuration is what sub-β claimed it would be
//! - Render order determines visibility on shared-position pixels
//!   (overlay drawn second wins)
//! - No Z-fight occurs at the pipeline/render-pass level under
//!   `depth_write_enabled: false`
//! - Regression boundary: if the depth-state semantics regress
//!   (e.g., `depth_write_enabled: true` accidentally), this test
//!   fails
//!
//! **What this test does NOT certify**:
//! - `editor-shell::render_frame` end-to-end (substrate plumbing
//!   tested separately by `frame_graph_*` tests; pipeline behavior
//!   tested here)
//! - Production highlight-overlay visuals using cad-core-derived
//!   `ProjectedMesh` (test uses hand-built synthetic mesh)
//! - User-perceived "looks correct" (pixel assertions are
//!   structural, not aesthetic)
//!
//! Mirrors `render_mesh_smoke.rs` precedent: hand-built 1×1×1 cuboid
//! `RenderMesh`, `LitMesh::from_render_mesh`, `HeadlessTarget`,
//! `ReadbackBuffer` for pixel-level assertion. Adds: depth attachment,
//! sub-α `new_with_depth` constructor, two materials (white cuboid +
//! orange overlay), two draws in one render pass with the overlay
//! drawn second.

use std::sync::Arc;

use rge_brep_render::RenderMesh;
use rge_gfx::{
    Camera, DepthStateKey, DirectionalLight, GfxContext, HeadlessTarget, LitMesh, LitMeshPipeline,
    Material, ReadbackBuffer,
};

// ---------------------------------------------------------------------------
// Hardware gate (mirrors render_mesh_smoke.rs::ctx_or_skip)
// ---------------------------------------------------------------------------

fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(_) => {
            eprintln!("SKIP: no GPU adapter — skipping lit_mesh_depth_overlay_smoke test");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Synthetic 1×1×1 cuboid `RenderMesh` (canonical 12-triangle layout)
// ---------------------------------------------------------------------------
//
// Duplicated from `render_mesh_smoke.rs::unit_cuboid_render_mesh` — that
// helper is test-local and not re-exportable. Structural copy; if a
// shared `common/mod.rs` test module is later added across the gfx
// test suite, both call sites should be migrated together.

fn unit_cuboid_render_mesh() -> RenderMesh {
    // 8 unique corners.
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, -0.5], // 0 — NNN
        [0.5, -0.5, -0.5],  // 1 — PNN
        [0.5, 0.5, -0.5],   // 2 — PPN
        [-0.5, 0.5, -0.5],  // 3 — NPN
        [-0.5, -0.5, 0.5],  // 4 — NNP
        [0.5, -0.5, 0.5],   // 5 — PNP
        [0.5, 0.5, 0.5],    // 6 — PPP
        [-0.5, 0.5, 0.5],   // 7 — NPP
    ];
    // 12 triangles in canonical face order (NegZ→PosZ→NegY→PosY→NegX→PosX).
    let indices: Vec<u32> = vec![
        0, 3, 2, 0, 2, 1, // NegZ
        4, 5, 6, 4, 6, 7, // PosZ
        0, 1, 5, 0, 5, 4, // NegY
        3, 7, 6, 3, 6, 2, // PosY
        0, 4, 7, 0, 7, 3, // NegX
        1, 2, 6, 1, 6, 5, // PosX
    ];
    let face_labels: Option<Vec<u64>> = Some(vec![
        0, 0, // NegZ
        1, 1, // PosZ
        2, 2, // NegY
        3, 3, // PosY
        4, 4, // NegX
        5, 5, // PosX
    ]);
    RenderMesh::from_buffers(&positions, &indices, face_labels.as_deref())
}

// ---------------------------------------------------------------------------
// Multi-sample assertion helpers (MAIN-RENDER-VISUAL-FIDELITY-002 dispatch
// 2026-05-14): broaden the original single-pixel-per-region assertions to
// catch diagonal-boundary leakage and region inversion. Same color
// thresholds as before; only the sample count grows.
// ---------------------------------------------------------------------------

/// Assert a single pixel falls inside the **overlay** region:
/// orange-dominant (high red, low blue, fully opaque alpha).
fn assert_overlay_pixel(buf: &ReadbackBuffer, x: u32, y: u32) {
    let (r, _g, b, a) = buf.pixel(x, y).expect("pixel in bounds");
    assert_eq!(
        a, 255,
        "alpha should be fully opaque at lit overlay pixel ({x}, {y})"
    );
    assert!(
        r > 80,
        "overlay region red channel should be high (lit orange) at ({x}, {y}); got r={r}"
    );
    assert!(
        b < 80,
        "overlay region blue channel should be LOW (orange has near-zero blue) at ({x}, {y}); got b={b}. \
         If b is high here, the overlay's orange material did NOT win over the cuboid's white — \
         likely depth-state regression or render-order bug."
    );
}

/// Assert a single pixel falls inside the **cuboid-only** region:
/// white-ish (high red AND high blue, fully opaque alpha).
fn assert_cuboid_only_pixel(buf: &ReadbackBuffer, x: u32, y: u32) {
    let (r, _g, b, a) = buf.pixel(x, y).expect("pixel in bounds");
    assert_eq!(
        a, 255,
        "alpha should be fully opaque at lit cuboid-only pixel ({x}, {y})"
    );
    assert!(
        r > 80,
        "cuboid-only region red channel should be high (lit white) at ({x}, {y}); got r={r}"
    );
    assert!(
        b > 80,
        "cuboid-only region blue channel should be HIGH (white has all RGB high) at ({x}, {y}); got b={b}. \
         If b is low here, the overlay's orange material leaked into the cuboid-only region — \
         index-buffer or render-pass-scoping bug."
    );
}

/// Assert a single pixel falls inside the **background** region:
/// near-black (sum of R+G+B is small; clear color = BLACK).
fn assert_background_pixel(buf: &ReadbackBuffer, x: u32, y: u32) {
    let (r, g, b, _a) = buf.pixel(x, y).expect("pixel in bounds");
    let bg_sum = u32::from(r) + u32::from(g) + u32::from(b);
    assert!(
        bg_sum < 30,
        "background pixel at ({x}, {y}) should be near-black (clear color = BLACK); got (r,g,b) = ({r}, {g}, {b})"
    );
}

// ---------------------------------------------------------------------------
// LOAD-BEARING visual harness test
// ---------------------------------------------------------------------------

/// Construct a 1×1×1 cuboid + a single-triangle overlay covering the
/// lower-right half of the cuboid's `+Z` face (the camera-facing
/// face). Both meshes share the same vertex positions. Draw cuboid
/// first with a white material, then overlay second with an orange
/// material, in a SINGLE render pass with a depth attachment at
/// `Depth24Plus` + `LessEqual` + `depth_write_enabled: false` —
/// matching the post-sub-α `LitMeshPipeline::new_with_depth(..,
/// Some(DepthStateKey { Depth24Plus, false, LessEqual }))`
/// configuration that editor-shell production uses post-sub-β.
///
/// Read back HeadlessTarget pixels and assert:
/// 1. Inside the overlay triangle's projected region (lower-right
///    of `+Z`): pixel is ORANGE-DOMINATED (low Blue channel,
///    high Red) — overlay color won render order under
///    `depth_write_enabled: false`.
/// 2. Inside the cuboid-only triangle's projected region (upper-left
///    of `+Z`): pixel is WHITE-ISH (high Blue channel) — cuboid
///    color visible where overlay didn't draw.
/// 3. Outside the cuboid silhouette: pixel is BACKGROUND (near-black).
///
/// If `depth_write_enabled` were `true` on the shared pipeline, the
/// two draws would Z-fight on shared-position pixels — undefined
/// winner. This test pins the post-sub-β semantic: render order +
/// no-depth-write produces deterministic overlay-on-cuboid visibility.
#[test]
fn lit_mesh_depth_overlay_pixel_readback() {
    let Some(ctx) = ctx_or_skip() else {
        return;
    };

    // 1. Hand-build the cuboid RenderMesh + LitMesh.
    let render_mesh = unit_cuboid_render_mesh();
    assert_eq!(
        render_mesh.positions.len(),
        36,
        "RenderMesh::from_buffers expands 8 corners × 12 triangles into 36 flat-shaded positions"
    );
    let target = HeadlessTarget::new(&ctx, 64, 64).expect("headless target");
    let lit_mesh = LitMesh::from_render_mesh(&ctx, &render_mesh).expect("from_render_mesh");

    // 2. Camera: ortho looking down -Z onto the cuboid's +Z face
    //    (mirrors render_mesh_smoke.rs). Cuboid occupies
    //    [-0.5, +0.5]² in NDC; 64×64 viewport → projected silhouette
    //    fills pixels [16..48] × [16..48].
    let camera = Camera::new(&ctx).expect("camera");
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, 5.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    let proj = glam::Mat4::orthographic_rh(-1.0, 1.0, -1.0, 1.0, 0.1, 20.0);
    camera.update(&ctx, proj * view, glam::Mat4::IDENTITY);

    // 3. Light: directed at -Z so +Z face is fully lit.
    let light = DirectionalLight::new(&ctx).expect("light");
    light.update(&ctx, glam::Vec3::new(0.0, 0.0, -1.0), glam::Vec3::ONE);

    // 4. Two materials. White cuboid: default material color.
    //    Orange overlay: explicit color override via `update_color`.
    //    Mirrors editor-shell's HIGHLIGHT_COLOR (1.0, 0.6, 0.0, 1.0)
    //    + HIGHLIGHT_PHONG factors.
    let white_4x4: Vec<u8> = vec![255u8; 4 * 4 * 4];
    let cuboid_material = Material::new(&ctx, &white_4x4, 4, 4).expect("cuboid material");
    let overlay_material = Material::new(&ctx, &white_4x4, 4, 4).expect("overlay material");
    overlay_material.update_color(
        &ctx,
        glam::Vec4::new(1.0, 0.6, 0.0, 1.0),
        glam::Vec4::new(0.1, 1.0, 0.5, 32.0),
    );

    // 5. Pipeline via sub-α `new_with_depth(.., Some(DepthStateKey { ... }))`.
    //    The DepthStateKey matches editor-shell production sub-β EXACTLY:
    //    Depth24Plus + depth_write_enabled: false + LessEqual.
    let depth_state = DepthStateKey::new(
        wgpu::TextureFormat::Depth24Plus,
        false,
        wgpu::CompareFunction::LessEqual,
    );
    let pipeline = LitMeshPipeline::new_with_depth(
        &ctx,
        camera.bind_group_layout(),
        light.bind_group_layout(),
        cuboid_material.bind_group_layout(),
        target.format(),
        Some(depth_state),
    )
    .expect("pipeline with depth");

    // 6. Depth texture allocated directly via wgpu (gfx-level test;
    //    NOT via TexturePool / FrameGraph substrate — that's
    //    production substrate plumbing, tested separately by the
    //    frame_graph_* tests. This harness verifies the pipeline +
    //    render-pass visual behavior under the SAME depth state.).
    let (width, height) = target.dimensions();
    let depth_texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("DepthOverlaySmokeDepth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // 7. Build the overlay index buffer = ONE triangle of the +Z
    //    face. RenderMesh::from_buffers expands triangle 2 (the
    //    first PosZ triangle, input indices [4, 5, 6]) into output
    //    indices [6, 7, 8] — three consecutive vertices in the
    //    flat-shaded vertex buffer. The triangle covers the
    //    LOWER-RIGHT half of the +Z face in NDC (vertices at NDC
    //    (-0.5,-0.5), (+0.5,-0.5), (+0.5,+0.5); diagonal from
    //    top-right to bottom-left). The remaining PosZ triangle
    //    (indices 9, 10, 11 — input [4, 6, 7]) covers the
    //    UPPER-LEFT half and stays "cuboid-only" (not drawn by
    //    overlay).
    use rge_gfx::IndexBuffer;
    let overlay_indices: Vec<u32> = vec![6, 7, 8];
    let overlay_index_buffer =
        IndexBuffer::new(&ctx, &overlay_indices).expect("overlay index buffer");

    // 8. Encode + submit a single render pass with depth attachment
    //    + two draws (cuboid first, overlay second). Mirrors the
    //    editor-shell production inline render-pass body shape
    //    (render_path.rs:244-294).
    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("LitMeshDepthOverlaySmokeEncoder"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("LitMeshDepthOverlaySmokePass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Clear color = BLACK so background pixels are
                    // unambiguously distinguishable from lit cuboid/overlay.
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(pipeline.pipeline());
        pass.set_bind_group(0, camera.bind_group(), &[]);
        pass.set_bind_group(1, light.bind_group(), &[]);
        pass.set_vertex_buffer(0, lit_mesh.vertex_buffer().buffer().slice(..));

        // Draw 1 — full cuboid with white material.
        pass.set_bind_group(2, cuboid_material.bind_group(), &[]);
        let cuboid_ib = lit_mesh.index_buffer().expect("cuboid has index buffer");
        pass.set_index_buffer(cuboid_ib.buffer().slice(..), cuboid_ib.index_format());
        pass.draw_indexed(0..cuboid_ib.index_count(), 0, 0..1);

        // Draw 2 — overlay triangle with orange material. Same
        // vertex buffer (shared positions); only the index buffer
        // and the @group(2) material bind group swap.
        pass.set_bind_group(2, overlay_material.bind_group(), &[]);
        pass.set_index_buffer(
            overlay_index_buffer.buffer().slice(..),
            overlay_index_buffer.index_format(),
        );
        pass.draw_indexed(0..overlay_index_buffer.index_count(), 0, 0..1);
    }
    ctx.queue().submit(std::iter::once(encoder.finish()));
    // `Arc<wgpu::Texture>` capture: keep depth_texture alive past the
    // queue submission so the GPU's command buffer execution can
    // reference it without UAF. (Borrow checker enforces this via the
    // `&depth_view` borrow inside the render-pass scope; documented
    // here for clarity — the test is single-frame so no ring rotation
    // is needed.)
    let _depth_lifetime: Arc<wgpu::Texture> = Arc::new(depth_texture);

    // 9. Read back HeadlessTarget pixels.
    let buf = ReadbackBuffer::from_target(&ctx, &target).expect("readback");

    // 10. Sample pixels and assert.
    //
    // Pixel coordinate system: wgpu viewport Y is +1 at NDC top,
    // -1 at NDC bottom; pixel y=0 is image top, y=height-1 is image
    // bottom (no Y-flip in the readback path — pixel Y matches
    // wgpu's framebuffer Y convention).
    //
    // Cuboid silhouette in NDC = [-0.5, +0.5]² → pixels
    // [16..48] × [16..48].
    //
    // Overlay triangle (input indices 4→5→6; output vertices
    // 6→7→8) covers NDC (-0.5,-0.5) → (+0.5,-0.5) → (+0.5,+0.5)
    // = pixels (16, 48) → (48, 48) → (48, 16) → LOWER-RIGHT half of
    // the +Z face silhouette (bounded by bottom edge, right edge,
    // and the diagonal from top-right (48, 16) to bottom-left (16, 48)).
    // The diagonal line equation in pixel coords is `x + y = 64`;
    // overlay covers pixels where `x + y > 64` inside the silhouette.
    //
    // Cuboid-only triangle (input 4→6→7; output vertices 9→10→11)
    // covers NDC (-0.5,-0.5) → (+0.5,+0.5) → (-0.5,+0.5) = pixels
    // (16, 48) → (48, 16) → (16, 16) → UPPER-LEFT half. Pixels
    // where `x + y < 64` inside the silhouette.
    //
    // MAIN-RENDER-VISUAL-FIDELITY-002 broadens the original
    // single-pixel-per-region assertions to three sample points per
    // region. Samples are chosen well inside each region (margin ≥ 4
    // pixels from any region boundary, especially the diagonal) so
    // the assertions stay stable under the recorder host's headless
    // target and camera. Same color thresholds as before; only the
    // sample count grows.

    // (a) Overlay region (lower-right of diagonal): three sample
    //     points distributed across the region's interior. Each
    //     must show orange-dominant pixels (high red, low blue).
    assert_overlay_pixel(&buf, 40, 40); // center of overlay (40 + 40 = 80, margin 16 past diagonal)
    assert_overlay_pixel(&buf, 44, 24); // upper-right of overlay (44 + 24 = 68, margin 4 past diagonal)
    assert_overlay_pixel(&buf, 38, 44); // lower-center of overlay (38 + 44 = 82, margin 18 past diagonal)

    // (b) Cuboid-only region (upper-left of diagonal): three sample
    //     points. Each must show white-ish pixels (high red AND
    //     high blue).
    assert_cuboid_only_pixel(&buf, 24, 24); // center of cuboid-only (24 + 24 = 48, margin 16 before diagonal)
    assert_cuboid_only_pixel(&buf, 20, 30); // left-center of cuboid-only (20 + 30 = 50, margin 14)
    assert_cuboid_only_pixel(&buf, 30, 20); // upper-center of cuboid-only (30 + 20 = 50, margin 14)

    // (c) Background region (outside silhouette [16..48]×[16..48]):
    //     three sample points at distinct image corners. Each must
    //     show near-black pixels (clear color = BLACK).
    assert_background_pixel(&buf, 4, 4); // top-left corner
    assert_background_pixel(&buf, 60, 4); // top-right corner (x=60 > 48)
    assert_background_pixel(&buf, 4, 60); // bottom-left corner (y=60 > 48)

    // Used the depth lifetime guard; explicit drop after assertions.
    drop(_depth_lifetime);
}
