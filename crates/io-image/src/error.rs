//! Error types for io-image.

use std::io;

/// Crate result alias.
pub type Result<T> = std::result::Result<T, ImageError>;

/// Errors that can occur during image import/export.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    /// Underlying IO error (filesystem, std::io::Read).
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// File magic did not match any supported format.
    #[error("unknown image format (magic bytes did not match)")]
    UnknownFormat,

    /// Decoder rejected payload as malformed.
    #[error("decode error: {0}")]
    Decode(String),

    /// Encoder rejected payload (e.g. unsupported pixel format for codec).
    #[error("encode error: {0}")]
    Encode(String),

    /// Pixel format mismatch — caller passed an Image whose `PixelFormat`
    /// is not representable by the requested codec.
    #[error("unsupported pixel format for {codec}: {actual:?}")]
    UnsupportedPixelFormat {
        /// Codec name that rejected the format.
        codec: &'static str,
        /// The pixel format that was passed.
        actual: crate::image_data::PixelFormat,
    },

    /// Wraps an `image::ImageError`.
    #[error("image crate error: {0}")]
    ImageCrate(String),

    /// Wraps an `exr` error.
    #[error("exr error: {0}")]
    Exr(String),
}

impl From<image::ImageError> for ImageError {
    fn from(e: image::ImageError) -> Self {
        Self::ImageCrate(e.to_string())
    }
}
