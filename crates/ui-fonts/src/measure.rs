//! Text measurement on top of the cosmic-text shaping API.
//!
//! [`Measure`] is a value-typed view into a [`crate::FontRegistry`] that
//! configures attributes (family, weight, slant, font size, line height) and
//! produces a [`MeasuredText`] with width / height / baseline / per-glyph
//! positions.

use cosmic_text::{
    Attrs, Buffer, BufferLine, Family as CtFamily, LineEnding, Metrics, Shaping, Style, Weight,
    Wrap,
};

use crate::registry::FontRegistry;

/// Italic / oblique selector mirrored from `cosmic_text::Style`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FontSlant {
    /// Upright glyphs.
    #[default]
    Normal,
    /// Italic glyphs (designed slanted face).
    Italic,
    /// Oblique glyphs (mathematically slanted face).
    Oblique,
}

impl FontSlant {
    fn to_cosmic(self) -> Style {
        match self {
            FontSlant::Normal => Style::Normal,
            FontSlant::Italic => Style::Italic,
            FontSlant::Oblique => Style::Oblique,
        }
    }
}

/// Convenience CSS-style weight aliases. Numeric values pass through
/// unchanged via [`Measure::with_weight_value`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum FontWeightHint {
    /// Light (300).
    Light,
    /// Regular (400).
    #[default]
    Regular,
    /// Medium (500).
    Medium,
    /// Semibold (600).
    Semibold,
    /// Bold (700).
    Bold,
}

impl FontWeightHint {
    /// Numeric CSS weight value.
    #[must_use]
    pub fn value(self) -> u16 {
        match self {
            FontWeightHint::Light => 300,
            FontWeightHint::Regular => 400,
            FontWeightHint::Medium => 500,
            FontWeightHint::Semibold => 600,
            FontWeightHint::Bold => 700,
        }
    }
}

/// Per-glyph layout output. Coordinates are in pixels relative to the
/// measurement origin (top-left of the buffer area).
#[derive(Clone, Copy, Debug)]
pub struct GlyphPos {
    /// Source byte offset of the cluster start in the input string.
    pub cluster_start: usize,
    /// Source byte offset of the cluster end in the input string.
    pub cluster_end: usize,
    /// Glyph index inside the resolved font face.
    pub glyph_id: u16,
    /// Sub-pixel x-offset relative to the cluster's pen position.
    pub x: f32,
    /// Y baseline of the glyph (top of line + ascent).
    pub y: f32,
    /// Advance width consumed by this glyph.
    pub w: f32,
    /// Glyph height as reported by shaping (line-height-relative).
    pub h: f32,
}

/// Aggregate measurement of a shaped string.
#[derive(Clone, Debug, Default)]
pub struct MeasuredText {
    /// Total advance width of the longest line, in pixels.
    pub width: f32,
    /// Total height across all lines, in pixels.
    pub height: f32,
    /// Y baseline of the first visual line (top of line + ascent).
    pub first_baseline: f32,
    /// Per-glyph positions for the first visual line. Higher-level callers
    /// that need every line should iterate the `cosmic_text::Buffer`
    /// directly via [`crate::FontRegistry::font_system_mut`].
    pub glyphs: Vec<GlyphPos>,
}

/// Measurement configuration. Cheap to clone; rebuild per-call as needed.
#[derive(Clone, Debug)]
pub struct Measure {
    family: String,
    font_size: f32,
    line_height: f32,
    weight: u16,
    slant: FontSlant,
    wrap: Wrap,
    width_limit: Option<f32>,
}

impl Measure {
    /// Build a measurement spec with default 13-pixel font and 1.2× line
    /// height (~16 px), matching the editor's default body type scale.
    #[must_use]
    pub fn new(family: impl Into<String>) -> Self {
        let font_size = 13.0_f32;
        Self {
            family: family.into(),
            font_size,
            line_height: font_size * 1.2,
            weight: 400,
            slant: FontSlant::Normal,
            wrap: Wrap::None,
            width_limit: None,
        }
    }

