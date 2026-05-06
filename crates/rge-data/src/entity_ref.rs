// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for Project/Scene/Prefab
//                                                                  identity scheme.
//
//! [`EntityId`] — scene-stable ULID for entities inside a `.rge-scene` /
//! `.rge-prefab`.
//!
//! Per `PLAN.md` §1.6.3 the file format identity scheme uses ULID with a
//! `Display` impl that truncates to `e_<8 hex chars>` for editor diagnostics.
//! Full 26-char Crockford-base32 ULID round-trips through serde so on-disk
//! identity remains stable; the truncated display is for human-readable
//! references in inspector / diagnostic spans only.
//!
//! # Display vs. serde
//!
//! - On disk (RON): the full 26-character ULID string (e.g. `"01H...XYZ"`),
//!   round-trip stable.
//! - In `Display`: an 8-hex-character prefix prefixed by `e_` (e.g.
//!   `e_abc12345`). This is **lossy** — never re-parse a `Display` string
//!   back into an `EntityId`.
//!
//! # Why not `u64`?
//!
//! ULIDs sort lexicographically by creation time, which makes deterministic
//! scene-cook (PLAN.md §1.6.10) trivial: feed the cook a deterministic seed
//! and `EntityId`s come out monotone. A bare `u64` would require a separate
//! creation-order index, so we lean on the canonical scheme.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Scene-stable ULID identity for an entity. See module docs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(pub Ulid);

impl EntityId {
    /// The all-zero (`nil`) ULID. Useful as a sentinel — never produced by
    /// random construction. Round-trips through serde as the canonical
    /// 26-char zero ULID string.
    pub const NIL: Self = Self(Ulid(0));

    /// Construct from a raw 128-bit value. `const`-friendly so fixtures and
    /// tests can build deterministic IDs.
    #[must_use]
    pub const fn from_u128(v: u128) -> Self {
        Self(Ulid(v))
    }

    /// The raw 128-bit value. Inverse of [`Self::from_u128`].
    #[must_use]
    pub const fn to_u128(self) -> u128 {
        self.0 .0
    }

    /// Borrow the underlying [`Ulid`].
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)] // borrow returned must outlive `&self`.
    pub const fn as_ulid(&self) -> &Ulid {
        &self.0
    }

    /// Format the ID as the canonical 26-char Crockford-base32 string used
    /// on disk. Round-trips back through [`FromStr`].
    #[must_use]
    pub fn to_canonical(self) -> String {
        self.0.to_string()
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::NIL
    }
}

/// Display format: `e_<first 8 hex digits of the random low half>`.
///
/// Lossy — for inspector / diagnostics only. Per `PLAN.md` §1.6.3 the
/// truncated form is `e_<8 hex chars>`. The 8 hex digits come from the top
/// 32 bits of the ULID's lower 64-bit randomness half.
impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // ULID layout: 48 bits timestamp ‖ 80 bits randomness, big-endian.
        // We pull the **top 32 bits** of the lower 64-bit word, which is the
        // first 32 bits of the 80-bit randomness section that fall outside
        // the upper 64 bits — a stable, deterministic slice.
        let bytes = self.0 .0.to_be_bytes();
        let prefix = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        write!(f, "e_{prefix:08x}")
    }
}

impl FromStr for EntityId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Ulid::from_string(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nil_round_trip_ron() {
        let e = EntityId::NIL;
        let s = ron::to_string(&e).expect("serialize");
        let back: EntityId = ron::from_str(&s).expect("deserialize");
        assert_eq!(e, back);
    }

    #[test]
    fn display_format_is_e_underscore_8hex() {
        // Build a ULID with a known low half: bytes 8..12 = ABCD1234.
        // We pack value = (hi << 64) | lo, where lo's high 32 bits are
        // bytes 8..12 of the 16-byte big-endian repr → 0xABCD_1234.
        let id = EntityId::from_u128((1u128 << 64) | 0xABCD_1234_5678_9ABC_u128);
        let s = format!("{id}");
        assert_eq!(s, "e_abcd1234");
        assert_eq!(s.len(), 10); // `e_` + 8 hex chars.
        assert!(s.starts_with("e_"));
    }

    #[test]
    fn display_format_zero_padding() {
        // High 32 bits of the lower 64-bit word are zero → expect zero-pad.
        let id = EntityId::from_u128(1u128 << 64);
        assert_eq!(format!("{id}"), "e_00000000");
    }

    #[test]
    fn canonical_roundtrip_through_str() {
        let id = EntityId::from_u128(0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210_u128);
        let s = id.to_canonical();
        let back: EntityId = s.parse().expect("parse");
        assert_eq!(id, back);
    }

    #[test]
    fn ord_is_by_ulid_value() {
        let a = EntityId::from_u128(1);
        let b = EntityId::from_u128(2);
        assert!(a < b);
    }

    #[test]
    fn serde_emits_canonical_string() {
        // Transparent serde wrapping → ULID's serde format (string).
        let id = EntityId::from_u128(0);
        let s = ron::to_string(&id).expect("serialize");
        assert!(s.contains('"'), "expected string-form ULID, got {s}");
    }

    #[test]
    fn nil_default() {
        assert_eq!(EntityId::default(), EntityId::NIL);
        assert_eq!(EntityId::NIL.to_u128(), 0);
    }
}
