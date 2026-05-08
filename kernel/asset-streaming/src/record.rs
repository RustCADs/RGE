//! Residency record handle types.

use serde::{Deserialize, Serialize};

use crate::state::ResidencyState;

/// 16-byte deterministic residency-record identifier.
///
/// v0 stub: caller-supplied bytes; future dispatches may derive the bytes
/// from `(asset_id, view_kind, ...)` via BLAKE3 or similar so that two
/// semantically-equivalent records collide deterministically across
/// processes.
///
/// The byte layout is opaque — callers should treat this type as an opaque
/// handle and rely only on the constructors / accessors below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResidencyId([u8; 16]);

impl ResidencyId {
    /// Construct a [`ResidencyId`] from 16 raw bytes.
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

/// Discriminant for the kind of asset whose residency is tracked.
///
/// v0 stub: a single placeholder variant. Real asset kinds (`Mesh` /
/// `Texture` / `Audio` / `Script` / etc.) land in dedicated future
/// dispatches when concrete consumers exist. Marking `#[non_exhaustive]`
/// preserves the freedom to add variants without breaking downstream
/// consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecordKind {
    /// v0 placeholder; real domain-specific variants land in future dispatches.
    Placeholder,
}

/// Carrier for a residency record tracked by the [`crate::ResidencyTracker`].
///
/// v0 stub: minimal payload — `id` + `state` + `kind` + `byte_size`. Future
/// dispatches may extend with hysteresis-deadline / priority handle (into
/// `kernel/io-scheduler`) / source-file path / dependent-record list, all
/// behind dedicated ADRs.
///
/// `byte_size` is informational in v0 — it captures the eventual residency
/// cost so callers can budget memory, but v0 does NOT enforce buffer
/// presence or validate the size against any real allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResidencyRecord {
    /// Unique identifier for this record.
    pub id: ResidencyId,
    /// Current lifecycle state — see [`ResidencyState`].
    pub state: ResidencyState,
    /// Discriminant for the kind of asset.
    pub kind: RecordKind,
    /// Eventual residency cost in bytes. Informational only in v0; future
    /// dispatches may enforce against memory budgets.
    pub byte_size: u64,
}

impl ResidencyRecord {
    /// Construct a [`ResidencyRecord`] from owned components.
    #[must_use]
    pub fn new(id: ResidencyId, state: ResidencyState, kind: RecordKind, byte_size: u64) -> Self {
        Self {
            id,
            state,
            kind,
            byte_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residency_id_from_bytes_round_trips() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let id = ResidencyId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn residency_id_zero_value_is_distinct_from_max() {
        let zero = ResidencyId::from_bytes([0u8; 16]);
        let max = ResidencyId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
    }

    #[test]
    fn residency_record_constructs_with_explicit_fields() {
        let id = ResidencyId::from_bytes([1u8; 16]);
        let r = ResidencyRecord::new(id, ResidencyState::Resident, RecordKind::Placeholder, 1024);
        assert_eq!(r.id.as_bytes(), &[1u8; 16]);
        assert_eq!(r.state, ResidencyState::Resident);
        assert_eq!(r.kind, RecordKind::Placeholder);
        assert_eq!(r.byte_size, 1024);
    }

    #[test]
    fn residency_record_serde_round_trip_preserves_all_fields() {
        let r = ResidencyRecord::new(
            ResidencyId::from_bytes([7u8; 16]),
            ResidencyState::Loading,
            RecordKind::Placeholder,
            42,
        );
        let json = serde_json::to_string(&r).expect("serialize");
        let decoded: ResidencyRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(r, decoded);
    }

    #[test]
    fn record_kind_non_exhaustive_pattern_compiles_via_default_arm() {
        #[allow(
            unreachable_patterns,
            reason = "cross-crate consumer pattern — wildcard required"
        )]
        fn label(k: &RecordKind) -> &'static str {
            match k {
                RecordKind::Placeholder => "placeholder",
                _ => "unknown",
            }
        }
        assert_eq!(label(&RecordKind::Placeholder), "placeholder");
    }
}
