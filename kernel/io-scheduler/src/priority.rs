//! Streaming priority — 4-tier taxonomy per PLAN §7.

use serde::{Deserialize, Serialize};

/// Streaming priority for an in-flight IO request.
///
/// Per PLAN §7's 4-tier classification of streaming priorities:
///
/// 1. `InFrustumNear` — assets visible at near range; must be resident now.
/// 2. `InFrustumFar` — assets visible at far range; queue and stream in.
/// 3. `OutOfFrustumNear` — out-of-view but spatially near; predictive load.
/// 4. `OutOfFrustumFar` — out-of-view and far; eligible for eviction.
///
/// `Ord` maps directly to scheduling priority: lower discriminant = higher
/// scheduling priority. The ordering is total and stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Priority {
    /// In-frustum, near range — must be resident.
    InFrustumNear,
    /// In-frustum, far range — queued for streaming.
    InFrustumFar,
    /// Out-of-frustum, near range — predictive prefetch.
    OutOfFrustumNear,
    /// Out-of-frustum, far range — evictable.
    OutOfFrustumFar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ord_total_ordering_lower_discriminant_is_higher_priority() {
        assert!(Priority::InFrustumNear < Priority::InFrustumFar);
        assert!(Priority::InFrustumFar < Priority::OutOfFrustumNear);
        assert!(Priority::OutOfFrustumNear < Priority::OutOfFrustumFar);
        // Transitivity sanity.
        assert!(Priority::InFrustumNear < Priority::OutOfFrustumFar);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        let original = Priority::OutOfFrustumNear;
        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: Priority = serde_json::from_str(&json).expect("deserialize");
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
        fn label(p: Priority) -> &'static str {
            match p {
                Priority::InFrustumNear => "in_frustum_near",
                Priority::InFrustumFar => "in_frustum_far",
                Priority::OutOfFrustumNear => "out_of_frustum_near",
                Priority::OutOfFrustumFar => "out_of_frustum_far",
                _ => "unknown",
            }
        }
        assert_eq!(label(Priority::InFrustumNear), "in_frustum_near");
        assert_eq!(label(Priority::OutOfFrustumFar), "out_of_frustum_far");
    }
}
