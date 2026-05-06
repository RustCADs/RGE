// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Token vocabulary for the RGE theme registry. Tokens are the leaves
// of theme resolution: every `Style` field eventually walks down to
// one of these concrete values. Tokens are RON-friendly and serde
// round-trippable so theme files in `assets/themes/*.theme.ron` can
// declare them directly.
//
// Design notes
// ============
// * `Color` carries both an sRGB byte tuple and the matching linear
//   `f32` triple. The sRGB form round-trips to RON as `(255,255,255)`
//   tuples that humans can hand-edit; the linear form is what wgpu
//   clear colours and shader uniforms want. Keeping the pair on one
//   token avoids a recompute hot-path during repaint.
// * `Length` is unit-aware: the resolver picks the right metric based
//   on the consuming widget (e.g. egui's `Pt`, layout's `Px`).
// * `Animation` carries a `Duration` in milliseconds plus a curve hint.
//   Setting all `motion.*` durations to zero is how the
//   `reduced-motion` accessibility variant collapses transitions.

use serde::{Deserialize, Serialize};

/// Concrete value attached to a token name. The full vocabulary the
/// theme registry knows how to serialize.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Token {
    /// 8-bit-per-channel sRGB plus the matching linear-space triple.
    Color(Color),
    /// A scalar dimension with explicit unit.
    Length(Length),
    /// Font family, point size, and weight.
    Font(FontToken),
    /// 4-sided padding (top, right, bottom, left).
    Padding(EdgeInsets),
    /// 4-sided margin.
    Margin(EdgeInsets),
    /// Drop-shadow with offset, blur, and colour.
    Shadow(ShadowToken),
    /// Animation duration plus easing curve.
    Animation(AnimationToken),
    /// Opacity multiplier in `[0.0, 1.0]`.
    Opacity(f32),
    /// Free-form string token (theme-id, custom flags). Used sparingly.
    Text(String),
}

/// 8-bit-per-channel sRGB colour with explicit alpha plus the
/// pre-computed linear triple. Invariants:
/// * `srgb` channels are in `0..=255`
/// * `linear` channels are in `0.0..=1.0`
/// * `linear` is the gamma-decoded sibling of `srgb` (the constructor
///   computes it; manual deserialisation must keep them in sync).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub srgb: [u8; 4],
    pub linear: [f32; 4],
}

impl Color {
    /// Build a colour from its sRGB byte form. Alpha defaults to 255.
    pub fn from_srgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_srgba(r, g, b, 255)
    }

    /// Build a colour from its sRGB byte form including alpha.
    pub fn from_srgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        let lin = |c: u8| -> f32 {
            let f = c as f32 / 255.0;
            if f <= 0.04045 {
                f / 12.92
            } else {
                ((f + 0.055) / 1.055).powf(2.4)
            }
        };
        Color {
            srgb: [r, g, b, a],
            linear: [lin(r), lin(g), lin(b), a as f32 / 255.0],
        }
    }

    /// WCAG relative-luminance for the colour, ignoring alpha.
    /// Per-channel coefficients match the W3C 2.x reference formula.
    pub fn relative_luminance(&self) -> f32 {
        let [r, g, b, _] = self.linear;
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Returns a new `Color` with alpha replaced.
    pub fn with_alpha(self, a: u8) -> Self {
        Self::from_srgba(self.srgb[0], self.srgb[1], self.srgb[2], a)
    }
}

/// Length value with explicit unit. The renderer resolves the unit
/// against its DPI / font context; the theme registry never tries to
/// guess.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Length {
    /// Logical pixels (post-DPI scale).
    Px(f32),
    /// `em` — multiple of the current resolved font size.
    Em(f32),
    /// Typographic points (1pt = 1/72 inch).
    Pt(f32),
    /// Percentage of the parent dimension, in `[0.0, 100.0]`.
    Percent(f32),
}

