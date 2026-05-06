//! Ordering hints — how an entry positions itself inside an extension
//! point.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! UE5 `UToolMenus` uses `EUMenuExtensionHook::Before/After` against a
//! named extension hook; the v0.8 plan §6.3 maps that one-to-one onto
//! [`OrderHint::Before`] / [`OrderHint::After`]. We add [`OrderHint::AtStart`]
//! / [`OrderHint::AtEnd`] / [`OrderHint::InSection`] so plugins can
//! pin without picking a sibling.
//!
//! Resolution lives in [`crate::menus::registry`] — this file is data-only.

use crate::menus::EntryId;

/// How an entry positions itself relative to siblings during resolve.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OrderHint {
    /// Place this entry immediately before the entry with the given id.
    /// If the target id is missing the entry falls back to [`AtEnd`](Self::AtEnd)
    /// in its section — see registry resolve rules.
    Before(EntryId),
    /// Place this entry immediately after the entry with the given id.
    /// Same fallback as [`Self::Before`].
    After(EntryId),
    /// Place this entry at the start of its section (or, if the section
    /// is the default, the start of the extension point).
    AtStart,
    /// Place this entry at the end of its section (or, if the section
    /// is the default, the end of the extension point). The default
    /// hint when not otherwise specified.
    AtEnd,
    /// Move this entry into the named section, then resolve as
    /// [`AtEnd`](Self::AtEnd) inside it. Equivalent to
    /// [`MenuEntry::with_section`](crate::menus::MenuEntry::with_section)
    /// + [`AtEnd`](Self::AtEnd); kept as a hint variant so plugins can
    /// re-section without rebuilding the entry.
    InSection(String),
}

impl Default for OrderHint {
    fn default() -> Self {
        Self::AtEnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_at_end() {
        assert_eq!(OrderHint::default(), OrderHint::AtEnd);
    }

    #[test]
    fn before_after_carry_id() {
        let b = OrderHint::Before(EntryId::new("file.exit"));
        let a = OrderHint::After(EntryId::new("file.open"));
        assert_ne!(b, a);
        if let OrderHint::Before(id) = &b {
            assert_eq!(id.as_str(), "file.exit");
        } else {
            panic!("Before variant expected");
        }
    }

    #[test]
    fn in_section_carries_name() {
        let s = OrderHint::InSection("primary".into());
        if let OrderHint::InSection(name) = s {
            assert_eq!(name, "primary");
        } else {
            panic!("InSection variant expected");
        }
    }
}
