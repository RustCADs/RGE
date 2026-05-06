//! Integration tests: mesh rendering with vertex colors and transform UBO.
//!
//! `renders_an_ndc_quad_with_vertex_colors`:
//!   Draws a 2×2 NDC quad (covers the entire clip space) with per-vertex
//!   colors and verifies the four corners match the expected colors.
//!
//! `scale_transform_shrinks_quad_to_center`:
//!   Applies a scale-by-0.5 transform so the quad only covers the central
//!   quarter of the target; the corners must be the clear color (black).

use rge_gfx::{
    mesh_pipeline, GfxContext, GfxContextError, HeadlessTarget, Mesh, MeshPipeline, ReadbackBuffer,
    Transform, Vertex,
};

/// Obtain a [`GfxContext`] or skip gracefully when no GPU is present.
fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(GfxContextError::NoAdapter) => {
            eprintln!("SKIP (no GPU adapter): mesh tests skipped");
            None
        }
        Err(e) => panic!("unexpected GfxContext error: {e}"),
    }
}

#[test]
fn renders_an_ndc_quad_with_vertex_colors() {
    let Some(ctx) = ctx_or_skip() else { return };
    let target = HeadlessTarget::new(&ctx, 64, 64).expect("target");

    // NDC corners: Y is up in NDC (y=+1 is screen top), but in framebuffer
    // coordinates y=0 is screen top.
    let vertices = [
        Vertex::new([-1.0, 1.0, 0.0], [1.0, 0.0, 0.0]), // top-left,     red
        Vertex::new([1.0, 1.0, 0.0], [0.0, 1.0, 0.0]),  // top-right,    green
        Vertex::new([-1.0, -1.0, 0.0], [0.0, 0.0, 1.0]), // bottom-left,  blue
        Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0, 1.0]), // bottom-right, white
    ];
    // Two CCW triangles covering the quad:
    //   0-2-1  (top-left, bottom-left, top-right)
    //   1-2-3  (top-right, bottom-left, bottom-right)
    let indices = [0u32, 2, 1, 1, 2, 3];

    let mesh = Mesh::from_indexed(&ctx, &vertices, &indices).expect("mesh");
    let transform = Transform::new(&ctx).expect("transform");
    transform.update(&ctx, glam::Mat4::IDENTITY);
    let pipeline =
        MeshPipeline::new(&ctx, transform.bind_group_layout(), target.format()).expect("pipeline");

    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mesh-test"),
        });
    mesh_pipeline::record_mesh_pass(
        &mut encoder,
        &target,
        &pipeline,
        &transform,
        &mesh,
        wgpu::Color::BLACK,
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));

    let readback = ReadbackBuffer::from_target(&ctx, &target).expect("readback");

    // Sample each corner (2 px in from each edge to avoid anti-aliased boundary).
    // y=0 in framebuffer is the TOP of the screen.
    let top_left = readback.pixel(2, 2).expect("tl");
    let top_right = readback.pixel(60, 2).expect("tr");
    let bot_left = readback.pixel(2, 60).expect("bl");
    let bot_right = readback.pixel(60, 60).expect("br");

    eprintln!("quad corners — tl:{top_left:?} tr:{top_right:?} bl:{bot_left:?} br:{bot_right:?}");

    // Corners at (2,2) / (60,2) / (2,60) / (60,60) sit slightly inside the
    // rasterised quad edges and pick up ~5–10% of adjacent vertex colour via
    // bilinear interpolation.  Use a dominance test rather than near-saturated
    // per-channel checks: the expected dominant channel must be at least 200
    // (≥78%), and the non-dominant channels must be substantially lower (≤50).
    let (r, g, b) = (top_left.0, top_left.1, top_left.2);
    assert!(
        r >= 200 && g <= 50 && b <= 50,
        "top-left should be predominantly red, got {top_left:?}"
    );

    let (r, g, b) = (top_right.0, top_right.1, top_right.2);
    assert!(
        g >= 200 && r <= 50 && b <= 50,
        "top-right should be predominantly green, got {top_right:?}"
    );

    let (r, g, b) = (bot_left.0, bot_left.1, bot_left.2);
    assert!(
        b >= 200 && r <= 50 && g <= 50,
        "bottom-left should be predominantly blue, got {bot_left:?}"
    );

    // Bottom-right is white: all channels high.
    let (r, g, b) = (bot_right.0, bot_right.1, bot_right.2);
    assert!(
        r >= 200 && g >= 200 && b >= 200,
        "bottom-right should be white, got {bot_right:?}"
    );
}

#[test]
fn scale_transform_shrinks_quad_to_center() {
    let Some(ctx) = ctx_or_skip() else { return };
    let target = HeadlessTarget::new(&ctx, 64, 64).expect("target");

    // All vertices red; we only care about presence/absence of color.
    let vertices = [
        Vertex::new([-1.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        Vertex::new([1.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        Vertex::new([-1.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
        Vertex::new([1.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let indices = [0u32, 2, 1, 1, 2, 3];

    let mesh = Mesh::from_indexed(&ctx, &vertices, &indices).expect("mesh");
    let transform = Transform::new(&ctx).expect("transform");
    // Scale by 0.5: quad spans ±0.5 in NDC → occupies central 32×32 of 64×64 target.
    transform.update(&ctx, glam::Mat4::from_scale(glam::Vec3::splat(0.5)));
    let pipeline =
        MeshPipeline::new(&ctx, transform.bind_group_layout(), target.format()).expect("pipeline");

    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    mesh_pipeline::record_mesh_pass(
        &mut encoder,
        &target,
        &pipeline,
        &transform,
        &mesh,
        wgpu::Color::BLACK,
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));

    let readback = ReadbackBuffer::from_target(&ctx, &target).expect("readback");

    let center = readback.pixel(32, 32).expect("center");
    let corner = readback.pixel(2, 2).expect("corner");

    eprintln!("scale test — center:{center:?} corner:{corner:?}");

    // Center is inside the half-scale quad → should be red.
    assert_eq!(center.0, 255, "center red channel — got {center:?}");
    // Far corner is outside the quad → should be the clear color (black).
    assert_eq!(corner.0, 0, "corner red channel — got {corner:?}");
}
