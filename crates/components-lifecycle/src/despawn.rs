//! [`Despawn`] — zero-sized "queued for removal" marker.
//!
//! Adding `Despawn` to an entity flags it for end-of-frame teardown. Until
//! the lifecycle system processes the queue, queries against the entity
//! still see its components — this matters for replication snapshots that
//! need to emit a final state before deletion.

use serde::{Deserialize, Serialize};

/// Zero-sized "scheduled for despawn" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Despawn;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let d = Despawn;
        let txt = ron::to_string(&d).expect("serialize");
        let back: Despawn = ron::from_str(&txt).expect("deserialize");
        assert_eq!(d, back);
    }
}
