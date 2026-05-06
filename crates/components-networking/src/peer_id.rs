//! Wave-W01 local stub for the network peer identifier.
//!
//! Replaced by `crates/replication::PeerId` (post-v1). The `u64` payload is
//! the eventual cluster-issued peer id (typically a hash of the peer's
//! cluster-cert public key truncated to 64 bits).

use serde::{Deserialize, Serialize};

/// Opaque peer identifier (W01-local stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct PeerId(pub u64);

impl PeerId {
    /// Reserved "this machine" id used by single-player builds. Setting
    /// owner to `LOCAL` is equivalent to omitting `NetworkOwner`.
    pub const LOCAL: PeerId = PeerId(0);
}

impl Default for PeerId {
    fn default() -> Self {
        Self::LOCAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let p = PeerId(0x1234);
        let s = ron::to_string(&p).expect("serialize");
        let back: PeerId = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn local_is_zero() {
        assert_eq!(PeerId::LOCAL.0, 0);
    }
}
