//! [`RemotePeer`] — entity represents a remote peer (other player).
//!
//! Lets gameplay code distinguish "my machine's avatar" (`NetworkOwner` =
//! local, no `RemotePeer`) from "another machine's avatar" without a
//! sentinel-checking convention.

use serde::{Deserialize, Serialize};

use crate::PeerId;

/// "This entity is the avatar / proxy for the named remote peer."
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct RemotePeer(pub PeerId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let r = RemotePeer(PeerId(0x55));
        let s = ron::to_string(&r).expect("serialize");
        let back: RemotePeer = ron::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }
}
