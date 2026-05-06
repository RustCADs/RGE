//! [`CadRef`] — points at a node in the cad-core operator graph by id.
//!
//! Used by the B-Rep entity role (PLAN.md §1.5.1) alongside `BRepHandle` —
//! the `BRepHandle` carries the GPU-residency tessellation; `CadRef` keeps
//! the ECS entity authoritatively pinned to its operator-graph node so
//! topology lineage (PLAN.md §1.5.4.3) survives history rebuilds.

use serde::{Deserialize, Serialize};

use crate::CadNodeId;

/// "This entity is backed by the cad-core node with the given id."
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct CadRef(pub CadNodeId);

impl CadRef {
    /// Wrap a [`CadNodeId`] in a cad-ref component.
    #[inline]
    #[must_use]
    pub const fn new(id: CadNodeId) -> Self {
        Self(id)
    }

    /// Borrow the underlying cad node id.
    #[inline]
    #[must_use]
    pub const fn id(&self) -> CadNodeId {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let r = CadRef::new(CadNodeId(0xfeed_face));
        let s = ron::to_string(&r).expect("serialize");
        let back: CadRef = ron::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }
}
