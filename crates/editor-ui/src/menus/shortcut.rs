//! Keyboard shortcuts + global accelerator table.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! The rustforge prior art appended shortcut hints to the rendered
//! label as text only (`"Save (Ctrl+S)"`). The registry treats shortcuts
//! as first-class data so that:
//!
//! 1. The accelerator table can be queried in O(1) to resolve a
//!    keystroke to the bound entry id.
//! 2. Conflict detection runs across the entire registered surface,
//!    not per-menu.
//!
//! Conflicts are reported as a list out of [`crate::menus::MenuRegistry::resolve`]
//! rather than being a hard registration error: real users frequently
//! redefine bindings, and surfacing the conflict to the host lets it
//! decide policy (warn / pick winner / refuse).

use std::collections::HashMap;

use crate::menus::EntryId;

/// Modifier-key bitset for shortcuts. Hand-rolled (no external
/// `bitflags` crate dep) — the four well-known keys cover every
/// mainstream desktop platform; XR / mobile do not bind shortcuts
/// today so the surface is intentionally narrow.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers {
    bits: u8,
}

impl Modifiers {
    /// Control key (Windows / Linux). On macOS the host pre-maps
    /// `Ctrl` → [`Self::SUPER`]; the registry stores whichever the
    /// caller registered.
    pub const CTRL: Self = Self { bits: 0b0000_0001 };
    /// Shift key.
    pub const SHIFT: Self = Self { bits: 0b0000_0010 };
    /// Alt / Option key.
    pub const ALT: Self = Self { bits: 0b0000_0100 };
    /// Super / Cmd / Win key.
    pub const SUPER: Self = Self { bits: 0b0000_1000 };

    /// Empty bitset (no modifiers held).
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Raw integer representation of the bitset.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.bits
    }

    /// Build a bitset from a raw integer. Bits outside the four
    /// declared flags are dropped silently — this normalises any
    /// stray host-side bits.
    #[must_use]
    pub const fn from_bits_truncate(bits: u8) -> Self {
        let mask = Self::CTRL.bits | Self::SHIFT.bits | Self::ALT.bits | Self::SUPER.bits;
        Self { bits: bits & mask }
    }

    /// `true` when every flag in `other` is set on `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    /// `true` when this bitset has no flags set.
    #[must_use]
    pub const fn is_no_modifiers(self) -> bool {
        self.bits == 0
    }
}

impl core::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

impl core::ops::BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

impl core::ops::BitAnd for Modifiers {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self {
            bits: self.bits & rhs.bits,
        }
    }
}

/// A single non-modifier key.
///
/// Open enum: the well-known set covers `[A-Z] [0-9] F1..F24 + arrows + a
/// few editing keys`; anything else falls through to [`Key::Other`]
/// with the raw string the host's input layer reports. The registry
/// hashes / equates [`Key`] structurally so the accelerator table is
/// O(1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// `A` through `Z`. Stored uppercase regardless of the active
    /// shift modifier.
    Char(char),
    /// `0` through `9`.
    Digit(u8),
    /// `F1` through `F24`.
    Function(u8),
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Backspace.
    Backspace,
    /// Return / Enter.
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Space.
    Space,
    /// Anything else — opaque to the registry, hashable via the inner
    /// string.
    Other(String),
}

/// A complete shortcut: zero-or-more modifiers plus a single key.
///
/// Two shortcuts that compare equal occupy the same slot in the
/// accelerator table; that equality drives conflict detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// Bitset of modifier keys.
    pub modifiers: Modifiers,
    /// The non-modifier key.
    pub key: Key,
}

impl Shortcut {
    /// Construct from a modifier set and key.
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Plain key with no modifiers — handy for single-stroke bindings
    /// like `R` or `Escape`.
    #[must_use]
    pub fn plain(key: Key) -> Self {
        Self {
            modifiers: Modifiers::empty(),
            key,
        }
    }

    /// Render in the rustforge convention (`"Ctrl+Shift+S"`). Order is
    /// fixed (`Ctrl, Shift, Alt, Super`) so two equal shortcuts always
    /// produce identical strings.
    #[must_use]
    pub fn display(&self) -> String {
        let mut buf = String::new();
        if self.modifiers.contains(Modifiers::CTRL) {
            buf.push_str("Ctrl+");
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            buf.push_str("Shift+");
        }
        if self.modifiers.contains(Modifiers::ALT) {
            buf.push_str("Alt+");
        }
        if self.modifiers.contains(Modifiers::SUPER) {
            buf.push_str("Super+");
        }
        match &self.key {
            Key::Char(c) => buf.push(*c),
            Key::Digit(d) => buf.push_str(&d.to_string()),
            Key::Function(n) => {
                buf.push('F');
                buf.push_str(&n.to_string());
            }
            Key::Up => buf.push_str("Up"),
            Key::Down => buf.push_str("Down"),
            Key::Left => buf.push_str("Left"),
            Key::Right => buf.push_str("Right"),
            Key::Home => buf.push_str("Home"),
            Key::End => buf.push_str("End"),
            Key::PageUp => buf.push_str("PageUp"),
            Key::PageDown => buf.push_str("PageDown"),
            Key::Insert => buf.push_str("Insert"),
            Key::Delete => buf.push_str("Delete"),
            Key::Backspace => buf.push_str("Backspace"),
            Key::Enter => buf.push_str("Enter"),
            Key::Tab => buf.push_str("Tab"),
            Key::Escape => buf.push_str("Escape"),
            Key::Space => buf.push_str("Space"),
            Key::Other(s) => buf.push_str(s),
        }
        buf
    }
}

