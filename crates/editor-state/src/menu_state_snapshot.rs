//! Read-only Play-menu enablement observation snapshot — which Play-mode menu
//! items (Play / Pause / Stop / Step) are valid in the current `PlayState`, so
//! the host's menu bar can grey out the ones whose transition would be a no-op.
//!
//! # Sibling to [`crate::SaveStatusSnapshot`], not a 6th coordination category
//!
//! Like [`crate::SaveStatusSnapshot`] / [`crate::InspectorSnapshot`], this is a
//! read-only **observation aggregator** — a plain, owned view assembled by
//! `editor-shell` from already-public state (`PlayState::can_play` /
//! `can_pause` / `can_stop` / `can_step`) for consumption by the host's Play
//! menu. It owns no state and stores no IDs; `editor-shell` produces a fresh
//! instance per frame via `EditorShell::menu_state_snapshot()`. The §0.6 freeze
//! gates the *coordination-category* count at 5 (Selection, Hover, ActiveTool,
//! ModalState, DragDrop); this is not one of them, so the
//! `editor-state-ownership` lint Part A (which forbids only those five names
//! outside editor-state) does not fire.
//!
//! Living here (rather than in `editor-shell` directly) keeps the
//! editor-shell ↔ editor-egui-host hosting direction open: both crates already
//! depend on `editor-state`, so a shared observation type avoids forcing either
//! crate to depend on the other — the same rationale that places
//! [`crate::SaveStatusSnapshot`] here.
//!
//! # Architectural invariants (shared with `SaveStatusSnapshot`)
//!
//! - **Single source per field.** Each `play_can_*` flag is derived once from
//!   the canonical `PlayState` query of the same stem (whose authority is the
//!   `PlayState` transition method it mirrors, pinned by a `PlayState` test).
//!   No staleness, no caching.
//! - **No side effects on construction.** Building the snapshot is a pure read.

/// Plain-data view of which Play-mode menu items are enabled in the current
/// `PlayState`. Built by `EditorShell::menu_state_snapshot()` as a pure read
/// with no side effects; consumed by the host's `render` to `add_enabled` each
/// Play menu item.
///
/// # Field stability
///
/// Each flag mirrors the `PlayState` query of the same stem
/// (`can_play` / `can_pause` / `can_stop` / `can_step`) — the enablement
/// authority, pinned to the transition methods by a `PlayState` test. `true` =
/// the item is clickable; `false` = greyed out.
///
/// # Default vs `all_enabled`
///
/// `Default` is all-`false` (the conventional zero, matching the derived
/// sibling snapshots). It is **not** the right pre-first-publish fallback: the
/// host shows [`Self::all_enabled`] until editor-shell publishes a real
/// snapshot, so no item is spuriously greyed on the first frame.
///
/// # Trait bounds
///
/// `Copy + Clone + Debug + PartialEq + Default` — a flat bag of `bool` leaves
/// (cf. [`crate::InspectorSnapshot`], also `Copy`). `Send + Sync` are
/// auto-derived so an `Arc<MenuStateSnapshot>` can cross the handoff.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MenuStateSnapshot {
    /// `PlayState::can_play` — Play / Resume is valid (Editing or Paused).
    pub play_can_start: bool,
    /// `PlayState::can_pause` — Pause is valid (PIE active).
    pub play_can_pause: bool,
    /// `PlayState::can_stop` — Stop is valid (PIE active).
    pub play_can_stop: bool,
    /// `PlayState::can_step` — Step is valid (Paused only).
    pub play_can_step: bool,
}

impl MenuStateSnapshot {
    /// Every Play item enabled. The host's pre-first-publish fallback — until
    /// editor-shell publishes the real per-`PlayState` snapshot, nothing is
    /// greyed out (a disabled item would be a worse first-frame artifact than a
    /// momentarily-enabled one whose click benign-swallows).
    #[must_use]
    pub const fn all_enabled() -> Self {
        Self {
            play_can_start: true,
            play_can_pause: true,
            play_can_stop: true,
            play_can_step: true,
        }
    }
}
