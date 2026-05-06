//! Wave-W01 local stub for the canonical cad-core node handle.
//!
//! Replaced by `cad-core::TopoId` / `PersistentFaceId` family (PLAN.md
//! §1.5.4.2). The `u64` payload is the operator-graph spawn order.

use serde::{Deserialize, Serialize};

/// Opaque handle into the cad-core operator graph (W01-local stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct CadNodeId(pub u64);

impl CadNodeId {
    /// Sentinel "no cad node bound yet" value.
    pub const NULL: CadNodeId = CadNodeId(0);

    /// Construct a cad node id from a raw integer.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

impl Default for CadNodeId {
    fn default() -> Self {
        Self::NULL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let n = CadNodeId(99);
        let s = ron::to_string(&n).expect("serialize");
        let back: CadNodeId = ron::from_str(&s).expect("deserialize");
        assert_eq!(n, back);
    }
}
