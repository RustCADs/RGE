// adapted from rustforge::crates::release-manifest on 2026-05-05 — generalized
//                                                                   for source-file
//                                                                   schema versioning.
//
//! [`SchemaVersion`] — the `version: "x.y.z"` field at the top of every
//! `.rge-project` / `.rge-scene` / `.rge-prefab`.
//!
//! Per `PLAN.md` §1.6.7 every source file carries a SemVer-shaped
//! `SchemaVersion`; the loader reads it, walks the [`migration`](crate::migration)
//! chain, and yields a struct deserialized at the current schema.
//!
//! # Wire format
//!
//! Stored as a single dotted-triple string `"major.minor.patch"`. We
//! deliberately serialize as a string (not a struct of three `u8`s) so the
//! source RON looks like:
//!
//! ```text
//! Project (
//!     version: "0.1.0",
//!     ...
//! )
//! ```
//!
//! rather than
//!
//! ```text
//! Project (
//!     version: SchemaVersion(major: 0, minor: 1, patch: 0),
//!     ...
//! )
//! ```
//!
//! …because the former is what humans expect. Round-trips through serde via
//! a hand-rolled visitor.
//!
//! # u8 components
//!
//! Per the W14 dispatch package the components are `u8`. We don't need
//! larger ranges — RGE plans to bump major versions yearly at most, and
//! 256 majors is a hundred years of releases. Small types keep the AST
//! representation tight and serde JSON / RON output compact.
//!
//! # Cross-ref
//!
//! `kernel/types::SchemaVersion` (W02) uses `u16` and is geared toward
//! per-Reflect-type version tags. This crate's `SchemaVersion` is the
//! file-format header version — a different lifetime / cadence. Phase 4
//! deliberately keeps the two distinct: type-schema versions bump on every
//! field add (high churn), file-format versions bump on every breaking
//! migration (low churn).

use core::fmt;
use core::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// SemVer-flavoured `(major, minor, patch)` triple for source-file schemas.
///
/// `Ord` is lexicographic (major dominates, then minor, then patch).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaVersion {
    /// Major — breaking changes; new migration entry required.
    pub major: u8,
    /// Minor — additive, default-compatible changes.
    pub minor: u8,
    /// Patch — pure renames, doc-only, or no semantic change.
    pub patch: u8,
}

/// Errors returned by [`SchemaVersion::from_str`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SchemaVersionParseError {
    /// Input wasn't `major.minor.patch` (3 dot-separated numeric components).
    #[error("expected `major.minor.patch`, got `{0}`")]
    MalformedShape(String),
    /// One of the components didn't fit in `u8`.
    #[error("component out of range (`{0}` exceeds u8::MAX)")]
    ComponentOutOfRange(String),
    /// One of the components wasn't numeric.
    #[error("non-numeric component `{0}`")]
    NonNumeric(String),
}

impl SchemaVersion {
    /// Construct a triple. `const`-friendly so callers can declare schema
    /// versions at compile time:
    ///
    /// ```ignore
    /// const PROJECT_V0_1: SchemaVersion = SchemaVersion::new(0, 1, 0);
    /// ```
    #[must_use]
    pub const fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// `0.0.0` — the implicit baseline used by initial fixtures and the
    /// "before any migration" sentinel.
    pub const V0_0_0: Self = Self::new(0, 0, 0);

    /// `0.1.0` — the first migration target on the v0.0 → v0.1 chain.
    pub const V0_1_0: Self = Self::new(0, 1, 0);

    /// True if both versions share the same major. Same-major data is
    /// load-compatible after at most an additive minor migration.
    #[must_use]
    pub const fn is_compatible_with(self, other: Self) -> bool {
        self.major == other.major
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for SchemaVersion {
    type Err = SchemaVersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('.');
        let parse_part = |p: Option<&str>| -> Result<u8, SchemaVersionParseError> {
            let comp = p.ok_or_else(|| SchemaVersionParseError::MalformedShape(s.to_string()))?;
            // Reject leading-/trailing-space components and empty fields.
            if comp.is_empty() || comp.chars().any(|c| !c.is_ascii_digit()) {
                return Err(SchemaVersionParseError::NonNumeric(comp.to_string()));
            }
            comp.parse::<u8>()
                .map_err(|_| SchemaVersionParseError::ComponentOutOfRange(comp.to_string()))
        };
        let major = parse_part(parts.next())?;
        let minor = parse_part(parts.next())?;
        let patch = parse_part(parts.next())?;
        if parts.next().is_some() {
            return Err(SchemaVersionParseError::MalformedShape(s.to_string()));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

// Hand-rolled serde so the on-disk form is a quoted dotted string.
impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        s.parse::<SchemaVersion>().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_lexicographic() {
        assert!(SchemaVersion::new(1, 0, 0) > SchemaVersion::new(0, 9, 99));
        assert!(SchemaVersion::new(1, 1, 0) > SchemaVersion::new(1, 0, 99));
        assert!(SchemaVersion::new(1, 0, 1) > SchemaVersion::new(1, 0, 0));
        assert_eq!(SchemaVersion::new(2, 3, 4), SchemaVersion::new(2, 3, 4));
    }

    #[test]
    fn major_compat_check() {
        assert!(SchemaVersion::new(1, 0, 0).is_compatible_with(SchemaVersion::new(1, 5, 3)));
        assert!(!SchemaVersion::new(1, 0, 0).is_compatible_with(SchemaVersion::new(2, 0, 0)));
    }

    #[test]
    fn display_is_dotted_triple() {
        assert_eq!(SchemaVersion::new(0, 1, 0).to_string(), "0.1.0");
        assert_eq!(SchemaVersion::new(2, 17, 99).to_string(), "2.17.99");
    }

    #[test]
    fn from_str_basic() {
        assert_eq!(
            "0.1.0".parse::<SchemaVersion>().unwrap(),
            SchemaVersion::V0_1_0
        );
        assert_eq!(
            "10.20.30".parse::<SchemaVersion>().unwrap(),
            SchemaVersion::new(10, 20, 30)
        );
    }

    #[test]
    fn from_str_rejects_too_few_components() {
        assert!(matches!(
            "0.1".parse::<SchemaVersion>(),
            Err(SchemaVersionParseError::MalformedShape(_) | SchemaVersionParseError::NonNumeric(_))
        ));
    }

    #[test]
    fn from_str_rejects_too_many_components() {
        assert!(matches!(
            "0.1.0.1".parse::<SchemaVersion>(),
            Err(SchemaVersionParseError::MalformedShape(_))
        ));
    }

    #[test]
    fn from_str_rejects_non_numeric() {
        assert!(matches!(
            "0.1.x".parse::<SchemaVersion>(),
            Err(SchemaVersionParseError::NonNumeric(_))
        ));
    }

    #[test]
    fn from_str_rejects_overflow() {
        assert!(matches!(
            "0.0.300".parse::<SchemaVersion>(),
            Err(SchemaVersionParseError::ComponentOutOfRange(_))
        ));
    }

    #[test]
    fn ron_round_trip_as_quoted_string() {
        let v = SchemaVersion::new(1, 2, 3);
        let s = ron::to_string(&v).expect("serialize");
        assert_eq!(s, "\"1.2.3\"");
        let back: SchemaVersion = ron::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn default_is_zero_zero_zero() {
        assert_eq!(SchemaVersion::default(), SchemaVersion::V0_0_0);
    }
}
