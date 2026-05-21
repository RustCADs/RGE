//! Dispatch L — embedded glTF image extraction.
//!
//! Walks `gltf::Document::images()` and decodes each image into an
//! [`ImageAsset`] (an [`rge_io_image::Image`] wrapper). Two source
//! variants are supported at v0:
//!
//! 1. **`gltf::image::Source::View`** — image bytes live in a buffer
//!    view in the `.glb`'s BIN chunk. We slice
//!    `buffers[view.buffer().index()][view.offset()..view.offset()+view
//!    .length()]` and hand the result to [`rge_io_image::load_bytes`].
//! 2. **`gltf::image::Source::Uri`** with a `data:image/png;base64,...`
//!    or `data:image/jpeg;base64,...` payload — base64-decode then
//!    `load_bytes`. The MIME-type prefix is used to gate which payload
//!    types are accepted; `load_bytes`'s magic-byte sniffer is the
//!    real decoder dispatch.
//!
//! External URIs (`file.png`, `https://example.com/x.png`, etc.) are
//! v0-rejected with a `Schema` error — mirrors the existing
//! external-buffer-URI policy in `import.rs`. M-wave can add a
//! file-loader behind a feature flag if needed.
//!
//! ## Why not just feature-flag `gltf::import`?
//!
//! The `gltf` crate's `import` feature pulls the `image` crate, which
//! itself transitively pulls `cpufeatures 0.3.0` requiring `edition2024`
//! / rustc 1.85+ — the workspace pin won't tolerate it. See
//! [`crate::import`] module docs for the full rationale. We do our own
//! View/URI resolution + delegate decoding to io-image, which IS the
//! workspace's sanctioned raster decoder (PLAN §1.6.5).

use rge_io_image::Image;

use crate::handles::ImageHandle;
use crate::import::BufferData;
use crate::GltfError;

/// Decoded image cached behind an [`ImageHandle`].
///
/// Thin wrapper around [`rge_io_image::Image`] that adds a
/// [`content_hash`](Self::content_hash) helper matching the pattern
/// the other asset families ([`crate::MeshAsset`],
/// [`crate::MaterialAsset`], etc.) use to derive their handles. The
/// inner `Image` is exposed directly via [`Self::inner`] /
/// [`Self::into_inner`] / [`Self::from_inner`] so callers that just
/// want the pixels don't have to learn a new API.
#[derive(Debug, Clone)]
pub struct ImageAsset {
    inner: Image,
}

impl ImageAsset {
    /// Wrap a decoded [`Image`] as an [`ImageAsset`].
    #[must_use]
    pub fn from_inner(inner: Image) -> Self {
        Self { inner }
    }

    /// Borrow the underlying [`Image`].
    #[must_use]
    pub fn inner(&self) -> &Image {
        &self.inner
    }

    /// Consume `self` and return the inner [`Image`].
    #[must_use]
    pub fn into_inner(self) -> Image {
        self.inner
    }

    /// Image width in pixels (mirrors [`Image::width`]).
    #[must_use]
    pub fn width(&self) -> u32 {
        self.inner.width
    }

    /// Image height in pixels (mirrors [`Image::height`]).
    #[must_use]
    pub fn height(&self) -> u32 {
        self.inner.height
    }

    /// Pixel storage format (mirrors [`Image::pixel_format`]).
    #[must_use]
    pub fn pixel_format(&self) -> rge_io_image::PixelFormat {
        self.inner.pixel_format
    }

    /// Raw byte buffer (mirrors [`Image::pixels`]).
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.inner.pixels
    }

    /// Compute the content-hash handle.
    ///
    /// Hashes the canonical byte form: `width` (LE u32), `height` (LE
    /// u32), `pixel_format` discriminator (single byte), then the raw
    /// pixel bytes. This means two `Rgba8` 4×4 images with identical
    /// pixels yield the same handle (cache dedup), while distinct
    /// dimensions or formats produce distinct handles.
    #[must_use]
    pub fn content_hash(&self) -> ImageHandle {
        let mut h = blake3::Hasher::new();
        h.update(&self.inner.width.to_le_bytes());
        h.update(&self.inner.height.to_le_bytes());
        let pf_tag: u8 = match self.inner.pixel_format {
            rge_io_image::PixelFormat::Rgba8 => 0,
            rge_io_image::PixelFormat::Rgba16 => 1,
            rge_io_image::PixelFormat::Rgba32F => 2,
        };
        h.update(&[pf_tag]);
        h.update(&self.inner.pixels);
        ImageHandle(*h.finalize().as_bytes())
    }
}

/// Walk every glTF image in `doc`, decode embedded bytes, and return
/// the resulting [`ImageAsset`] vec indexed by glTF image index.
///
/// `buffers` is the buffer set already resolved by
/// [`crate::import::resolve_buffers`]; we only need the bytes for
/// `View`-source images.
///
/// # Errors
///
/// - [`GltfError::Schema`] when an image has a `Uri` source that isn't
///   `data:image/png;base64,...` or `data:image/jpeg;base64,...`
///   (external file/HTTP URIs are v0-rejected).
/// - [`GltfError::Schema`] when a `View` source references a buffer
///   index outside the supplied `buffers` slice, or with offset+length
///   exceeding the buffer.
/// - [`GltfError::Schema`] when `rge_io_image::load_bytes` fails to
///   decode the payload (corrupt PNG, unknown magic, etc.). The
///   underlying [`rge_io_image::ImageError`] is included verbatim.
pub fn extract_images(
    doc: &gltf::Document,
    buffers: &[BufferData],
) -> Result<Vec<ImageAsset>, GltfError> {
    let mut out = Vec::with_capacity(doc.images().count());
    for image in doc.images() {
        let bytes = encoded_bytes_for_image(&image, buffers)?;
        let decoded = rge_io_image::load_bytes(&bytes).map_err(|e| {
            GltfError::Schema(format!("image {} decode failed: {e}", image.index()))
        })?;
        out.push(ImageAsset::from_inner(decoded));
    }
    Ok(out)
}

