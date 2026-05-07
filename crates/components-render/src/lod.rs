//! [`Lod`] — level-of-detail mesh swap configuration.
//!
//! Stored as a fixed-capacity tier list. The renderer's LOD system picks the
//! highest-tier entry whose `screen_area_threshold` is exceeded; the entity's
//! `MeshHandle` is then re-pointed at the chosen tier's asset.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// Single LOD tier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LodLevel {
    /// Mesh asset rendered at this tier.
    pub mesh: AssetId,
    /// Pixel-area threshold above which this tier is selected, measured at
    /// the current render-target resolution. Lower = farther distance.
    pub screen_area_threshold: f32,
}

impl Default for LodLevel {
    fn default() -> Self {
        Self {
            mesh: NULL_ASSET_ID,
            screen_area_threshold: 0.0,
        }
    }
}

/// LOD tier list.
///
/// Bounded at 4 tiers — beyond that, GLTF / engine convention treats the
/// extras as imposters. The `len` field tracks how many slots are populated;
/// the rest are zeroed. Components must be `Copy`-able for ECS storage,
/// hence the fixed array instead of a `Vec<LodLevel>`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Lod {
    /// Tier slots, ordered finest-first.
    pub levels: [LodLevel; 4],
    /// How many slots are populated. Values beyond this are ignored.
    pub len: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_lod_level() {
        let l = LodLevel {
            mesh: AssetId::from_bytes(b"lod-level-mesh-1"),
            screen_area_threshold: 10_000.0,
        };
        let s = ron::to_string(&l).expect("serialize");
        let back: LodLevel = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }

    #[test]
    fn round_trip_ron_lod() {
        let mut l = Lod::default();
        l.levels[0] = LodLevel {
            mesh: AssetId::from_bytes(b"lod-mesh-tier-0"),
            screen_area_threshold: 100_000.0,
        };
        l.levels[1] = LodLevel {
            mesh: AssetId::from_bytes(b"lod-mesh-tier-1"),
            screen_area_threshold: 10_000.0,
        };
        l.len = 2;
        let s = ron::to_string(&l).expect("serialize");
        let back: Lod = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }
}
