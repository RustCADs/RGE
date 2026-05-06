//! Extension points — typed identifiers for slots that menus / toolbars
//! anchor to.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! In the rustforge prior art, the host built `Vec<MenuDefinition>`
//! directly each frame. To let plugins extend the same surface without
//! forking the host, we route registrations through named slots:
//! `editor.main_menu.file`, `editor.toolbar.play_mode`, etc.
//!
//! The slot id itself is just a string newtype; the registry tracks
//! which ids have been declared so that `register_entry` against an
//! unknown slot fails fast instead of silently dropping entries.

use core::fmt;

/// A named slot that menu / toolbar entries anchor to.
///
/// The id is a dotted string (`"editor.main_menu.file"`,
/// `"editor.context.viewport"`, `"editor.toolbar.play_mode"`) — the
/// segmenting is purely conventional; the registry does not parse it.
/// Hosts and plugins pick ids cooperatively; conflict resolution is
/// the user's responsibility (see `MenuRegistry::declare_extension_point`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtensionPoint(String);

impl ExtensionPoint {
    /// Build an extension point from a stable id. Empty ids panic in
    /// debug and produce an unspecified-but-non-empty placeholder in
    /// release; an empty id is always a programming error.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        debug_assert!(!id.is_empty(), "ExtensionPoint id must be non-empty");
        Self(id)
    }

    /// Borrow the underlying id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and yield the inner [`String`].
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ExtensionPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ExtensionPoint {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for ExtensionPoint {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_str() {
        let p = ExtensionPoint::new("editor.main_menu.file");
        assert_eq!(p.as_str(), "editor.main_menu.file");
        assert_eq!(p.to_string(), "editor.main_menu.file");
    }

    #[test]
    fn equality_is_by_id() {
        assert_eq!(
            ExtensionPoint::from("editor.toolbar.play_mode"),
            ExtensionPoint::from(String::from("editor.toolbar.play_mode")),
        );
        assert_ne!(ExtensionPoint::from("a"), ExtensionPoint::from("b"),);
    }
}
