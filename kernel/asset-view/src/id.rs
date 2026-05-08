//! Asset-view identifier.
//!
//! Mirrors `kernel/io-scheduler::IoRequestId` / `kernel/job-system::JobId` in
//! shape — opaque 16-byte caller-supplied identifier with `const` accessors.

use serde::{Deserialize, Serialize};

/// 16-byte deterministic asset-view identifier.
///
/// v0 stub: caller-supplied bytes; future dispatches may derive the bytes from
/// `(asset_id, view_kind, byte_offset, byte_len)` via BLAKE3 or similar so
/// that two semantically-equivalent views collide deterministically across
/// processes.
///
/// The byte layout is opaque — callers should treat this type as an opaque
/// handle and rely only on the constructors / accessors below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssetViewId([u8; 16]);

impl AssetViewId {
    /// Construct an [`AssetViewId`] from 16 raw bytes.
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

impl std::fmt::Display for AssetViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render as a short hex prefix for legibility in test failure
        // messages and diagnostic output.
        for byte in &self.0[..4] {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_round_trips() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let id = AssetViewId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn zero_value_is_distinct_from_max() {
        let zero = AssetViewId::from_bytes([0u8; 16]);
        let max = AssetViewId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
    }

    #[test]
    fn display_shows_hex_prefix() {
        let id =
            AssetViewId::from_bytes([0xab, 0xcd, 0xef, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(format!("{id}"), "abcdef01");
    }

    #[test]
    fn ord_matches_byte_order() {
        let a = AssetViewId::from_bytes([0u8; 16]);
        let b = AssetViewId::from_bytes([1u8; 16]);
        assert!(a < b);
    }

    #[test]
    fn serde_round_trip_preserves_bytes() {
        let id = AssetViewId::from_bytes([0xa5u8; 16]);
        let json = serde_json::to_string(&id).expect("serialize");
        let decoded: AssetViewId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, decoded);
    }
}
