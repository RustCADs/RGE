// adapted from rustforge::runtime-color::space on 2026-05-05 — kept the linear
//                                                  RGB tint convention but stripped the
//                                                  ColorSpace enum; v0 highlight
//                                                  components carry a raw [f32;4] in
//                                                  scene-linear sRGB. Color-space
//                                                  re-introduction is a W18 (io-image)
//                                                  concern.
//
//! [`Highlight`] — selection / hover badge component.
//!
//! Optional component the editor adds to selected entities. Render passes
//! that draw a colored outline / Fresnel glow read this. The `intensity`
//! field lets multi-selection mute non-primary picks.

use serde::{Deserialize, Serialize};

/// Editor highlight color + intensity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    /// Linear sRGB (R, G, B, A). Pre-multiplied alpha not assumed.
    pub color: [f32; 4],
    /// Render-side multiplier. `1.0` = primary selection; `0.4` = secondary.
    pub intensity: f32,
}

impl Highlight {
    /// Default editor selection color (warm orange, full intensity).
    pub const PRIMARY: Highlight = Highlight {
        color: [1.0, 0.6, 0.1, 1.0],
        intensity: 1.0,
    };

    /// Default secondary-selection color (same hue, dimmed).
    pub const SECONDARY: Highlight = Highlight {
        color: [1.0, 0.6, 0.1, 1.0],
        intensity: 0.4,
    };
}

impl Default for Highlight {
    fn default() -> Self {
        Self::PRIMARY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_primary() {
        let h = Highlight::PRIMARY;
        let s = ron::to_string(&h).expect("serialize");
        let back: Highlight = ron::from_str(&s).expect("deserialize");
        assert_eq!(h, back);
    }

    #[test]
    fn round_trip_ron_custom() {
        let h = Highlight {
            color: [0.2, 0.7, 1.0, 0.8],
            intensity: 0.65,
        };
        let s = ron::to_string(&h).expect("serialize");
        let back: Highlight = ron::from_str(&s).expect("deserialize");
        assert_eq!(h, back);
    }
}
