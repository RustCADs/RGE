// adapted from rustforge::runtime-smartobject::definition on 2026-05-05 — kept the
//                                                      String-newtype shape; dropped
//                                                      the `tags` / blake3 id fields
//                                                      since W01 components are
//                                                      state-only and tags belong in
//                                                      a future components-tags crate.
//
//! [`Name`] — user-visible label component.
//!
//! Shows up in the editor outliner, scene-tree pickers, and diagnostic spans.
//! Not unique: an editor warning lints duplicates within a parent, but the
//! type itself does not enforce uniqueness (PLAN.md §1.5.1 lists `Name` as
//! optional-but-recommended on every entity).

use serde::{Deserialize, Serialize};

/// User-visible label for an entity.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Name(pub String);

impl Name {
    /// Construct a `Name` from anything that can become a `String`.
    #[inline]
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string slice.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// `String::default()` already gives an empty string; relying on the derive
// avoids a dedicated `impl Default` block flagged by clippy's
// `derivable_impls` lint.

impl From<&str> for Name {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for Name {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let n = Name::new("MainCamera");
        let s = ron::to_string(&n).expect("serialize");
        let back: Name = ron::from_str(&s).expect("deserialize");
        assert_eq!(n, back);
    }

    #[test]
    fn round_trip_unicode_ron() {
        let n = Name::new("Tooth.Quadrant.UpperRight.Molar");
        let s = ron::to_string(&n).expect("serialize");
        let back: Name = ron::from_str(&s).expect("deserialize");
        assert_eq!(n, back);
    }

    #[test]
    fn from_str_constructs_owned() {
        let n: Name = "Player".into();
        assert_eq!(n.as_str(), "Player");
    }
}
