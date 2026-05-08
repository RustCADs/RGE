//! Job handle types.

use serde::{Deserialize, Serialize};

use crate::priority::JobPriority;

/// 16-byte deterministic job identifier.
///
/// v0 stub: caller-supplied bytes; future dispatches may derive the bytes from
/// job content via BLAKE3 or similar so that two semantically-equivalent jobs
/// collide deterministically across processes.
///
/// The byte layout is opaque — callers should treat this type as an opaque
/// handle and rely only on the constructors / accessors below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JobId([u8; 16]);

impl JobId {
    /// Construct a [`JobId`] from 16 raw bytes.
    ///
    /// `const` so callers can build well-known sentinel IDs at compile time.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns a borrow of the underlying byte array.
    ///
    /// `const` so the borrow can flow through `const fn` consumers without
    /// needing a runtime-evaluated copy.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Discriminant for the kind of job.
///
/// v0 stub: a single placeholder variant. Real job kinds (`Compute` /
/// `IoBound` / `RenderPrep` / etc.) land in dedicated future dispatches when
/// concrete consumers exist. Marking `#[non_exhaustive]` preserves the
/// freedom to add variants without breaking downstream consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum JobKind {
    /// v0 placeholder; real domain-specific variants land in future dispatches.
    Placeholder,
}

/// Carrier for a job submitted to the scheduler.
///
/// v0 stub: minimal payload — `id` + `priority` + `kind` discriminant. Future
/// dispatches may extend with payload bytes / completion-callback handles /
/// closure storage / cancellation tokens, all behind dedicated ADRs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// Unique identifier for this job.
    pub id: JobId,
    /// Scheduling priority — see [`JobPriority`] for the 4-tier taxonomy.
    pub priority: JobPriority,
    /// Discriminant for the kind of job.
    pub kind: JobKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_from_bytes_round_trips() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let id = JobId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn job_id_zero_value_is_distinct_from_max() {
        let zero = JobId::from_bytes([0u8; 16]);
        let max = JobId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
    }

    #[test]
    fn job_constructs_with_explicit_fields() {
        let id = JobId::from_bytes([1u8; 16]);
        let job = Job {
            id,
            priority: JobPriority::Critical,
            kind: JobKind::Placeholder,
        };
        assert_eq!(job.id.as_bytes(), &[1u8; 16]);
        assert_eq!(job.priority, JobPriority::Critical);
        assert_eq!(job.kind, JobKind::Placeholder);
    }

    #[test]
    fn job_serde_round_trip_preserves_all_fields() {
        let job = Job {
            id: JobId::from_bytes([7u8; 16]),
            priority: JobPriority::Background,
            kind: JobKind::Placeholder,
        };
        let json = serde_json::to_string(&job).expect("serialize");
        let decoded: Job = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(job, decoded);
    }

    #[test]
    fn job_kind_non_exhaustive_pattern_compiles_via_default_arm() {
        #[allow(
            unreachable_patterns,
            reason = "cross-crate consumer pattern — wildcard required"
        )]
        fn label(k: &JobKind) -> &'static str {
            match k {
                JobKind::Placeholder => "placeholder",
                _ => "unknown",
            }
        }
        assert_eq!(label(&JobKind::Placeholder), "placeholder");
    }
}
