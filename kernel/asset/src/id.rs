//! Content-addressed asset identifier.
//!
//! [`AssetId`] is the blake3 hash of an asset's byte image, formatted as
//! `"blake3:<64-hex-chars-lowercase>"` for stable string-form serialisation.
//!
//! # Determinism
//!
//! blake3 is a pure function of input bytes — same bytes on two different
//! machines produce the same hash. A few known-vector tests pin this so an
//! accidental blake3 dep bump that changes the algorithm shows up here, not
//! as a silent corruption of every existing pak file.
//!
//! # Why string-form serialisation
//!
//! The `Display` / serde representations use the explicit `"blake3:<hex>"`
//! string rather than raw bytes. Three reasons:
//! 1. `.rge-scene` / `.rge-project` files are RON — humans benefit from
//!    being able to grep for an asset reference.
//! 2. The `blake3:` prefix is a discriminator for future algorithm migration
//!    (ADR-077 escape-clause discipline).
//! 3. URL-safe and filename-safe.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// blake3 hex digest length in characters (32 bytes × 2 hex chars/byte = 64).
const HEX_LEN: usize = 64;

/// String-form prefix that discriminates blake3 from any future hash family.
const PREFIX: &str = "blake3:";

/// Content-addressed asset identifier.
///
/// Round-trips through `Display` / `FromStr` / serde as
/// `"blake3:<64-hex-chars-lowercase>"`.  Equality and hashing operate on the
/// underlying 32-byte blake3 digest — the string form is canonical on
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId {
    /// Raw 32-byte blake3 hash. Held in fixed-size form (not the
    /// `blake3::Hash` newtype) so this struct is repr-stable for future
    /// zero-copy work without depending on blake3's struct layout.
    #[serde(with = "serde_string")]
    bytes: [u8; 32],
}

impl AssetId {
    /// Content-hash the supplied bytes and return the resulting [`AssetId`].
    ///
    /// This is the canonical constructor — every other code path that produces
    /// an `AssetId` either calls this or parses a string already produced by
    /// `Display`.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let h = blake3::hash(bytes);
        Self {
            bytes: *h.as_bytes(),
        }
    }

    /// Construct from a raw 32-byte blake3 digest.
    ///
    /// Useful when the digest was produced elsewhere (e.g. streaming hash,
    /// batch-cook tool).
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self { bytes: raw }
    }

    /// Borrow the underlying 32-byte digest.
    #[must_use]
    pub const fn raw(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Full 64-character lowercase hex string (without the `blake3:` prefix).
    #[must_use]
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(HEX_LEN);
        for b in self.bytes {
            out.push(nibble_to_hex(b >> 4));
            out.push(nibble_to_hex(b & 0x0f));
        }
        out
    }
}

#[inline]
fn nibble_to_hex(n: u8) -> char {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    HEX[(n & 0x0f) as usize] as char
}

#[inline]
fn hex_to_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PREFIX)?;
        for b in self.bytes {
            // Direct write avoids an intermediate `String` allocation that
            // `self.hex()` would do.
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Parse error for [`AssetId`] string-form.
///
/// Variants are deliberately granular so a corrupted `.rge-scene` line
/// surfaces *which* part of the `AssetId` string is malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AssetIdParseError {
    /// Input did not start with `blake3:`.
    ///
    /// Future hash families will produce a different prefix; this error is
    /// the discriminator.
    #[error("asset id missing `blake3:` prefix")]
    MissingPrefix,

    /// Hex body has the wrong number of characters.
    ///
    /// blake3 is exactly 32 bytes → exactly 64 lowercase hex chars after the
    /// prefix.
    #[error("asset id hex body wrong length: expected {expected}, got {got}")]
    BadLength {
        /// Expected hex-character count (always 64 for blake3).
        expected: usize,
        /// Actual hex-character count after the `blake3:` prefix.
        got: usize,
    },

    /// A character in the hex body is not `[0-9a-fA-F]`.
    #[error("asset id hex body contains non-hex character")]
    BadHex,
}

impl FromStr for AssetId {
    type Err = AssetIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let body = s
            .strip_prefix(PREFIX)
            .ok_or(AssetIdParseError::MissingPrefix)?;
        if body.len() != HEX_LEN {
            return Err(AssetIdParseError::BadLength {
                expected: HEX_LEN,
                got: body.len(),
            });
        }
        let mut bytes = [0u8; 32];
        let body_bytes = body.as_bytes();
        for (i, dst) in bytes.iter_mut().enumerate() {
            let hi = hex_to_nibble(body_bytes[i * 2]).ok_or(AssetIdParseError::BadHex)?;
            let lo = hex_to_nibble(body_bytes[i * 2 + 1]).ok_or(AssetIdParseError::BadHex)?;
            *dst = (hi << 4) | lo;
        }
        Ok(Self { bytes })
    }
}

// ---------------------------------------------------------------------------
// serde — delegated through Display / FromStr so the on-disk form stays the
// human-readable "blake3:<hex>" string rather than a raw byte array.
// ---------------------------------------------------------------------------

