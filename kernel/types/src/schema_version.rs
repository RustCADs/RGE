//! [`SchemaVersion`] — every reflected type carries one.
//!
//! Hot-reload migration (PLAN.md §1.13 snapshot-recoverable failure class)
//! and project-file forward/backward compat depend on a per-type version
//! number. The reflection layer enforces this by making [`SchemaVersion`] a
//! required associated constant on the [`Reflect`](crate::Reflect) trait.
//!
//! # Bumping policy
//!
//! - **Patch** — non-breaking field rename via `#[reflect(serde_alias = "old")]`.
//! - **Minor** — additive field with default; old data still loads.
//! - **Major** — incompatible change; migration entry required in the
//!   `kernel/asset` migration table (Phase 2.4 — not yet built).
//!
//! Phase 1.1 only enforces the constant's presence and serde shape. Migration
//! routing belongs to a later wave.

use core::fmt;

use serde::{Deserialize, Serialize};

/// SemVer-flavoured triple. Stored alongside the reflected payload.
///
/// We deliberately avoid pulling `semver` (heavy + serde-impl bloat). For
/// schema versioning we only need lex compare on (major, minor, patch).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Major version — breaking changes. Bumping requires a migration entry.
    pub major: u16,
    /// Minor version — additive, default-compatible changes.
    pub minor: u16,
    /// Patch — pure renames or doc-only.
    pub patch: u16,
}

impl SchemaVersion {
    /// Construct a version triple. `const`-friendly so derived types can put
    /// `const SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);`.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Sentinel used by the derive macro when the user did not set
    /// `#[reflect(version = "x.y.z")]`. Encodes "the type is reflected but
    /// not yet versioned" — CI lint should warn but not fail in Phase 1.1.
    pub const UNVERSIONED: Self = Self::new(0, 0, 0);

    /// True if `self` and `other` are major-compatible (same major version).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_lexicographic() {
        assert!(SchemaVersion::new(1, 0, 0) > SchemaVersion::new(0, 9, 99));
        assert!(SchemaVersion::new(1, 1, 0) > SchemaVersion::new(1, 0, 99));
        assert!(SchemaVersion::new(1, 0, 1) > SchemaVersion::new(1, 0, 0));
    }

    #[test]
    fn major_compat_check() {
        assert!(SchemaVersion::new(1, 0, 0).is_compatible_with(SchemaVersion::new(1, 5, 3)));
        assert!(!SchemaVersion::new(1, 0, 0).is_compatible_with(SchemaVersion::new(2, 0, 0)));
    }

    #[test]
    fn display_format() {
        assert_eq!(SchemaVersion::new(1, 2, 3).to_string(), "1.2.3");
    }

    #[test]
    fn serde_round_trip() {
        let v = SchemaVersion::new(2, 3, 5);
        let s = ron::to_string(&v).unwrap();
        let back: SchemaVersion = ron::from_str(&s).unwrap();
        assert_eq!(v, back);
    }
}
