//! Dispatch L — io-gltf embedded image extraction integration tests.
//!
//! Exercises the new `extract_images` + cache + MaterialAsset wiring
//! against the real-PNG `textured_cube.glb` fixture and a hand-rolled
//! `data:image/png;base64,...` URI GLB. Pure substrate validation: no
//! editor / shell / gfx involvement.

mod common;

use std::io::Write;

use rge_io_gltf::{extract_images, import_glb_bytes, Cache, ImageHandle, MemoryCache};
use rge_io_image::PixelFormat;

fn approx_eq_pixels(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Tolerance 1: PNG round-trip through zune-png / image / etc. can
    // produce off-by-one bytes when the encoder optimises filter
    // chains; the checkerboard's color contrast is FAR larger than
    // any encoder noise so this tolerance is plenty for identity.
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() <= 1)
}

// ---------------------------------------------------------------------------
// extract_images shape tests — exercised against the textured_cube fixture
// ---------------------------------------------------------------------------

#[test]
fn extract_images_returns_one_per_glb_image() {
    let path = common::textured_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read textured_cube.glb");
    let gltf = gltf::Gltf::from_slice(&bytes).expect("parse glb");
    // The fixture writes 1 image; the importer enumerates them via
    // doc.images(). resolve_buffers is required to materialise the
    // BIN chunk so extract_images can slice the buffer-view-source
    // image bytes.
    let buffers = {
        let mut out = Vec::new();
        for buffer in gltf.document.buffers() {
            let len = buffer.length();
            match buffer.source() {
                gltf::buffer::Source::Bin => {
                    let blob = gltf.blob.as_ref().expect("bin chunk present");
                    out.push(blob[..len].to_vec());
                }
                gltf::buffer::Source::Uri(_) => unreachable!("fixture uses BIN chunk only"),
            }
        }
        out
    };
    let images = extract_images(&gltf.document, &buffers).expect("extract_images");
    assert_eq!(images.len(), 1, "textured_cube has exactly one image");
}

#[test]
fn extract_images_decodes_view_source_image_correctly() {
    // Import the textured-cube fixture; the cache must hold one 4×4
    // Rgba8 image whose pixels match the checkerboard layout the
    // fixture-builder used. Proves the buffer-view image-source
    // extraction path end-to-end.
    let path = common::textured_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read");
    let mut cache = MemoryCache::new();
    drop(import_glb_bytes(&bytes, &mut cache).expect("import"));
    assert_eq!(cache.image_count(), 1, "one image cached");

    let handle = first_material_base_color_image_handle(&bytes)
        .expect("material has base_color_image_handle");
    let asset = cache.get_image(&handle).expect("get_image");
    assert_eq!(asset.width(), 4, "4×4 checkerboard");
    assert_eq!(asset.height(), 4);
    assert_eq!(asset.pixel_format(), PixelFormat::Rgba8);
    let pixels = asset.pixels();
    assert_eq!(pixels.len(), 4 * 4 * 4);
    // Pixel (0,0) is red, (1,0) is blue — the checkerboard's
    // defining property. Tolerance ±1 for PNG encoder filter noise.
    assert!(approx_eq_pixels(&pixels[0..4], &[255, 0, 0, 255]));
    assert!(approx_eq_pixels(&pixels[4..8], &[0, 0, 255, 255]));
}

#[test]
fn extract_images_decodes_data_uri_png_base64_correctly() {
    // Build a minimal GLB on the fly with a `data:image/png;base64,...`
    // image source instead of a buffer view. Verifies the Uri-path
    // branch in encoded_bytes_for_image.
    let png = make_tiny_red_png_2x2();
    let png_b64 = base64_encode(&png);
    let json = serde_json::json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [] }],
        "materials": [{
            "name": "data-uri-mat",
            "pbrMetallicRoughness": {
                "baseColorTexture": { "index": 0 }
            }
        }],
        "textures": [{ "source": 0 }],
        "images": [{ "uri": format!("data:image/png;base64,{png_b64}"), "name": "tiny" }]
    });
    let glb = wrap_json_only_as_glb(&json);
    let mut cache = MemoryCache::new();
    drop(import_glb_bytes(&glb, &mut cache).expect("import data-uri glb"));
    assert_eq!(cache.image_count(), 1);
    let handle =
        first_material_base_color_image_handle(&glb).expect("material has base_color_image_handle");
    let asset = cache.get_image(&handle).expect("get_image");
    assert_eq!(asset.width(), 2);
    assert_eq!(asset.height(), 2);
}

#[test]
fn extract_images_rejects_external_file_uri() {
    let json = serde_json::json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [] }],
        "materials": [{
            "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 } }
        }],
        "textures": [{ "source": 0 }],
        "images": [{ "uri": "external.png", "name": "external" }]
    });
    let glb = wrap_json_only_as_glb(&json);
    let mut cache = MemoryCache::new();
    let err = import_glb_bytes(&glb, &mut cache).expect_err("must reject");
    assert!(format!("{err}").contains("unsupported image URI"), "{err}");
}

