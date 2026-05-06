//! [`Replicated`] — zero-sized "this entity participates in replication" marker.
//!
//! No payload — the replication system uses presence only. Scope policy
//! (every-tick / on-change / interest-management) lives in the separate
//! [`crate::ReplicationPolicy`] component when needed.

use serde::{Deserialize, Serialize};

/// Zero-sized "replicated" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Replicated;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let r = Replicated;
        let s = ron::to_string(&r).expect("serialize");
        let back: Replicated = ron::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<Replicated>(), 0);
    }
}
