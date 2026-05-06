//! Integration test: renders a red triangle on a black background to a
//! headless 64×64 texture and reads back pixels.
//!
//! On environments without a GPU (CI without a virtual GPU), the test skips
//! gracefully rather than failing.

use rge_gfx::{
    FrameRecorder, GfxContext, GfxContextError, HeadlessTarget, ReadbackBuffer, TrianglePipeline,
};

/// Shared helper: obtain a [`GfxContext`] or print a skip message and return
/// `None` when no GPU adapter is available.
fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(GfxContextError::NoAdapter) => {
            eprintln!("SKIP (no GPU adapter): headless triangle tests skipped");
            None
        }
        Err(e) => panic!("unexpected GfxContext init error: {e}"),
    }
}

#[test]
fn context_init_smoke() {
    let Some(ctx) = ctx_or_skip() else { return };
    let info = ctx.adapter_info();
    eprintln!("adapter: {} ({:?})", info.name, info.backend);
    // No assertion on backend — varies per platform.
}

#[test]
fn renders_a_red_triangle_on_black_background() {
    let Some(ctx) = ctx_or_skip() else { return };

    let target = HeadlessTarget::new(&ctx, 64, 64).expect("target creation");
    let pipeline = TrianglePipeline::new(&ctx, target.format()).expect("pipeline creation");

    let mut frame = FrameRecorder::new(&ctx);
    frame.render_triangle(&target, &pipeline, wgpu::Color::BLACK);
    frame.submit();

    let readback = ReadbackBuffer::from_target(&ctx, &target).expect("readback");

    // (32, 24) is inside the triangle: it should be red.
    // The triangle spans:
    //   top vertex at NDC y=+0.5  → texel y = 64*(1-0.5)/2 = 16
    //   base at NDC y=-0.5        → texel y = 64*(1+0.5)/2 = 48
    //   centre x at NDC x=0       → texel x = 32
    // y=24 is comfortably inside the triangle.
    let center = readback.pixel(32, 24).expect("center pixel");
    assert_eq!(
        center,
        (255, 0, 0, 255),
        "center should be red — got {center:?}"
    );

    // Top-right corner (60, 60) is outside the triangle → should be the clear
    // colour: black.
    let corner = readback.pixel(60, 60).expect("corner pixel");
    assert_eq!(
        corner,
        (0, 0, 0, 255),
        "corner should be black — got {corner:?}"
    );
}

#[test]
fn readback_buffer_pixel_out_of_bounds_returns_none() {
    let Some(ctx) = ctx_or_skip() else { return };

    let target = HeadlessTarget::new(&ctx, 64, 64).expect("target");
    let pipeline = TrianglePipeline::new(&ctx, target.format()).expect("pipeline");

    let mut frame = FrameRecorder::new(&ctx);
    frame.render_triangle(&target, &pipeline, wgpu::Color::BLACK);
    frame.submit();

    let readback = ReadbackBuffer::from_target(&ctx, &target).expect("readback");
    assert!(readback.pixel(64, 0).is_none());
    assert!(readback.pixel(0, 64).is_none());
    assert!(readback.pixel(1000, 1000).is_none());
}
