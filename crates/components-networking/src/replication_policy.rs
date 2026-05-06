//! [`ReplicationPolicy`] — per-entity replication scope.
//!
//! Optional sibling of [`crate::Replicated`]. When absent, the replication
//! system applies the world's default (typically `OnChange`).

use serde::{Deserialize, Serialize};

/// Replication scope.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplicationPolicy {
    /// Replicate every tick — wasteful but simple. Used for player avatars.
    EveryTick,
    /// Replicate only when a tracked component changes (default).
    #[default]
    OnChange,
    /// Replicate only when in interest of at least one observer (relevance
    /// filtering — for large worlds with thousands of NPCs).
    InterestManaged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_every_tick() {
        let p = ReplicationPolicy::EveryTick;
        let s = ron::to_string(&p).expect("serialize");
        let back: ReplicationPolicy = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn round_trip_ron_on_change() {
        let p = ReplicationPolicy::OnChange;
        let s = ron::to_string(&p).expect("serialize");
        let back: ReplicationPolicy = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn round_trip_ron_interest_managed() {
        let p = ReplicationPolicy::InterestManaged;
        let s = ron::to_string(&p).expect("serialize");
        let back: ReplicationPolicy = ron::from_str(&s).expect("deserialize");
        assert_eq!(p, back);
    }

    #[test]
    fn default_is_on_change() {
        assert_eq!(ReplicationPolicy::default(), ReplicationPolicy::OnChange);
    }
}
