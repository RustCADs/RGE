//! [`NetworkOwner`] — names the peer that owns this entity for prediction
//! and authority.

use serde::{Deserialize, Serialize};

use crate::PeerId;

/// "The peer with this id owns this entity" — used for client-side prediction
/// and for routing input to the right simulation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct NetworkOwner(pub PeerId);

impl NetworkOwner {
    /// Owner = local machine.
    #[inline]
    #[must_use]
    pub const fn local() -> Self {
        Self(PeerId::LOCAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let o = NetworkOwner(PeerId(42));
        let s = ron::to_string(&o).expect("serialize");
        let back: NetworkOwner = ron::from_str(&s).expect("deserialize");
        assert_eq!(o, back);
    }
}
