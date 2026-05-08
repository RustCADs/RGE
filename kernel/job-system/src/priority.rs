//! Job priority — 4-tier taxonomy for the work scheduler.

use serde::{Deserialize, Serialize};

/// Scheduling priority for a job submitted to [`crate::JobScheduler`].
///
/// Generic 4-tier taxonomy for distinguishing work urgency:
///
/// 1. `Critical` — must complete before any non-critical work proceeds.
/// 2. `High`     — should complete this frame; preempts `Normal` and below.
/// 3. `Normal`   — best-effort default for routine work.
/// 4. `Background` — eligible for deferral; runs only when no higher-priority
///    work is pending.
///
/// `Ord` maps directly to scheduling priority: lower discriminant = higher
/// scheduling priority. The ordering is total and stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum JobPriority {
    /// Critical — must complete before any lower-priority work.
    Critical,
    /// High — should complete this frame.
    High,
    /// Normal — best-effort default.
    Normal,
    /// Background — eligible for deferral.
    Background,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ord_total_ordering_lower_discriminant_is_higher_priority() {
        assert!(JobPriority::Critical < JobPriority::High);
        assert!(JobPriority::High < JobPriority::Normal);
        assert!(JobPriority::Normal < JobPriority::Background);
        // Transitivity sanity.
        assert!(JobPriority::Critical < JobPriority::Background);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        let original = JobPriority::High;
        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: JobPriority = serde_json::from_str(&json).expect("deserialize");
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
        fn label(p: JobPriority) -> &'static str {
            match p {
                JobPriority::Critical => "critical",
                JobPriority::High => "high",
                JobPriority::Normal => "normal",
                JobPriority::Background => "background",
                _ => "unknown",
            }
        }
        assert_eq!(label(JobPriority::Critical), "critical");
        assert_eq!(label(JobPriority::Background), "background");
    }
}
