//! IO request handle types.

use serde::{Deserialize, Serialize};

use crate::priority::Priority;

/// 16-byte deterministic request identifier.
///
/// v0 stub: caller-supplied bytes; future dispatches may derive the bytes from
/// request content via BLAKE3 or similar so that two semantically-equivalent
/// requests collide deterministically across processes.
///
/// The byte layout is opaque — callers should treat this type as an opaque
/// handle and rely only on the constructors / accessors below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IoRequestId([u8; 16]);

impl IoRequestId {
    /// Construct an [`IoRequestId`] from 16 raw bytes.
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

/// Discriminant for the kind of IO request.
///
/// v0 stub: a single placeholder variant. Real driver kinds (`FileLoad` /
/// `NetFetch` / `MemoryMap` / etc.) land in dedicated future dispatches when
/// their dispatch tables and driver crates exist. Marking `#[non_exhaustive]`
/// preserves the freedom to add variants without breaking downstream
/// consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IoRequestKind {
    /// v0 placeholder; real driver-specific variants land in future dispatches.
    Placeholder,
}

/// Carrier for an IO request submitted to the scheduler.
///
/// v0 stub: minimal payload — `id` + `priority` + `kind` discriminant. Future
/// dispatches may extend with payload bytes / completion-callback handles /
/// timeouts / cancellation tokens, all behind dedicated ADRs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IoRequest {
    /// Unique identifier for this request.
    pub id: IoRequestId,
    /// Streaming priority — see [`Priority`] for the 4-tier taxonomy.
    pub priority: Priority,
    /// Discriminant for the kind of IO requested.
    pub kind: IoRequestKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_request_id_from_bytes_round_trips() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let id = IoRequestId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn io_request_id_zero_value_is_distinct_from_max() {
        let zero = IoRequestId::from_bytes([0u8; 16]);
        let max = IoRequestId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
    }

    #[test]
    fn io_request_constructs_with_explicit_fields() {
        let id = IoRequestId::from_bytes([1u8; 16]);
        let req = IoRequest {
            id,
            priority: Priority::InFrustumNear,
            kind: IoRequestKind::Placeholder,
        };
        assert_eq!(req.id.as_bytes(), &[1u8; 16]);
        assert_eq!(req.priority, Priority::InFrustumNear);
        assert_eq!(req.kind, IoRequestKind::Placeholder);
    }

    #[test]
    fn io_request_serde_round_trip_preserves_all_fields() {
        let req = IoRequest {
            id: IoRequestId::from_bytes([7u8; 16]),
            priority: Priority::OutOfFrustumFar,
            kind: IoRequestKind::Placeholder,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let decoded: IoRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, decoded);
    }

    #[test]
    fn io_request_kind_non_exhaustive_pattern_compiles_via_default_arm() {
        #[allow(
            unreachable_patterns,
            reason = "cross-crate consumer pattern — wildcard required"
        )]
        fn label(k: &IoRequestKind) -> &'static str {
            match k {
                IoRequestKind::Placeholder => "placeholder",
                _ => "unknown",
            }
        }
        assert_eq!(label(&IoRequestKind::Placeholder), "placeholder");
    }
}