/// A diagnostic emitted when two or more entries claim the same
/// keystroke. Reported by [`AcceleratorTable::detect_conflicts`] and
/// surfaced through [`crate::menus::MenuRegistry::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutConflict {
    /// The shortcut all entries claim.
    pub shortcut: Shortcut,
    /// Every entry id that registered this shortcut, in registration
    /// order.
    pub entries: Vec<EntryId>,
}

/// O(1) lookup table from shortcut to bound entry id(s).
///
/// Non-conflict entries are stored as `Vec<EntryId>` of length 1 so
/// the data shape is uniform; the helper [`AcceleratorTable::resolve`]
/// returns the first id (the "winning" registration) and
/// [`AcceleratorTable::detect_conflicts`] reports collisions. The host
/// decides whether a conflict is fatal.
#[derive(Debug, Default, Clone)]
pub struct AcceleratorTable {
    /// Registered bindings. `HashMap` because [`Shortcut`] is `Hash + Eq`.
    bindings: HashMap<Shortcut, Vec<EntryId>>,
}

impl AcceleratorTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `shortcut → entry`. Multiple registrations for the
    /// same shortcut accumulate (see [`Self::detect_conflicts`]).
    /// Insertion is amortised O(1).
    pub fn register(&mut self, shortcut: Shortcut, entry: EntryId) {
        self.bindings.entry(shortcut).or_default().push(entry);
    }

    /// Look up the winning binding for a keystroke. Returns the first
    /// registered id; returns `None` if nothing is bound.
    #[must_use]
    pub fn resolve(&self, shortcut: &Shortcut) -> Option<&EntryId> {
        self.bindings.get(shortcut).and_then(|v| v.first())
    }

    /// Number of distinct shortcuts registered (multi-bound shortcuts
    /// count once each).
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` when no shortcut is bound.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Collect every conflict: shortcuts with two or more bound entries.
    /// The returned list is sorted by display string for stable output
    /// in test snapshots; entry order inside each conflict is
    /// registration order.
    #[must_use]
    pub fn detect_conflicts(&self) -> Vec<ShortcutConflict> {
        let mut out: Vec<ShortcutConflict> = self
            .bindings
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(s, v)| ShortcutConflict {
                shortcut: s.clone(),
                entries: v.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.shortcut.display().cmp(&b.shortcut.display()));
        out
    }

    /// Drop every registered binding.
    pub fn clear(&mut self) {
        self.bindings.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_compose_with_bitor() {
        let m = Modifiers::CTRL | Modifiers::SHIFT;
        assert!(m.contains(Modifiers::CTRL));
        assert!(m.contains(Modifiers::SHIFT));
        assert!(!m.contains(Modifiers::ALT));
        assert!(!m.contains(Modifiers::SUPER));
    }

    #[test]
    fn modifiers_from_bits_truncate_drops_unknown() {
        let m = Modifiers::from_bits_truncate(0xFF);
        // Only the four declared bits survive.
        assert_eq!(m.bits(), 0b0000_1111);
    }

    #[test]
    fn shortcut_display_in_canonical_order() {
        let s = Shortcut::new(Modifiers::SHIFT | Modifiers::CTRL, Key::Char('S'));
        // Order is fixed Ctrl,Shift,Alt,Super even though the bitor was
        // SHIFT|CTRL — that's the canonicalisation we depend on for
        // identity.
        assert_eq!(s.display(), "Ctrl+Shift+S");
    }

    #[test]
    fn shortcut_display_no_modifiers() {
        assert_eq!(Shortcut::plain(Key::Char('R')).display(), "R");
    }

    #[test]
    fn function_and_arrow_keys_round_trip() {
        assert_eq!(Shortcut::plain(Key::Function(5)).display(), "F5");
        assert_eq!(Shortcut::plain(Key::Up).display(), "Up");
        assert_eq!(Shortcut::plain(Key::PageDown).display(), "PageDown");
    }

    #[test]
    fn accelerator_table_resolves_o1() {
        let mut t = AcceleratorTable::new();
        let s_save = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        let s_open = Shortcut::new(Modifiers::CTRL, Key::Char('O'));
        t.register(s_save.clone(), EntryId::new("file.save"));
        t.register(s_open.clone(), EntryId::new("file.open"));
        assert_eq!(t.resolve(&s_save).map(|e| e.as_str()), Some("file.save"));
        assert_eq!(t.resolve(&s_open).map(|e| e.as_str()), Some("file.open"));
        assert_eq!(t.len(), 2);
        assert!(t.detect_conflicts().is_empty());
    }

    #[test]
    fn accelerator_table_reports_conflicts() {
        let mut t = AcceleratorTable::new();
        let s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        t.register(s.clone(), EntryId::new("file.save"));
        t.register(s.clone(), EntryId::new("plugin.foo.save_alt"));
        let conflicts = t.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].shortcut, s);
        assert_eq!(conflicts[0].entries.len(), 2);
        assert_eq!(conflicts[0].entries[0].as_str(), "file.save");
        assert_eq!(conflicts[0].entries[1].as_str(), "plugin.foo.save_alt");
    }
}
