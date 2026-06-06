//! `editor-egui-host::menu` — host projection of the editor's main menus.
//!
//! Resolves the canonical editor-menu definition EACH FRAME against the live
//! [`PredicateContext`] the editor-shell publishes, and projects each of the four
//! surfaces (File / Edit / Play / View) to the `(label, shortcut display,
//! `[`Command`]`, enabled)` tuples the host's menu bar paints — `enabled` greys
//! items whose enablement predicate is false for the current state. Also owns
//! [`menu_item`] (one button + optional `shortcut_text`).
//!
//! The menu DEFINITION — extension points, entries, and the File/Edit
//! accelerators — moved down to `editor-ui` (W08 canonical menu source) so
//! `editor-shell` can resolve the same bindings for accelerator EXECUTION without
//! a reverse crate edge; this module keeps only the host's display projection.
//! The `menu` submodule itself was split out of `lib.rs`
//! (EGUIHOST-MENU-EXTRACTION) to keep the host crate root under the §1.3 Rule-3
//! 1000-line cap; MENU-SHORTCUT-DISPLAY (#304) shipped the File/Edit accelerator
//! data the projection carries. Play's plain-key playback bindings are projected
//! only through display hints, not as executable menu accelerators.

use rge_editor_ui::menus::{
    edit_menu_point, file_menu_point, play_menu_point, view_menu_point, Command, ExtensionPoint,
    MenuRegistry, PredicateContext, Shortcut,
};

/// Resolve `registry` against the live `ctx` and project each of the four points
/// (File / Edit / Play / View) to the `(label, shortcut display, command,
/// enabled)` tuples the menu bar paints. The shortcut element is
/// `Some(`[`Shortcut::display`]`)` for real executable shortcuts (File/Edit) and
/// also for passive display-only hints such as Play's Space/Escape keys. Passive
/// hints do not enter the accelerator table; the keystroke itself is routed by
/// editor-shell's playback path. `enabled` is the resolved entry's
/// [`rge_editor_ui::menus::ResolvedEntry::enabled`] for `ctx` (greys the item
/// when its enablement predicate is false). Returns `(file, edit, play, view)`.
///
/// Called PER FRAME with the live [`PredicateContext`] the editor-shell publishes,
/// so menu enablement tracks the live `PlayState` / editing state. The host caches
/// the `registry` (built once from `default_editor_menu` in `editor-ui`) and
/// re-resolves here each frame; the menus' content + order are owned by
/// `default_editor_menu` (the `menu_tests` pin every label + display string).
pub(crate) fn project_main_menu(
    registry: &MenuRegistry,
    ctx: &PredicateContext,
) -> (
    Vec<(String, Option<String>, Command, bool)>,
    Vec<(String, Option<String>, Command, bool)>,
    Vec<(String, Option<String>, Command, bool)>,
    Vec<(String, Option<String>, Command, bool)>,
) {
    let resolved = registry.resolve(ctx);
    // Project each resolved entry to `(label, optional shortcut display,
    // command, enabled)`. The accelerator is sourced from the resolved
    // `MenuEntry.shortcut` via `Shortcut::display`, falling back to the passive
    // `shortcut_hint`; `enabled` is the resolved `ResolvedEntry.enabled` (the
    // host greys disabled items, which stay present).
    let project = |point: &ExtensionPoint| -> Vec<(String, Option<String>, Command, bool)> {
        resolved
            .entries_for(point)
            .iter()
            .map(|r| {
                (
                    r.entry.label.clone(),
                    r.entry
                        .shortcut
                        .as_ref()
                        .or(r.entry.shortcut_hint.as_ref())
                        .map(Shortcut::display),
                    r.entry.command.clone(),
                    r.enabled,
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

/// Add one main-menu item: its `label`, plus — when the entry carries an
/// accelerator — that hint rendered as egui's right-aligned `shortcut_text`.
/// `enabled` greys the item out — every caller passes the item's resolved
/// [`rge_editor_ui::menus::ResolvedEntry::enabled`] (from [`project_main_menu`]).
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
