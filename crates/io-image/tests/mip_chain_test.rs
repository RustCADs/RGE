//! Mip-chain integration tests.
//!
//! Spec exit: dimensions correct down to 1×1 for all supported formats.

mod common;

use rge_io_image::mip_chain::generate_mip_chain;
use rge_io_image::PixelFormat;

#[test]
fn power_of_two_chain_dimensions() {
    let img = common::synthetic_rgba8(); // 16x16
    let chain = generate_mip_chain(&img).unwrap();
    let dims: Vec<(u32, u32)> = chain.iter().map(|i| (i.width, i.height)).collect();
    assert_eq!(dims, vec![(16, 16), (8, 8), (4, 4), (2, 2), (1, 1)]);
}

#[test]
fn rectangular_chain_descends_to_1x1() {
    // Non-square: 16x4 → 8x2 → 4x1 → 2x1 → 1x1.
    let mut pixels = vec![0u8; 16 * 4 * 4];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = 50;
        chunk[1] = 100;
        chunk[2] = 150;
        chunk[3] = 200;
    }
    let img = rge_io_image::Image::from_rgba8(16, 4, pixels);
    let chain = generate_mip_chain(&img).unwrap();
    let last = chain.last().unwrap();
    assert_eq!((last.width, last.height), (1, 1));
}

#[test]
fn rgba16_chain() {
    let img = common::synthetic_rgba16(); // 8x8
    let chain = generate_mip_chain(&img).unwrap();
    let dims: Vec<(u32, u32)> = chain.iter().map(|i| (i.width, i.height)).collect();
    assert_eq!(dims, vec![(8, 8), (4, 4), (2, 2), (1, 1)]);
    for level in &chain {
        assert_eq!(level.pixel_format, PixelFormat::Rgba16);
    }
}

#[test]
fn rgba32f_chain() {
    let img = common::synthetic_rgba32f(); // 8x8
    let chain = generate_mip_chain(&img).unwrap();
    let dims: Vec<(u32, u32)> = chain.iter().map(|i| (i.width, i.height)).collect();
    assert_eq!(dims, vec![(8, 8), (4, 4), (2, 2), (1, 1)]);
    for level in &chain {
        assert_eq!(level.pixel_format, PixelFormat::Rgba32F);
    }
}

#[test]
fn one_by_one_chain_is_singleton() {
    let img = rge_io_image::Image::zeros(1, 1, PixelFormat::Rgba8);
    let chain = generate_mip_chain(&img).unwrap();
    assert_eq!(chain.len(), 1);
}
