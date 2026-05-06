//! [`BRepHandle`] — render-side handle for a B-Rep entity's tessellation.
//!
//! Per PLAN.md §1.5.1 the B-Rep entity role is `Transform` + `BRepHandle` +
//! `MaterialHandle` + `Visibility` + `Name`. The handle stores the
//! `CadNodeId` that the cad-core graph emits for the operator and an
//! `AssetId` slot for the cached tessellation lookup (PLAN.md §1.5.4.1 —
//! tessellation cache keyed on `(cad_node_id, tolerance, lod_bucket)`).

use serde::{Deserialize, Serialize};

use crate::{AssetId, CadNodeId, NULL_ASSET_ID};

/// "This entity draws by tessellating the cad-core node with the given id;
/// the renderer caches the result under the given asset slot."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BRepHandle {
    /// Operator-graph node this entity represents.
    pub cad_node: CadNodeId,
    /// Cached tessellation, when resident in the asset store. Use
    /// [`NULL_ASSET_ID`] to represent "not yet tessellated".
    pub tessellation: AssetId,
}

impl BRepHandle {
    /// Construct a B-Rep handle for the given operator-graph node, with no
    /// tessellation cached yet.
    #[inline]
    #[must_use]
    pub const fn new(cad_node: CadNodeId) -> Self {
        Self {
            cad_node,
            tessellation: NULL_ASSET_ID,
        }
    }
}

impl Default for BRepHandle {
    fn default() -> Self {
        Self::new(CadNodeId::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let b = BRepHandle {
            cad_node: CadNodeId(7),
            tessellation: AssetId::from_bytes(b"brep-tess-fixture-1"),
        };
        let s = ron::to_string(&b).expect("serialize");
        let back: BRepHandle = ron::from_str(&s).expect("deserialize");
        assert_eq!(b, back);
    }

    #[test]
    fn new_starts_with_null_tess() {
        let b = BRepHandle::new(CadNodeId(11));
        assert_eq!(b.tessellation, NULL_ASSET_ID);
        assert_eq!(b.cad_node, CadNodeId(11));
    }
}