    /// Set font size in pixels.
    #[must_use]
    pub fn with_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self.line_height = font_size * 1.2;
        self
    }

    /// Set both font size and line height in pixels.
    #[must_use]
    pub fn with_size_and_line_height(mut self, font_size: f32, line_height: f32) -> Self {
        self.font_size = font_size;
        self.line_height = line_height;
        self
    }

    /// Set CSS-style weight via [`FontWeightHint`].
    #[must_use]
    pub fn with_weight(mut self, weight: FontWeightHint) -> Self {
        self.weight = weight.value();
        self
    }

    /// Set CSS-style weight by raw 100..900 value.
    #[must_use]
    pub fn with_weight_value(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// Set italic / oblique slant.
    #[must_use]
    pub fn with_slant(mut self, slant: FontSlant) -> Self {
        self.slant = slant;
        self
    }

    /// Set wrap mode. Defaults to [`Wrap::None`] for single-line measurement.
    #[must_use]
    pub fn with_wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Constrain layout to a maximum width in pixels. Wraps according to
    /// [`Self::with_wrap`].
    #[must_use]
    pub fn with_width(mut self, width: f32) -> Self {
        self.width_limit = Some(width);
        self
    }

    /// Currently configured family name.
    #[must_use]
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Currently configured font size in pixels.
    #[must_use]
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Build cosmic-text [`Attrs`] reflecting the current configuration.
    fn attrs(&self) -> Attrs<'_> {
        Attrs::new()
            .family(CtFamily::Name(&self.family))
            .weight(Weight(self.weight))
            .style(self.slant.to_cosmic())
    }

    /// Shape `text` and return the aggregate measurement.
    ///
    /// Single-line strings produce one entry in [`MeasuredText::glyphs`];
    /// multi-line strings (with `\n`) shape every line but only the first
    /// line's glyphs are returned in `glyphs` — `width` is the max across
    /// all lines and `height` is the cumulative height. This matches the
    /// expected use of "give me one measurement for a label".
    pub fn measure(&self, registry: &mut FontRegistry, text: &str) -> MeasuredText {
        let metrics = Metrics::new(self.font_size, self.line_height);
        let mut buffer = Buffer::new(registry.font_system_mut(), metrics);
        buffer.set_wrap(self.wrap);
        buffer.set_size(self.width_limit, None);

        // Use one BufferLine per logical line: cosmic-text splits on \n
        // automatically when set_text sees them, so a single set_text call is
        // enough.
        buffer.set_text(text, &self.attrs(), Shaping::Advanced, None);
        buffer.shape_until_scroll(registry.font_system_mut(), false);

        let mut max_w = 0.0_f32;
        let mut total_h = 0.0_f32;
        let mut first_baseline = 0.0_f32;
        let mut glyphs: Vec<GlyphPos> = Vec::new();
        let mut first = true;

        for run in buffer.layout_runs() {
            if first {
                first_baseline = run.line_y;
                for g in run.glyphs {
                    glyphs.push(GlyphPos {
                        cluster_start: g.start,
                        cluster_end: g.end,
                        glyph_id: g.glyph_id,
                        x: g.x,
                        y: run.line_y,
                        w: g.w,
                        h: run.line_height,
                    });
                }
                first = false;
            }
            if run.line_w > max_w {
                max_w = run.line_w;
            }
            total_h += run.line_height;
        }

        // Empty string → still report a single line of height.
        if total_h == 0.0 {
            total_h = self.line_height;
            first_baseline = self.line_height; // approx — top to baseline
        }

        MeasuredText {
            width: max_w,
            height: total_h,
            first_baseline,
            glyphs,
        }
    }

    /// Build (but do not consume) a `cosmic_text::Buffer` matching this
    /// measurement spec. Useful for callers that want to keep shaping state
    /// alive across frames (editor labels) instead of re-shaping every time.
    #[must_use]
    pub fn build_buffer(&self, registry: &mut FontRegistry, text: &str) -> Buffer {
        let metrics = Metrics::new(self.font_size, self.line_height);
        let mut buffer = Buffer::new(registry.font_system_mut(), metrics);
        buffer.set_wrap(self.wrap);
        buffer.set_size(self.width_limit, None);
        buffer.set_text(text, &self.attrs(), Shaping::Advanced, None);
        buffer.shape_until_scroll(registry.font_system_mut(), false);
        buffer
    }
}

/// Marker so [`BufferLine`], [`LineEnding`] are referenced (and re-exported
/// transitively) — keeps documentation links stable when callers want them.
#[doc(hidden)]
#[must_use]
pub fn _doc_anchor() -> (Option<BufferLine>, Option<LineEnding>) {
    (None, None)
}