impl Length {
    /// Resolve the length to logical pixels using a context-supplied
    /// `em_size_px` (current font em in pixels) and `parent_px`
    /// (used for `%`). DPI scaling is the caller's responsibility.
    pub fn to_px(self, em_size_px: f32, parent_px: f32) -> f32 {
        match self {
            Length::Px(v) => v,
            Length::Em(v) => v * em_size_px,
            Length::Pt(v) => v * (96.0 / 72.0),
            Length::Percent(v) => v / 100.0 * parent_px,
        }
    }
}

/// Font reference: family name (looked up in `ui-fonts` registry),
/// size in points, and weight.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontToken {
    pub family: String,
    pub size: Length,
    pub weight: FontWeight,
}

/// CSS-style numeric font weight (100 thin .. 900 black).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

impl FontWeight {
    pub fn css_value(self) -> u16 {
        match self {
            FontWeight::Thin => 100,
            FontWeight::ExtraLight => 200,
            FontWeight::Light => 300,
            FontWeight::Regular => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
            FontWeight::ExtraBold => 800,
            FontWeight::Black => 900,
        }
    }
}

/// 4-sided edge insets, in arbitrary `Length` units. Padding and
/// margin both share this shape — they differ only in how the
/// consumer interprets them.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeInsets {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl EdgeInsets {
    /// All four sides equal.
    pub fn uniform(value: Length) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// Drop-shadow: offset (logical px), blur radius (logical px), spread,
/// and colour.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShadowToken {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

/// Animation duration + curve hint. `duration_ms` is the canonical
/// authoring unit; `reduced-motion` zeroes it via an axis transform.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationToken {
    pub duration_ms: u32,
    pub curve: Curve,
}

/// Easing curve identifier. Renderer maps these to its actual easing
/// function — the theme just labels.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Curve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Spring,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_alpha_default() {
        let c = Color::from_srgb(255, 128, 0);
        assert_eq!(c.srgb, [255, 128, 0, 255]);
    }

    #[test]
    fn linear_round_trip_white_black() {
        let white = Color::from_srgb(255, 255, 255);
        assert!((white.linear[0] - 1.0).abs() < 1e-3);
        let black = Color::from_srgb(0, 0, 0);
        assert!(black.linear[0].abs() < 1e-6);
    }

    #[test]
    fn luminance_white_is_one() {
        let white = Color::from_srgb(255, 255, 255);
        assert!((white.relative_luminance() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn luminance_black_is_zero() {
        let black = Color::from_srgb(0, 0, 0);
        assert!(black.relative_luminance() < 1e-6);
    }

    #[test]
    fn length_em_scales() {
        assert_eq!(Length::Em(1.5).to_px(16.0, 100.0), 24.0);
    }

    #[test]
    fn length_percent_against_parent() {
        assert_eq!(Length::Percent(50.0).to_px(16.0, 200.0), 100.0);
    }

    #[test]
    fn pt_to_px_default_dpi() {
        // 12pt at 96 dpi == 16px.
        assert!((Length::Pt(12.0).to_px(16.0, 0.0) - 16.0).abs() < 1e-3);
    }

    #[test]
    fn font_weight_css_codes() {
        assert_eq!(FontWeight::Regular.css_value(), 400);
        assert_eq!(FontWeight::Bold.css_value(), 700);
    }

    #[test]
    fn edge_insets_uniform() {
        let e = EdgeInsets::uniform(Length::Px(4.0));
        assert_eq!(e.top, e.bottom);
        assert_eq!(e.left, e.right);
    }

    #[test]
    fn token_serde_round_trip_color() {
        let t = Token::Color(Color::from_srgba(10, 20, 30, 200));
        let s = ron::to_string(&t).unwrap();
        let back: Token = ron::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn token_serde_round_trip_animation() {
        let t = Token::Animation(AnimationToken {
            duration_ms: 220,
            curve: Curve::EaseOut,
        });
        let s = ron::to_string(&t).unwrap();
        let back: Token = ron::from_str(&s).unwrap();
        assert_eq!(t, back);
    }
}
