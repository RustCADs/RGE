//! Residency lifecycle state — 4-tier taxonomy for the residency tracker.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a tracked asset's residency.
///
/// 4-tier lifecycle progression (no load-priority semantics — `Ord` order
/// reflects the lifecycle, not scheduling priority):
///
/// 1. `NotResident` — not loaded; tracker entry exists for the id but no
///    backing storage is held.
/// 2. `Loading`     — load in progress; backing storage being filled.
/// 3. `Resident`    — loaded and available for read.
/// 4. `Unloading`   — unload in progress; backing storage being released.
///
/// `Ord` derives lifecycle progression (`NotResident < Loading < Resident <
/// Unloading`) so consumers can sort lifecycle samples chronologically; the
/// ordering is **not** a load-priority signal — that lives in
/// `kernel/io-scheduler::Priority`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResidencyState {
    /// Not loaded. Tracker entry exists but no backing storage is held.
    NotResident,
    /// Load in progress. Backing storage being filled.
    Loading,
    /// Loaded and available for read.
    Resident,
    /// Unload in progress. Backing storage being released.
    Unloading,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ord_lifecycle_progression() {
        assert!(ResidencyState::NotResident < ResidencyState::Loading);
        assert!(ResidencyState::Loading < ResidencyState::Resident);
        assert!(ResidencyState::Resident < ResidencyState::Unloading);
        // Transitivity sanity.
        assert!(ResidencyState::NotResident < ResidencyState::Unloading);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        let original = ResidencyState::Loading;
        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: ResidencyState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn non_exhaustive_pattern_compiles_via_default_arm() {
        // Cross-crate consumers must use a wildcard arm; this fixture asserts
        // the convention compiles. The `unreachable_patterns` allow guards
        // against the in-crate rebuild where every variant is in fact named.
        #[allow(
            unreachable_patterns,
            reason = "cross-crate consumer pattern — wildcard required"
        )]
        fn label(s: ResidencyState) -> &'static str {
            match s {
                ResidencyState::NotResident => "not_resident",
                ResidencyState::Loading => "loading",
                ResidencyState::Resident => "resident",
                ResidencyState::Unloading => "unloading",
                _ => "unknown",
            }
        }
        assert_eq!(label(ResidencyState::NotResident), "not_resident");
        assert_eq!(label(ResidencyState::Resident), "resident");
    }
}
