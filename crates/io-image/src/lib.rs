//! `rge-io-image` — single-source-of-truth raster image importer/exporter.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: image codec failures (unknown magic, malformed PNG/JPEG,
//! unsupported pixel format, EXR/HDR parse error, mip-chain dimension
//! mismatch) are transient and recoverable in-place — the caller surfaces
//! the error to the user, retries with a different codec, or skips the
//! asset. No PIE state is owned by io-image itself; it's a stateless format
//! adapter. Matches pak-format + io-gltf + asset-store (transient I/O /
//! parse failures).
//!
//! Per [`PLAN.md`] §1.6.5 (Import/export authority), this crate is the **only**
//! path for PNG / JPEG / EXR / HDR raster ingestion in RGE. CI lint enforces
//! that no other crate links to `image` or `exr` directly.
//!
//! # Pixel formats
//!
//! All decoded images land in [`Image`], a tagged union over the natively
//! representable pixel formats:
//!
//! | Format    | Storage    | Source codecs                |
//! |-----------|------------|------------------------------|
//! | `Rgba8`   | `Vec<u8>`  | PNG (8-bit), JPEG (RGB)      |
//! | `Rgba16`  | `Vec<u16>` | PNG (16-bit)                 |
//! | `Rgba32F` | `Vec<f32>` | EXR (any precision), HDR     |
//!
//! Codecs decode lossy-up only as needed (e.g. JPEG always 8-bit; PNG bit-depth
//! preserved; EXR/HDR always float). Round-trip fidelity is asserted by tests.
//!
//! # Reading
//!
//! - High-level: [`load_path`] sniffs format from magic bytes and routes.
//! - Per-codec: [`png::load_png`], [`jpeg::load_jpeg`], [`exr::load_exr`],
//!   [`hdr::load_hdr`].
//!
//! # Writing
//!
//! Each codec exposes a `save_*` entry-point. PNG/EXR are lossless;
//! [`jpeg::save_jpeg`] takes a quality parameter; [`hdr::save_hdr`] is
//! lossless float (RGBE-encoded).
//!
//! # Mip chains
//!
//! [`mip_chain::generate_mip_chain`] produces a Vec of [`Image`] from level 0
//! down to 1×1 using a box filter. Typically consumed by GPU upload stages.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::doc_markdown,
    reason = "format-adapter crate: pixel↔float casts and wrapping arithmetic are intrinsic to image codecs (sample bit-depth conversions, premultiplication math); rich error types intentional for authoring diagnostics"
)]

pub mod asset_store_stub;
pub mod error;
pub mod exr;
pub mod format_detect;
pub mod hdr;
pub mod image_data;
pub mod jpeg;
pub mod mip_chain;
pub mod png;

use std::path::Path;

pub use error::{ImageError, Result};
pub use format_detect::{detect_format, ImageFormat};
pub use image_data::{Image, PixelFormat};

/// Load an image from a filesystem path. Format is detected from magic bytes
/// (file extension is **not** consulted — see [`detect_format`]).
pub fn load_path(path: impl AsRef<Path>) -> Result<Image> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(ImageError::Io)?;
    load_bytes(&bytes)
}

/// Load an image from an in-memory byte slice. Format detected from magic.
pub fn load_bytes(bytes: &[u8]) -> Result<Image> {
    match detect_format(bytes) {
        Some(ImageFormat::Png) => png::load_png(bytes),
        Some(ImageFormat::Jpeg) => jpeg::load_jpeg(bytes),
        Some(ImageFormat::OpenExr) => exr::load_exr(bytes),
        Some(ImageFormat::RadianceHdr) => hdr::load_hdr(bytes),
        None => Err(ImageError::UnknownFormat),
    }
}
