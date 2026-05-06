//! [`Authoritative`] — zero-sized "this peer holds authority for this entity"
//! marker.
//!
//! Distinct from [`crate::NetworkOwner`]: ownership describes *who-controls-
//! the-input-for*; authority describes *who-resolves-state-conflicts-for*.
//! The two usually but do not always coincide (server-authoritative MMOs
//! split them; lockstep RTS games unify them).

use serde::{Deserialize, Serialize};

/// Zero-sized "this peer is authoritative for this entity" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Authoritative;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let a = Authoritative;
        let s = ron::to_string(&a).expect("serialize");
        let back: Authoritative = ron::from_str(&s).expect("deserialize");
        assert_eq!(a, back);
    }
}
