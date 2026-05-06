// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for content-addressed
//                                                                  asset identity.
//
//! Re-export of canonical [`AssetId`] from `rge-kernel-asset` (Phase 4.1).
//!
//! The canonical type lives in `kernel/asset`; this module re-exports it so
//! downstream code inside `rge-data` continues to resolve the same names.
//! `AssetIdParseError` is also re-exported for callers that match on parse
//! failures.
//!
//! # Migration note (W14 → Phase 4.1)
//!
//! The previous local `AssetId` had:
//! - `from_bytes([u8; 32])` (const-friendly raw-digest constructor) →
//!   now `from_raw([u8; 32])` on the kernel type.
//! - `from_content(&[u8])` (blake3 hash constructor) →
//!   now `from_bytes(&[u8])` on the kernel type.
//! - `as_bytes()` accessor → now `raw()` on the kernel type.
//! - `AssetIdParseError::WrongLength(n)` / `NonHex(n)` →
//!   now `BadLength { expected, got }` / `BadHexChar(char)` on the kernel type.
//! - `empty()` sentinel (blake3 of `b""`) → compute inline with
//!   `AssetId::from_bytes(b"")` if needed; there is no zero-value sentinel.

pub use rge_kernel_asset::{AssetId, AssetIdParseError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_is_stable_for_same_content() {
        let a = AssetId::from_bytes(b"hello, world");
        let b = AssetId::from_bytes(b"hello, world");
        assert_eq!(a, b);
    }

    #[test]
    fn from_bytes_is_distinct_for_distinct_content() {
        let a = AssetId::from_bytes(b"hello, world");
        let b = AssetId::from_bytes(b"hello, world!");
        assert_ne!(a, b);
    }

    #[test]
    fn empty_blake3_canonical() {
        // BLAKE3 of empty input — known canonical hash. This is a
        // regression guard that the dependency hasn't silently changed.
        let id = AssetId::from_bytes(b"");
        assert_eq!(
            id.to_string(),
            "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn display_format_has_prefix_and_64_hex() {
        let id = AssetId::from_bytes(b"abc");
        let s = id.to_string();
        assert!(s.starts_with("blake3:"));
        let body = s.strip_prefix("blake3:").unwrap();
        assert_eq!(body.len(), 64);
        assert!(body.bytes().all(|c| c.is_ascii_hexdigit()));
        assert!(body.bytes().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn fromstr_round_trip() {
        let id = AssetId::from_bytes(b"abc");
        let s = id.to_string();
        let back: AssetId = s.parse().expect("parse");
        assert_eq!(id, back);
    }

    #[test]
    fn fromstr_rejects_missing_prefix() {
        let s = "deadbeef".repeat(8); // 64 hex chars, no prefix.
        assert_eq!(s.parse::<AssetId>(), Err(AssetIdParseError::MissingPrefix));
    }

    #[test]
    fn fromstr_rejects_wrong_length() {
        let err = "blake3:deadbeef".parse::<AssetId>().unwrap_err();
        assert!(matches!(
            err,
            AssetIdParseError::BadLength {
                expected: 64,
                got: 8
            }
        ));
    }

    #[test]
    fn fromstr_rejects_non_hex() {
        let mut s = String::from("blake3:");
        s.push_str(&"g".repeat(64));
        assert!(matches!(
            s.parse::<AssetId>(),
            Err(AssetIdParseError::BadHex)
        ));
    }

    #[test]
    fn ron_round_trip_yields_string() {
        let id = AssetId::from_bytes(b"sample");
        let s = ron::to_string(&id).expect("serialize");
        assert!(s.starts_with('"') && s.ends_with('"'));
        let back: AssetId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn from_raw_round_trips_through_raw() {
        let raw = [0x42u8; 32];
        let id = AssetId::from_raw(raw);
        assert_eq!(*id.raw(), raw);
    }
}
