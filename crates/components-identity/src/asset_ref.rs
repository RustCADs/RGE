//! [`AssetRef`] — points at a `kernel/asset` payload by id.
//!
//! Distinct from a `MeshHandle` / `MaterialHandle` (which are typed handles
//! into the GPU residency cache); this is the *source* asset reference that
//! the asset-store dereferences into a runtime resource.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// "This entity is backed by the asset with the given id."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct AssetRef(pub AssetId);

impl Default for AssetRef {
    fn default() -> Self {
        Self(NULL_ASSET_ID)
    }
}

impl AssetRef {
    /// Wrap an [`AssetId`] in an asset-ref component.
    #[inline]
    #[must_use]
    pub const fn new(id: AssetId) -> Self {
        Self(id)
    }

    /// Borrow the underlying asset id.
    #[inline]
    #[must_use]
    pub const fn id(&self) -> AssetId {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let a = AssetRef::new(AssetId::from_bytes(b"asset-ref-fixture-1"));
        let s = ron::to_string(&a).expect("serialize");
        let back: AssetRef = ron::from_str(&s).expect("deserialize");
        assert_eq!(a, back);
    }
}
