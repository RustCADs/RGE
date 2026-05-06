//! Stable tab identifiers.
//!
//! UE Slate parallel: `FTabId` (a `FName` plus optional instance suffix). Per PLAN.md §6.6 / §6.8,
//! this is the persistent handle that survives layout serialize/restore round-trips and feeds the
//! [`SpawnerRegistry`] when materializing a tab body.
//!
//! `TabId` is a thin newtype around `String`. Identifiers are conventionally lower-snake-case
//! (e.g. `scene_panel`, `viewport_main`, `console`); the equality contract is byte-exact, no
//! normalization. Plugins (Tier-3) MUST namespace their TabIds (`<plugin>::<tab>`) per the
//! plugin-isolation conventions in PLAN.md §11.
//!
//! [`SpawnerRegistry`]: crate::dock::spawner_registry::SpawnerRegistry

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable, serializable tab identifier.
///
/// Derives `Hash`/`Eq`/`Ord` so a `TabId` works as a `HashMap`/`BTreeMap` key in the spawner
/// registry and the layout-diff version-migration code.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabId(String);

impl TabId {
    /// Construct a `TabId` from anything `Into<String>`.
    ///
    /// No validation is performed at v0.0.1; callers are expected to use lower-snake-case names.
    /// (We reserve the right to add a debug-only validator in a later wave once a v0.1 conformance
    /// bar exists — see PLAN.md §1.10.)
    #[inline]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the inner string.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the inner string.
    #[inline]
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TabId {
    #[inline]
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for TabId {
    #[inline]
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<Cow<'_, str>> for TabId {
    #[inline]
    fn from(value: Cow<'_, str>) -> Self {
        Self(value.into_owned())
    }
}

impl AsRef<str> for TabId {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_is_byte_exact() {
        assert_eq!(TabId::new("scene_panel"), TabId::from("scene_panel"));
        assert_ne!(TabId::new("scene_panel"), TabId::from("ScenePanel"));
    }

    #[test]
    fn round_trips_through_ron() {
        let original = TabId::new("viewport_main");
        let s = ron::to_string(&original).unwrap();
        let back: TabId = ron::from_str(&s).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn round_trips_through_json() {
        let original = TabId::new("console");
        let s = serde_json::to_string(&original).unwrap();
        // transparent newtype: emitted as bare string
        assert_eq!(s, "\"console\"");
        let back: TabId = serde_json::from_str(&s).unwrap();
        assert_eq!(original, back);
    }
}
