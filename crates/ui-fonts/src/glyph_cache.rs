//! Glyph atlas — caches rasterized glyph images keyed by `(font, glyph,
//! subpixel-quantized-x)`.
//!
//! The cache is intentionally backend-agnostic: it stores raw 8-bit
//! coverage masks (greyscale anti-aliased glyphs) plus their bounding boxes.
//! A higher-level renderer (egui's painter, the wgpu compositor, etc.) takes
//! these images and uploads them to its own GPU atlas.
//!
//! Invalidation is wholesale: a font swap calls [`GlyphCache::clear`] and
//! every consumer re-rasterizes on demand. This keeps the swap budget under
//! 100 ms even for large caches because there is no per-glyph eviction
//! bookkeeping to walk.

use std::collections::HashMap;
use std::time::Instant;

use cosmic_text::{fontdb, CacheKey, SwashCache, SwashContent};

use crate::registry::FontRegistry;

/// Stable identifier for a cached glyph entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GlyphKey {
    /// Source font ID.
    pub font_id: fontdb::ID,
    /// Glyph index in the font.
    pub glyph_id: u16,
    /// Font size in 1/64-px units (cosmic-text quantization grain).
    pub size_subpx: u32,
    /// Sub-pixel x bin, 0..=2 (0=integer pixel, 1=+1/3, 2=+2/3).
    pub subpx_bin: u8,
}

/// Rasterized glyph payload.
#[derive(Clone, Debug)]
pub struct GlyphImage {
    /// Pixel width of the bitmap.
    pub width: u32,
    /// Pixel height of the bitmap.
    pub height: u32,
    /// Bitmap left bearing relative to the pen position (positive → right).
    pub left: i32,
    /// Bitmap top bearing relative to the baseline (positive → up).
    pub top: i32,
    /// True when the glyph is full-color (e.g. an emoji) and `pixels` carries
    /// RGBA data; false when greyscale-coverage in 8-bit alpha.
    pub color: bool,
    /// Raw pixel buffer. For greyscale: `width × height` bytes. For color:
    /// `width × height × 4` bytes (RGBA, premultiplied).
    pub pixels: Vec<u8>,
}

/// Glyph atlas. Internally backed by [`cosmic_text::SwashCache`] for
/// rasterization but exposes its own image-keyed map so callers can drive
/// invalidation independently of the cosmic-text shaping cache.
#[derive(Debug)]
pub struct GlyphCache {
    swash: SwashCache,
    images: HashMap<GlyphKey, Option<GlyphImage>>,
    last_swap: Instant,
}

impl GlyphCache {
    /// Build an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            swash: SwashCache::new(),
            images: HashMap::new(),
            last_swap: Instant::now(),
        }
    }

    /// Number of rasterized entries currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// True iff the cache holds zero entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Purge every cached glyph image and reset the underlying swash cache.
    /// Called on font swap so the next paint re-rasterizes against the new
    /// faces.
    pub fn clear(&mut self) {
        self.images.clear();
        self.swash = SwashCache::new();
        self.last_swap = Instant::now();
    }

    /// Wall-clock time since the last [`Self::clear`] call.
    #[must_use]
    pub fn time_since_last_swap(&self) -> std::time::Duration {
        self.last_swap.elapsed()
    }

    /// Build a [`GlyphKey`] from a cosmic-text [`CacheKey`].
    #[must_use]
    pub fn key_from_cosmic(font_id: fontdb::ID, ck: CacheKey) -> GlyphKey {
        GlyphKey {
            font_id,
            glyph_id: ck.glyph_id,
            size_subpx: ck.font_size_bits,
            subpx_bin: subpx_bin(ck.x_bin),
        }
    }

    /// Rasterize and cache the glyph identified by `cache_key`. Returns
    /// `None` if cosmic-text cannot produce an image (unsupported glyph,
    /// missing face).
    pub fn get_or_render(
        &mut self,
        registry: &mut FontRegistry,
        font_id: fontdb::ID,
        cache_key: CacheKey,
    ) -> Option<&GlyphImage> {
        use std::collections::hash_map::Entry;
        let key = Self::key_from_cosmic(font_id, cache_key);
        if let Entry::Vacant(e) = self.images.entry(key) {
            let image = self
                .swash
                .get_image_uncached(registry.font_system_mut(), cache_key)
                .map(|img| GlyphImage {
                    width: img.placement.width,
                    height: img.placement.height,
                    left: img.placement.left,
                    top: img.placement.top,
                    color: matches!(img.content, SwashContent::Color),
                    pixels: img.data,
                });
            e.insert(image);
        }
        self.images.get(&key).and_then(|opt| opt.as_ref())
    }

    /// Pre-render every glyph that the supplied iterator produces. Used by
    /// the menu/widget layer at page-load time so the first paint frame
    /// doesn't stall on rasterization.
    pub fn warm<I>(&mut self, registry: &mut FontRegistry, items: I)
    where
        I: IntoIterator<Item = (fontdb::ID, CacheKey)>,
    {
        for (id, ck) in items {
            let _ = self.get_or_render(registry, id, ck);
        }
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Quantize a sub-pixel x-bin into a 0..=2 bucket. cosmic-text emits a
/// signed `i32` whose magnitude is in 1/3-px steps; we re-bin to a small
/// unsigned value so cache keys stay compact.
fn subpx_bin(x_bin: cosmic_text::SubpixelBin) -> u8 {
    match x_bin {
        cosmic_text::SubpixelBin::Zero => 0,
        cosmic_text::SubpixelBin::One => 1,
        cosmic_text::SubpixelBin::Two => 2,
        cosmic_text::SubpixelBin::Three => 3,
    }
}
