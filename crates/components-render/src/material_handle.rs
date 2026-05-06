// adapted from rustforge::runtime-pbr::openpbr_params on 2026-05-05 — kept the
//                                                  AssetId-handle indirection but
//                                                  dropped the inline OpenPBR param
//                                                  block; v0 component carries only
//                                                  the handle. Material parameter
//                                                  storage moves into a future
//                                                  components-material crate when
//                                                  material-graph (W12 sibling) lands.
//
//! [`MaterialHandle`] — typed handle into the material residency cache.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// "This entity shades using the material asset with the given id."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct MaterialHandle(pub AssetId);

impl Default for MaterialHandle {
    fn default() -> Self {
        Self(NULL_ASSET_ID)
    }
}

impl MaterialHandle {
    /// Wrap an [`AssetId`] in a material-handle component.
    #[inline]
    #[must_use]
    pub const fn new(id: AssetId) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let m = MaterialHandle::new(AssetId::from_bytes(b"material-handle-fixture-1"));
        let s = ron::to_string(&m).expect("serialize");
        let back: MaterialHandle = ron::from_str(&s).expect("deserialize");
        assert_eq!(m, back);
    }
}
