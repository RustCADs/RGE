//! [`MeshHandle`] — typed handle into the GPU residency cache for triangle
//! meshes.
//!
//! Distinct from a raw `AssetId`: the handle is the resolved-and-uploaded
//! result, so the renderer can iterate `Query<&MeshHandle>` without per-frame
//! asset-store lookups. Asset eviction strips the handle component; the
//! `AssetRef` (in components-identity) survives so the streamer can
//! re-resolve.

use serde::{Deserialize, Serialize};

use crate::{AssetId, NULL_ASSET_ID};

/// "This entity draws using the mesh asset with the given id."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct MeshHandle(pub AssetId);

impl Default for MeshHandle {
    fn default() -> Self {
        Self(NULL_ASSET_ID)
    }
}

impl MeshHandle {
    /// Wrap an [`AssetId`] in a mesh-handle component.
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
        let m = MeshHandle::new(AssetId::from_bytes(b"mesh-handle-fixture-1"));
        let s = ron::to_string(&m).expect("serialize");
        let back: MeshHandle = ron::from_str(&s).expect("deserialize");
        assert_eq!(m, back);
    }
}