/// Resolve a `gltf::Image` to the encoded image bytes (PNG/JPEG/etc.).
///
/// Returns an owned `Vec<u8>` to keep the View-vs-URI shape uniform
/// and avoid leaking a borrow into the buffer set.
fn encoded_bytes_for_image(
    image: &gltf::Image,
    buffers: &[BufferData],
) -> Result<Vec<u8>, GltfError> {
    match image.source() {
        gltf::image::Source::View { view, mime_type: _ } => {
            let buf_idx = view.buffer().index();
            let buffer = buffers.get(buf_idx).ok_or_else(|| {
                GltfError::Schema(format!(
                    "image {} buffer-view references buffer {} but only {} buffers resolved",
                    image.index(),
                    buf_idx,
                    buffers.len()
                ))
            })?;
            let start = view.offset();
            let end = start + view.length();
            if end > buffer.len() {
                return Err(GltfError::Schema(format!(
                    "image {} buffer-view {}..{} exceeds buffer {} length {}",
                    image.index(),
                    start,
                    end,
                    buf_idx,
                    buffer.len()
                )));
            }
            Ok(buffer[start..end].to_vec())
        }
        gltf::image::Source::Uri { uri, mime_type: _ } => decode_image_data_uri(uri),
    }
}

/// Strict subset of RFC-2397 — accept the two glTF-spec image MIME
/// prefixes only. Mirrors [`crate::import::decode_data_uri`]'s posture
/// for buffers: only inline `data:` URIs at v0; external `file.png` /
/// `https://...` URIs return a clear `Schema` error.
fn decode_image_data_uri(uri: &str) -> Result<Vec<u8>, GltfError> {
    const PREFIXES: &[&str] = &["data:image/png;base64,", "data:image/jpeg;base64,"];
    for p in PREFIXES {
        if let Some(rest) = uri.strip_prefix(p) {
            return crate::import::base64_decode_exposed(rest)
                .ok_or_else(|| GltfError::Schema("base64 decode failed in image data URI".into()));
        }
    }
    Err(GltfError::Schema(format!(
        "unsupported image URI (only data:image/png;base64 and data:image/jpeg;base64 supported at v0): {uri}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba8_image_4x4(r: u8, g: u8, b: u8) -> Image {
        let mut pixels = Vec::with_capacity(4 * 4 * 4);
        for _ in 0..(4 * 4) {
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
        Image::from_rgba8(4, 4, pixels)
    }

    #[test]
    fn image_handle_content_hash_is_stable() {
        let asset = ImageAsset::from_inner(rgba8_image_4x4(10, 20, 30));
        assert_eq!(asset.content_hash(), asset.content_hash());
    }

    #[test]
    fn image_handle_distinguishes_different_pixels() {
        let a = ImageAsset::from_inner(rgba8_image_4x4(10, 20, 30));
        let b = ImageAsset::from_inner(rgba8_image_4x4(10, 20, 31));
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn image_handle_distinguishes_different_dimensions() {
        let small = ImageAsset::from_inner(Image::zeros(2, 2, rge_io_image::PixelFormat::Rgba8));
        let big = ImageAsset::from_inner(Image::zeros(4, 4, rge_io_image::PixelFormat::Rgba8));
        assert_ne!(small.content_hash(), big.content_hash());
    }

    #[test]
    fn decode_image_data_uri_rejects_external_file() {
        let err = decode_image_data_uri("file:///tmp/x.png").expect_err("must reject");
        assert!(matches!(err, GltfError::Schema(_)));
    }

    #[test]
    fn decode_image_data_uri_rejects_https() {
        let err = decode_image_data_uri("https://example.com/x.png").expect_err("must reject");
        assert!(matches!(err, GltfError::Schema(_)));
    }

    #[test]
    fn decode_image_data_uri_rejects_unknown_data_payload() {
        // `data:application/octet-stream;base64,...` is valid for
        // BUFFER URIs but NOT for images. Image URIs must use
        // `data:image/png;base64,` or `data:image/jpeg;base64,`.
        let err = decode_image_data_uri("data:application/octet-stream;base64,Zm9v")
            .expect_err("must reject");
        assert!(matches!(err, GltfError::Schema(_)));
    }

    #[test]
    fn decode_image_data_uri_accepts_png_prefix() {
        // base64("hi") = "aGk=" — payload is "hi" (not a real PNG), but
        // the URI prefix is the supported shape; bytes flow through.
        let bytes = decode_image_data_uri("data:image/png;base64,aGk=").expect("accept");
        assert_eq!(bytes, b"hi");
    }

    #[test]
    fn decode_image_data_uri_accepts_jpeg_prefix() {
        let bytes = decode_image_data_uri("data:image/jpeg;base64,aGk=").expect("accept");
        assert_eq!(bytes, b"hi");
    }
}