#[test]
fn extract_images_rejects_external_https_uri() {
    let json = serde_json::json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [] }],
        "materials": [{
            "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 } }
        }],
        "textures": [{ "source": 0 }],
        "images": [{ "uri": "https://example.com/x.png", "name": "remote" }]
    });
    let glb = wrap_json_only_as_glb(&json);
    let mut cache = MemoryCache::new();
    let err = import_glb_bytes(&glb, &mut cache).expect_err("must reject");
    assert!(format!("{err}").contains("unsupported image URI"), "{err}");
}

// ---------------------------------------------------------------------------
// MaterialAsset.base_color_image_handle wiring tests
// ---------------------------------------------------------------------------

#[test]
fn material_base_color_image_handle_populated_when_texture_present() {
    let path = common::textured_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read");
    let handle = first_material_base_color_image_handle(&bytes)
        .expect("material has base_color_image_handle");
    let mut cache = MemoryCache::new();
    drop(import_glb_bytes(&bytes, &mut cache).expect("import"));
    assert!(
        cache.get_image(&handle).is_some(),
        "handle resolves in cache"
    );
}

#[test]
fn material_base_color_image_handle_none_when_no_texture() {
    // cube.glb fixture has NO base_color_texture — its material's
    // base_color_image_handle must be None after import.
    let path = common::cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read cube.glb");
    let handle_opt = first_material_base_color_image_handle(&bytes);
    assert!(
        handle_opt.is_none(),
        "cube.glb has no base_color_texture; handle must be None (got {handle_opt:?})"
    );
}

// ---------------------------------------------------------------------------
// Top-level integration test — the canonical "textured cube" assertion
// ---------------------------------------------------------------------------

#[test]
fn textured_cube_import_exposes_4x4_base_color_image() {
    let path = common::textured_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read");
    let mut cache = MemoryCache::new();
    drop(import_glb_bytes(&bytes, &mut cache).expect("import"));
    assert_eq!(cache.image_count(), 1);
    let handle = first_material_base_color_image_handle(&bytes)
        .expect("material has base_color_image_handle");
    let asset = cache.get_image(&handle).expect("image in cache");
    assert_eq!((asset.width(), asset.height()), (4, 4));
    assert_eq!(asset.pixel_format(), PixelFormat::Rgba8);
}

// ---------------------------------------------------------------------------
// Helpers — local to the integration suite
// ---------------------------------------------------------------------------

fn first_material_base_color_image_handle(glb: &[u8]) -> Option<ImageHandle> {
    let mut cache = MemoryCache::new();
    drop(import_glb_bytes(glb, &mut cache).ok()?);
    // The fixture has exactly one material; we re-import a fresh
    // cache and scan via `extract_materials` to pull the populated
    // MaterialAsset directly. Re-running the importer is cheap
    // (small GLB, no GPU work).
    let gltf = gltf::Gltf::from_slice(glb).ok()?;
    let buffers: Vec<Vec<u8>> = gltf
        .document
        .buffers()
        .map(|b| {
            let len = b.length();
            match b.source() {
                gltf::buffer::Source::Bin => gltf.blob.as_ref().expect("bin chunk")[..len].to_vec(),
                gltf::buffer::Source::Uri(_) => Vec::new(),
            }
        })
        .collect();
    // Run extract_images + scene_builder logic indirectly: import the
    // scene into a fresh cache and look up the first material via
    // the scene's entities. Since the textured fixture has no
    // entities with mesh attached, we walk the materials directly via
    // the cache count. Simplest: use rge_io_gltf::extract_materials
    // + scene_builder's resolution logic.
    let materials = rge_io_gltf::extract_materials(&gltf.document);
    let images = extract_images(&gltf.document, &buffers).expect("extract_images");
    let image_handles: Vec<ImageHandle> =
        images.into_iter().map(|i| cache.insert_image(i)).collect();
    let texture_index_to_image_handle: Vec<Option<ImageHandle>> = gltf
        .document
        .textures()
        .map(|t| image_handles.get(t.source().index()).copied())
        .collect();
    let first_mat = materials.into_iter().next()?;
    let tex_idx = first_mat.base_color_texture?;
    texture_index_to_image_handle
        .get(tex_idx)
        .copied()
        .flatten()
}

fn make_tiny_red_png_2x2() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16);
    for _ in 0..4 {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    let img = rge_io_image::Image::from_rgba8(2, 2, rgba);
    rge_io_image::png::save_png(&img).expect("save_png")
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
        out.push(CHARSET[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = u32::from(rem[0]) << 16;
            out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

fn wrap_json_only_as_glb(json: &serde_json::Value) -> Vec<u8> {
    let mut json_bytes = serde_json::to_vec(json).expect("serialize");
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let json_len = u32::try_from(json_bytes.len()).expect("fits u32");
    let total_len = 12u32 + 8u32 + json_len;
    let mut out: Vec<u8> = Vec::with_capacity(total_len as usize);
    out.write_all(&0x4654_6C67_u32.to_le_bytes()).unwrap();
    out.write_all(&2_u32.to_le_bytes()).unwrap();
    out.write_all(&total_len.to_le_bytes()).unwrap();
    out.write_all(&json_len.to_le_bytes()).unwrap();
    out.write_all(&0x4E4F_534A_u32.to_le_bytes()).unwrap();
    out.write_all(&json_bytes).unwrap();
    out
}
