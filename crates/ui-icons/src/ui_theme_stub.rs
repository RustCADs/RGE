//! Local stub of `ui-theme::Color` for use in this crate until W05 lands.
//!
//! `ui-icons` only needs the RGB triple — it does not depend on the
//! token system, palette resolution, or theme switching that `ui-theme`
//! will eventually provide. When W05 merges the real
//! `ui_theme::Color`, the type alias here can be retargeted at the
//! upstream crate without churning callers.
//!
//! Adapted from the Phase-5 spec: only the fields actually consumed
//! by [`crate::tint::apply_tint`] are present.

/// 24-bit RGB color used by tinting.
///
/// Alpha is intentionally absent: icons inherit panel alpha from the
/// rendering context, and Lucide-style monochrome icons have no
/// per-pixel alpha embedded in the source SVG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel (0..=255).
    pub r: u8,
    /// Green channel (0..=255).
    pub g: u8,
    /// Blue channel (0..=255).
    pub b: u8,
}

impl Color {
    /// Pure black — convenient default for tests.
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    /// Pure white.
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };
    /// Construct from three channels.
    #[inline]
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}
