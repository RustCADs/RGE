// adapted from rustforge::runtime-pbr::ibl on 2026-05-05 — kept the cube-extent +
//                                                  intensity convention; dropped the
//                                                  prefiltered-mip-chain handle since
//                                                  the gfx wave owns GPU residency.
//
//! [`ReflectionProbe`] — captures local cubemap reflections for IBL.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// Reflection-probe component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReflectionProbe {
    /// Extents of the probe's parallax-correction box, half-extents in
    /// local space. `[r, r, r]` for a sphere bounding cube.
    pub box_half_extents: [f32; 3],
    /// Captured cubemap asset (stitched + prefiltered). [`NULL_ASSET_ID`]
    /// when the probe still needs a bake.
    pub cubemap: AssetId,
    /// Optional fade-out distance: 0.0 means no fade.
    pub fade_distance_m: f32,
    /// Multiplier on probe contribution. 1.0 = exact bake; <1.0 dims the
    /// probe relative to global IBL.
    pub intensity: f32,
}

impl Default for ReflectionProbe {
    fn default() -> Self {
        Self {
            box_half_extents: [0.0; 3],
            cubemap: NULL_ASSET_ID,
            fade_distance_m: 0.0,
            intensity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let p = ReflectionProbe {
            box_half_extents: [4.0, 2.5, 4.0],
            cubemap: AssetId::from_bytes(b"reflection-probe-cubemap-1"),
            fade_distance_m: 0.5,
            intensity: 0.8,
        };
        let s = ron::to_string(&p).expect("serialize");
        let back: ReflectionProbe = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn default_probe_has_zero_extents() {
        let p = ReflectionProbe::default();
        for axis in p.box_half_extents {
            assert!(axis.abs() < f32::EPSILON);
        }
        assert_eq!(p.cubemap, NULL_ASSET_ID);
    }
}
