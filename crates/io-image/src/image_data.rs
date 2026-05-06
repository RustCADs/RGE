//! In-memory image representation.

/// Decoded image with explicit pixel format and dimensions.
///
/// `pixels` is always a flat row-major buffer with `width * height *
/// channels_per_pixel(pixel_format)` elements, packed in scanline order with
/// no padding. The element type depends on `pixel_format`:
///
/// - [`PixelFormat::Rgba8`]: `pixels` length = `width * height * 4` bytes.
/// - [`PixelFormat::Rgba16`]: `pixels` length = `width * height * 4 * 2` bytes
///   (LE-packed `u16`s; helpers below do the conversion).
/// - [`PixelFormat::Rgba32F`]: `pixels` length = `width * height * 4 * 4`
///   bytes (LE-packed `f32`s).
#[derive(Clone, Debug)]
pub struct Image {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Pixel storage format.
    pub pixel_format: PixelFormat,
    /// Raw byte buffer, scanline-row-major, no padding.
    pub pixels: Vec<u8>,
}

/// Discriminator for [`Image::pixel_format`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    /// 8-bit unsigned RGBA.
    Rgba8,
    /// 16-bit unsigned RGBA (native-endian in memory; serialized little-endian).
    Rgba16,
    /// 32-bit float RGBA.
    Rgba32F,
}

impl PixelFormat {
    /// Bytes per pixel for this format.
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 => 4,
            Self::Rgba16 => 8,
            Self::Rgba32F => 16,
        }
    }
}

impl Image {
    /// Construct an empty image of the given format with all-zero pixels.
    #[must_use]
    pub fn zeros(width: u32, height: u32, pixel_format: PixelFormat) -> Self {
        let len = (width as usize) * (height as usize) * pixel_format.bytes_per_pixel();
        Self {
            width,
            height,
            pixel_format,
            pixels: vec![0u8; len],
        }
    }

    /// Total pixel count.
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// Iterate `Rgba32F` pixels as `[f32; 4]`. Panics if format mismatch.
    #[must_use]
    pub fn iter_rgba32f(&self) -> RgbaF32Iter<'_> {
        assert_eq!(self.pixel_format, PixelFormat::Rgba32F);
        RgbaF32Iter {
            buf: &self.pixels,
            cursor: 0,
        }
    }

    /// Iterate `Rgba16` pixels as `[u16; 4]`. Panics if format mismatch.
    #[must_use]
    pub fn iter_rgba16(&self) -> Rgba16Iter<'_> {
        assert_eq!(self.pixel_format, PixelFormat::Rgba16);
        Rgba16Iter {
            buf: &self.pixels,
            cursor: 0,
        }
    }

    /// Iterate `Rgba8` pixels as `[u8; 4]`. Panics if format mismatch.
    #[must_use]
    pub fn iter_rgba8(&self) -> Rgba8Iter<'_> {
        assert_eq!(self.pixel_format, PixelFormat::Rgba8);
        Rgba8Iter {
            buf: &self.pixels,
            cursor: 0,
        }
    }

    /// Pack a slice of `f32` quartets into the byte buffer for `Rgba32F`.
    pub fn from_rgba32f(width: u32, height: u32, samples: &[f32]) -> Self {
        let pixel_count = (width as usize) * (height as usize);
        assert_eq!(samples.len(), pixel_count * 4);
        let mut pixels = Vec::with_capacity(pixel_count * 16);
        for &v in samples {
            pixels.extend_from_slice(&v.to_le_bytes());
        }
        Self {
            width,
            height,
            pixel_format: PixelFormat::Rgba32F,
            pixels,
        }
    }

    /// Pack a slice of `u16` quartets into the byte buffer for `Rgba16`.
    pub fn from_rgba16(width: u32, height: u32, samples: &[u16]) -> Self {
        let pixel_count = (width as usize) * (height as usize);
        assert_eq!(samples.len(), pixel_count * 4);
        let mut pixels = Vec::with_capacity(pixel_count * 8);
        for &v in samples {
            pixels.extend_from_slice(&v.to_le_bytes());
        }
        Self {
            width,
            height,
            pixel_format: PixelFormat::Rgba16,
            pixels,
        }
    }

    /// Construct an `Rgba8` image directly from RGBA bytes.
    pub fn from_rgba8(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let pixel_count = (width as usize) * (height as usize);
        assert_eq!(rgba.len(), pixel_count * 4);
        Self {
            width,
            height,
            pixel_format: PixelFormat::Rgba8,
            pixels: rgba,
        }
    }
}

/// Iterator over `Rgba32F` pixels.
pub struct RgbaF32Iter<'a> {
    buf: &'a [u8],
    cursor: usize,
}

impl Iterator for RgbaF32Iter<'_> {
    type Item = [f32; 4];
    fn next(&mut self) -> Option<[f32; 4]> {
        if self.cursor + 16 > self.buf.len() {
            return None;
        }
        let chunk = &self.buf[self.cursor..self.cursor + 16];
        self.cursor += 16;
        let mut out = [0.0f32; 4];
        for (i, w) in chunk.chunks_exact(4).enumerate() {
            out[i] = f32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        }
        Some(out)
    }
}

/// Iterator over `Rgba16` pixels.
pub struct Rgba16Iter<'a> {
    buf: &'a [u8],
    cursor: usize,
}

impl Iterator for Rgba16Iter<'_> {
    type Item = [u16; 4];
    fn next(&mut self) -> Option<[u16; 4]> {
        if self.cursor + 8 > self.buf.len() {
            return None;
        }
        let chunk = &self.buf[self.cursor..self.cursor + 8];
        self.cursor += 8;
        let mut out = [0u16; 4];
        for (i, w) in chunk.chunks_exact(2).enumerate() {
            out[i] = u16::from_le_bytes([w[0], w[1]]);
        }
        Some(out)
    }
}

/// Iterator over `Rgba8` pixels.
pub struct Rgba8Iter<'a> {
    buf: &'a [u8],
    cursor: usize,
}

impl Iterator for Rgba8Iter<'_> {
    type Item = [u8; 4];
    fn next(&mut self) -> Option<[u8; 4]> {
        if self.cursor + 4 > self.buf.len() {
            return None;
        }
        let chunk = &self.buf[self.cursor..self.cursor + 4];
        self.cursor += 4;
        Some([chunk[0], chunk[1], chunk[2], chunk[3]])
    }
}
