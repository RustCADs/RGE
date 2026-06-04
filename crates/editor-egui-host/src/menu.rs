//! `editor-egui-host::menu` — host projection of the editor's main menus.
//!
//! Resolves the canonical editor-menu definition ([`default_editor_menu`]) once
//! and projects each of the four surfaces (File / Edit / Play / View) to the
//! `(label, accelerator display, `[`Command`]`)` triples the host's menu bar
//! paints. Also owns the two render-time helpers the menu bar calls:
//! [`play_item_enabled`] (per-item PIE enablement routing) and [`menu_item`]
//! (one button + optional `shortcut_text`).
//!
//! The menu DEFINITION — extension points, entries, and the File/Edit
//! accelerators — moved down to `editor-ui` (W08 canonical menu source) so
//! `editor-shell` can resolve the same bindings for accelerator EXECUTION without
//! a reverse crate edge; this module keeps only the host's display projection.
//! The `menu` submodule itself was split out of `lib.rs`
//! (EGUIHOST-MENU-EXTRACTION) to keep the host crate root under the §1.3 Rule-3
//! 1000-line cap; MENU-SHORTCUT-DISPLAY (#304) shipped the File/Edit accelerator
//! data the projection carries.

use rge_editor_ui::menus::{
    default_editor_menu, edit_menu_point, file_menu_point, play_menu_point, view_menu_point,
    Command, ExtensionPoint, PredicateContext, Shortcut,
};

/// Resolve the canonical editor menu ([`default_editor_menu`]) once against an
/// empty [`PredicateContext`] and project each of the four points (File / Edit /
/// Play / View) to the `(label, accelerator display, `[`Command`]`)` triples the
/// menu bar paints. The accelerator element is `Some(`[`Shortcut::display`]`)` for
/// the File/Edit entries — their real keyboard accelerators, rendered as egui
/// `shortcut_text` — and `None` for every Play/View entry (display-only; the
/// keystroke itself is routed by editor-shell). Returns `(file, edit, play, view)`.
///
/// All four menus are static (no predicates / dynamic visibility), so resolving
/// once at construction is sufficient and the host caches the results; per-frame
/// re-resolve is deferred to a future dispatch. The menus' content + order are
/// owned by [`default_editor_menu`] in `editor-ui`; this projection is only the
/// host's render-shape adapter (the `menu_tests` pin every label + display string).
pub(crate) fn build_main_menu_entries() -> (
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
) {
    let resolved = default_editor_menu().resolve(&PredicateContext::default());
    // Project each resolved entry to `(label, optional accelerator display,
    // command)`. The middle element is sourced straight from the resolved
    // `MenuEntry.shortcut` via `Shortcut::display` — `Some("Ctrl+S")` for the
    // File/Edit entries, `None` for every Play/View entry.
    let project = |point: &ExtensionPoint| -> Vec<(String, Option<String>, Command)> {
        resolved
            .entries_for(point)
            .iter()
            .map(|r| {
                (
                    r.entry.label.clone(),
                    r.entry.shortcut.as_ref().map(Shortcut::display),
                    r.entry.command.clone(),
                )
            })
            .collect()
    };
    (
        project(&file_menu_point()),
        project(&edit_menu_point()),
        project(&play_menu_point()),
        project(&view_menu_point()),
    )
}

/// Map a Play-menu [`Command`] to its enabled flag from the per-frame
/// [`rge_editor_state::MenuStateSnapshot`] (published by editor-shell from the
/// canonical `PlayState`). The host re-encodes NO `PlayState` validity — it only
/// routes the already-computed booleans. Non-Play commands never appear in the
/// Play menu; they default to enabled (the editor-shell router benign-ignores any
/// stray command anyway).
pub(crate) fn play_item_enabled(
    cmd: &Command,
    menu_state: &rge_editor_state::MenuStateSnapshot,
) -> bool {
    match cmd {
        Command::PlayStart => menu_state.play_can_start,
        Command::PlayPause => menu_state.play_can_pause,
        Command::PlayStop => menu_state.play_can_stop,
        Command::PlayStep => menu_state.play_can_step,
        _ => true,
    }
}

/// Add one main-menu item: its `label`, plus — when the entry carries an
/// accelerator — that hint rendered as egui's right-aligned `shortcut_text`.
/// `enabled` greys the item out (`true` for every File / Edit / View item; the
/// Play menu passes its per-item PIE enablement from [`play_item_enabled`]).
/// Returns the click [`egui::Response`]. Display-only: the accelerator is a
/// passive hint (the keystroke is routed by editor-shell); activation is the
/// click.
pub(crate) fn menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    shortcut: Option<&str>,
) -> egui::Response {
    let mut button = egui::Button::new(label);
    if let Some(text) = shortcut {
        button = button.shortcut_text(text);
    }
    ui.add_enabled(enabled, button)
}
