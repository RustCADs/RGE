# Wave W18 — io-image

> Self-contained agent dispatch. Phase 4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.6.4 (import authority).

## Goal

PNG/JPEG/EXR/HDR import + export via `image` and `exr` crates. Output to asset-store. Mip-chain generation.

## Crate owned

`crates/io-image`.

## Files this wave touches

```
crates/io-image/src/{lib.rs, png.rs, jpeg.rs, exr.rs, hdr.rs, mip_chain.rs, format_detect.rs}
crates/io-image/tests/{round_trip_png.rs, round_trip_jpeg.rs, round_trip_exr.rs, mip_chain_test.rs}
crates/io-image/tests/fixtures/{test.png, test.jpg, test.exr, test.hdr}
```

## Stubs needed

- `asset-store::Cache` trait (W16) — local stub.
- `image` workspace dep (PNG/JPEG/HDR).
- `exr` workspace dep (OpenEXR HDR).

## Implementation order

1. `format_detect.rs` — sniff file magic; route to appropriate decoder.
2. `png.rs` — load PNG → RGBA8 / RGBA16 / RGBA32F (depending on bit depth); save reverse.
3. `jpeg.rs` — load JPEG → RGB8; save reverse with quality param.
4. `exr.rs` — OpenEXR via `exr` crate; HDR float channels.
5. `hdr.rs` — Radiance HDR; .hdr files.
6. `mip_chain.rs` — generate mip levels (box filter or Lanczos); cap at 1×1.
7. Test: round-trip PNG via import + export + import; pixel-exact match (lossless).
8. Test: round-trip JPEG via import + export at quality 95; PSNR > 40dB.
9. Test: round-trip EXR; float values within 1e-5 tolerance.
10. Test: mip chain generation; level-N has dimensions (w/2^N, h/2^N).

## Rustforge prior art (steal-and-adapt)

(none specific — rustforge has no dedicated image-import crate). Greenfield with `image` + `exr`.

## Exit criteria

- Round-trip PNG pixel-exact (lossless format).
- Round-trip JPEG quality 95 PSNR > 40dB.
- Round-trip EXR float within 1e-5.
- Mip chain dimensions correct down to 1×1.
- Format detection from file magic (no extension reliance).
- `cargo test -p rge-io-image` passes.

## Duration estimate

1 day.

## Anti-pattern check

PASS — `io-image` is the only import path for raster image formats. CI lint enforces.

## Handoff

After merge: W17 io-gltf consumes `io-image` for embedded textures; material editor (post-W18) loads textures via this path.
