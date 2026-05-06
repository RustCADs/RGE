//! Layout-name versioning.
//!
//! UE Slate parallel: `FLayoutSaveRestore::ApplyVersionTransforms` / `LoadFromConfig` versioning.
//! Per PLAN.md §6.6 we mandate a name-suffix versioning rule:
//!
//! ```text
//! rge_main_v0.1.0
//!          ^^^^^^ — semver suffix: bump on schema change.
//! ```
//!
//! Migration policy (PLAN.md §6.6 + §1.10 backwards-compat bar):
//!
//! - **Suffix major/minor changes** trigger a migration pass: tabs whose `TabId` is unchanged
//!   keep their geometry (split fractions, parent topology); new TabIds get appended to the
//!   primary leaf; removed TabIds are dropped.
//! - **Patch-level changes** (`v0.1.0` → `v0.1.1`) are considered transparent — no migration runs.
//!
//! The actual diff/merge is in [`super::layout_service`]; this module only owns the *parsing*
//! and *comparison* primitives.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Parsed layout name: `<base>_v<major>.<minor>.<patch>`.
///
/// Round-trip stable: `Display` re-emits the canonical form.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct LayoutName {
    /// Base identifier without the `_v` suffix (e.g. `rge_main`).
    pub base: String,
    /// Semver triple from the suffix.
    pub version: LayoutVersion,
}

/// Three-component semver tuple.
///
/// Not a full semver impl — we only need ordering on `(major, minor, patch)` and equality.
/// No prerelease / build metadata is permitted in a layout name.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
pub struct LayoutVersion {
    /// Major component — bumped on incompatible schema changes.
    pub major: u32,
    /// Minor component — bumped when migration logic changes for the same base layout.
    pub minor: u32,
    /// Patch component — transparent revision bump (no migration runs).
    pub patch: u32,
}

/// Errors from [`LayoutName::parse`].
#[derive(Debug, Error)]
pub enum LayoutNameError {
    /// No `_v<...>` suffix was found.
    #[error("layout name `{0}` missing `_v<major>.<minor>.<patch>` suffix")]
    MissingSuffix(String),
    /// The suffix did not parse as three `u32` components separated by dots.
    #[error("layout name `{0}` has malformed version suffix `{1}`")]
    MalformedVersion(String, String),
}

impl LayoutVersion {
    /// Construct from explicit components.
    #[must_use]
    #[inline]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Returns true when the two versions differ in a way that requires a layout-migration pass.
    /// Patch-level changes are transparent.
    #[must_use]
    #[inline]
    pub const fn requires_migration(self, other: Self) -> bool {
        self.major != other.major || self.minor != other.minor
    }
}

impl fmt::Display for LayoutVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl LayoutName {
    /// Construct from explicit parts.
    #[must_use]
    #[inline]
    pub fn new(base: impl Into<String>, version: LayoutVersion) -> Self {
        Self {
            base: base.into(),
            version,
        }
    }

    /// Parse `<base>_v<major>.<minor>.<patch>`.
    ///
    /// Returns [`LayoutNameError::MissingSuffix`] if the `_v` marker is absent and
    /// [`LayoutNameError::MalformedVersion`] if the trailing triple does not parse.
    pub fn parse(s: &str) -> Result<Self, LayoutNameError> {
        let Some(idx) = s.rfind("_v") else {
            return Err(LayoutNameError::MissingSuffix(s.to_owned()));
        };
        let base = &s[..idx];
        let rest = &s[idx + 2..];
        let mut parts = rest.split('.');
        let parse_u32 = |p: Option<&str>| p.and_then(|x| x.parse::<u32>().ok());
        let major = parse_u32(parts.next());
        let minor = parse_u32(parts.next());
        let patch = parse_u32(parts.next());
        if parts.next().is_some() {
            return Err(LayoutNameError::MalformedVersion(
                s.to_owned(),
                rest.to_owned(),
            ));
        }
        match (major, minor, patch) {
            (Some(major), Some(minor), Some(patch)) => {
                Ok(Self::new(base, LayoutVersion::new(major, minor, patch)))
            }
            _ => Err(LayoutNameError::MalformedVersion(
                s.to_owned(),
                rest.to_owned(),
            )),
        }
    }

    /// True if the two names share a base; only such pairs are migration-eligible.
    #[must_use]
    #[inline]
    pub fn is_same_base(&self, other: &Self) -> bool {
        self.base == other.base
    }

    /// True if migrating `other` → `self` requires geometry merge (vs. straight replace / accept).
    #[must_use]
    #[inline]
    pub fn requires_migration_from(&self, other: &Self) -> bool {
        self.is_same_base(other) && self.version.requires_migration(other.version)
    }
}

impl fmt::Display for LayoutName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_v{}", self.base, self.version)
    }
}

impl std::str::FromStr for LayoutName {
    type Err = LayoutNameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_canonical() {
        let name: LayoutName = "rge_main_v0.1.0".parse().unwrap();
        assert_eq!(name.base, "rge_main");
        assert_eq!(name.version, LayoutVersion::new(0, 1, 0));
        assert_eq!(name.to_string(), "rge_main_v0.1.0");
    }

    #[test]
    fn rejects_missing_suffix() {
        assert!(matches!(
            LayoutName::parse("rge_main"),
            Err(LayoutNameError::MissingSuffix(_))
        ));
    }

    #[test]
    fn rejects_two_part_version() {
        assert!(matches!(
            LayoutName::parse("rge_main_v0.1"),
            Err(LayoutNameError::MalformedVersion(_, _))
        ));
    }

    #[test]
    fn rejects_four_part_version() {
        assert!(matches!(
            LayoutName::parse("rge_main_v0.1.0.0"),
            Err(LayoutNameError::MalformedVersion(_, _))
        ));
    }

    #[test]
    fn rejects_non_numeric_version() {
        assert!(matches!(
            LayoutName::parse("rge_main_v0.beta.0"),
            Err(LayoutNameError::MalformedVersion(_, _))
        ));
    }

    #[test]
    fn patch_changes_are_transparent() {
        let a = LayoutName::parse("rge_main_v0.1.0").unwrap();
        let b = LayoutName::parse("rge_main_v0.1.5").unwrap();
        assert!(!a.requires_migration_from(&b));
    }

    #[test]
    fn minor_change_requires_migration() {
        let a = LayoutName::parse("rge_main_v0.2.0").unwrap();
        let b = LayoutName::parse("rge_main_v0.1.0").unwrap();
        assert!(a.requires_migration_from(&b));
    }

    #[test]
    fn cross_base_does_not_migrate() {
        let a = LayoutName::parse("rge_main_v0.2.0").unwrap();
        let b = LayoutName::parse("animation_v0.1.0").unwrap();
        assert!(!a.requires_migration_from(&b));
    }

    #[test]
    fn version_ordering_is_lexicographic() {
        assert!(LayoutVersion::new(0, 1, 0) < LayoutVersion::new(0, 2, 0));
        assert!(LayoutVersion::new(0, 1, 9) < LayoutVersion::new(0, 2, 0));
        assert!(LayoutVersion::new(1, 0, 0) > LayoutVersion::new(0, 99, 99));
    }
}