/// Serde helper module: serialise `[u8; 32]` as the full `"blake3:<hex>"`
/// string by delegating to `AssetId`'s `Display` / `FromStr`.
mod serde_string {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::AssetId;

    pub(super) fn serialize<S: Serializer>(bytes: &[u8; 32], ser: S) -> Result<S::Ok, S::Error> {
        let id = AssetId { bytes: *bytes };
        ser.collect_str(&id)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<[u8; 32], D::Error> {
        let s = <String as Deserialize>::deserialize(de)?;
        let id: AssetId = s.parse().map_err(serde::de::Error::custom)?;
        Ok(id.bytes)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;

    #[test]
    fn from_bytes_is_deterministic() {
        let a = AssetId::from_bytes(b"the quick brown fox");
        let b = AssetId::from_bytes(b"the quick brown fox");
        assert_eq!(a, b);
    }

    #[test]
    fn from_bytes_is_blake3_of_input() {
        let id = AssetId::from_bytes(b"hello");
        let expected = blake3::hash(b"hello");
        assert_eq!(id.raw(), expected.as_bytes());
    }

    #[test]
    fn different_bytes_produce_different_ids() {
        let a = AssetId::from_bytes(b"alpha");
        let b = AssetId::from_bytes(b"beta");
        assert_ne!(a, b);
    }

    #[test]
    fn display_is_blake3_prefixed_lowercase_64_hex_chars() {
        let id = AssetId::from_bytes(b"x");
        let s = id.to_string();
        assert!(s.starts_with("blake3:"), "got {s}");
        assert_eq!(s.len(), 7 + 64, "got {s}");
        assert!(
            s[7..]
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "hex body must be lowercase ascii hex: {s}"
        );
    }

    #[test]
    fn hex_matches_blake3_to_hex_helper() {
        let id = AssetId::from_bytes(b"matching");
        let theirs = blake3::hash(b"matching").to_hex().to_string();
        assert_eq!(id.hex(), theirs);
    }

    #[test]
    fn round_trip_through_display_and_from_str() {
        let id = AssetId::from_bytes(b"round-trip me");
        let s = id.to_string();
        let back: AssetId = s.parse().expect("parse");
        assert_eq!(id, back);
    }

    #[test]
    fn from_str_rejects_missing_prefix() {
        let s = "0123456789abcdef".repeat(4);
        let err = AssetId::from_str(&s).unwrap_err();
        assert_eq!(err, AssetIdParseError::MissingPrefix);
    }

    #[test]
    fn from_str_rejects_wrong_length() {
        let err = AssetId::from_str("blake3:dead").unwrap_err();
        assert!(matches!(
            err,
            AssetIdParseError::BadLength {
                expected: 64,
                got: 4
            }
        ));
    }

    #[test]
    fn from_str_rejects_non_hex_character() {
        let mut s = String::from("blake3:");
        s.push_str(&"0".repeat(63));
        s.push('z');
        let err = AssetId::from_str(&s).unwrap_err();
        assert_eq!(err, AssetIdParseError::BadHex);
    }

    #[test]
    fn from_str_accepts_uppercase_hex() {
        // We emit lowercase but accept both.
        let id = AssetId::from_bytes(b"case-mixed");
        let upper = format!("blake3:{}", id.hex().to_ascii_uppercase());
        let parsed: AssetId = upper.parse().expect("uppercase parse");
        assert_eq!(parsed, id);
    }

    #[test]
    fn from_raw_round_trips_through_raw() {
        let raw = [42u8; 32];
        let id = AssetId::from_raw(raw);
        assert_eq!(*id.raw(), raw);
    }

    #[test]
    fn cross_machine_determinism_known_vectors() {
        let cases: &[(&[u8], &str)] = &[
            (
                b"",
                "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            ),
            (
                b"abc",
                "blake3:6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
            ),
        ];
        for (input, expected) in cases {
            let id = AssetId::from_bytes(input);
            assert_eq!(
                id.to_string(),
                *expected,
                "cross-machine determinism broken for input len {}",
                input.len()
            );
        }
    }

    #[test]
    fn hash_and_eq_use_underlying_bytes() {
        use std::collections::HashMap;
        let mut m = HashMap::new();
        m.insert(AssetId::from_bytes(b"k"), 1u32);
        let lookup = AssetId::from_bytes(b"k");
        assert_eq!(m.get(&lookup), Some(&1));
    }

    #[test]
    fn ord_is_total_for_btreemap() {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        m.insert(AssetId::from_bytes(b"b"), 2u32);
        m.insert(AssetId::from_bytes(b"a"), 1u32);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn serde_round_trips_via_string() {
        // Validate that our serde_string module encodes/decodes correctly by
        // exercising round-trip through RON (which we already depend on).
        let id = AssetId::from_bytes(b"serde-test");
        let ron_str = ron::to_string(&id).expect("serialize");
        // The on-wire form must be the quoted "blake3:..." string.
        assert!(ron_str.contains("blake3:"), "on-wire: {ron_str}");
        let back: AssetId = ron::from_str(&ron_str).expect("deserialize");
        assert_eq!(id, back);
    }
}
