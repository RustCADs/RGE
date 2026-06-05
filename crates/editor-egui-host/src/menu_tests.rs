//! Unit tests for the host's main-menu wiring: that
//! [`crate::menu::project_main_menu`] resolves each extension point
//! (File / Edit / Play / View) to the expected
//! `(label, accelerator display, `[`Command`]`)` list in order, that File/Edit
//! items carry their real accelerator hint while Play/View carry none, and that
//! each resolved [`Command`] round-trips through
//! the [`super::MenuCommandHandoff`] FIFO.
//!
//! Originally extracted verbatim from the inline `#[cfg(test)] mod menu_tests`
//! in `lib.rs` (EGUIHOST-TEST-EXTRACTION) — at the time a behaviour-identical
//! move that dropped `lib.rs` back under the §1.3 Rule 3 1000-line split cap.
//! MENU-SHORTCUT-DISPLAY (#304) later widened these tests to pin the File/Edit
//! accelerator display + the Play/View deferral; EGUIHOST-MENU-EXTRACTION then
//! moved the menu-construction code these tests target into the `menu` submodule
//! (hence the `crate::menu::` paths below), keeping `lib.rs` under the cap.

use rge_editor_ui::menus::{default_editor_menu, Command, PredicateContext};

use super::MenuCommandHandoff;
use crate::menu::project_main_menu;

/// Project the canonical menu's four points to `(label, accel, command)` triples,
/// dropping the resolved `enabled` flag — these tests pin labels / commands /
/// accelerator display / order, which are context-independent. Resolved against an
/// empty context; enablement is covered by `enablement_tracks_context`.
#[allow(clippy::type_complexity)]
fn menu_entries() -> (
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
) {
    let strip = |v: Vec<(String, Option<String>, Command, bool)>| {
        v.into_iter()
            .map(|(l, a, c, _)| (l, a, c))
            .collect::<Vec<_>>()
    };
    let (f, e, p, vw) = project_main_menu(&default_editor_menu(), &PredicateContext::default());
    (strip(f), strip(e), strip(p), strip(vw))
}

#[test]
fn file_menu_registry_resolves_the_authoring_loop_commands() {
    let (file, _edit, _play, _view) = menu_entries();
    assert_eq!(
        file,
        vec![
            (
                "Open…".to_owned(),
                Some("Ctrl+O".to_owned()),
                Command::OpenFile,
            ),
            ("Save".to_owned(), Some("Ctrl+S".to_owned()), Command::Save),
            (
                "Save As New Project…".to_owned(),
                Some("Ctrl+Shift+S".to_owned()),
                Command::SaveAs,
            ),
        ],
        "the MenuRegistry resolves the File menu to exactly Open / Save / \
         Save-As-new-project, in order — each with its real accelerator display"
    );
}

#[test]
fn edit_menu_registry_resolves_undo_redo_in_order() {
    let (_file, edit, _play, _view) = menu_entries();
    assert_eq!(
        edit,
        vec![
            ("Undo".to_owned(), Some("Ctrl+Z".to_owned()), Command::Undo),
            ("Redo".to_owned(), Some("Ctrl+Y".to_owned()), Command::Redo),
        ],
        "the MenuRegistry resolves the Edit menu to exactly Undo / Redo, in order \
         — each with its real accelerator display"
    );
}

#[test]
fn file_menu_entries_round_trip_through_the_handoff_in_order() {
    let (file, _edit, _play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in file {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::OpenFile, Command::Save, Command::SaveAs],
        "each resolved File item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn edit_menu_entries_round_trip_through_the_handoff_in_order() {
    let (_file, edit, _play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in edit {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::Undo, Command::Redo],
        "each resolved Edit item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn play_menu_registry_resolves_play_pause_stop_step_in_order() {
    let (_file, _edit, play, _view) = menu_entries();
    assert_eq!(
        play,
        vec![
            ("Play".to_owned(), None, Command::PlayStart),
            ("Pause".to_owned(), None, Command::PlayPause),
            ("Stop".to_owned(), None, Command::PlayStop),
            ("Step".to_owned(), None, Command::PlayStep),
        ],
        "the MenuRegistry resolves the Play menu to exactly Play / Pause / Stop / \
         Step, in order — no accelerator display (Play's real keys are the plain \
         Space/Escape PIE binds, not menu accelerators)"
    );
}

#[test]
fn play_menu_entries_round_trip_through_the_handoff_in_order() {
    let (_file, _edit, play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in play {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![
            Command::PlayStart,
            Command::PlayPause,
            Command::PlayStop,
            Command::PlayStep,
        ],
        "each resolved Play item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn view_menu_registry_resolves_reset_camera() {
    let (_file, _edit, _play, view) = menu_entries();
    assert_eq!(
        view,
        vec![("Reset Camera".to_owned(), None, Command::ResetCamera)],
        "the MenuRegistry resolves the View menu to exactly Reset Camera \
         (no accelerator display)"
    );
}

#[test]
fn view_menu_entries_round_trip_through_the_handoff() {
    let (_file, _edit, _play, view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in view {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::ResetCamera],
        "each resolved View item enqueues its Command; they drain FIFO"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn enablement_tracks_context() {
    // Greying flows from the resolved `ResolvedEntry.enabled` (the 4th projected
    // element) — the canonical registry path that replaced the bespoke
    // MenuStateSnapshot / play_item_enabled channel. Each context yields a
    // distinct enablement pattern. (PredicateContext is #[non_exhaustive], so it
    // is built via default() + field assignment, not a struct literal.)
    let enabled_of = |entries: &[(String, Option<String>, Command, bool)], cmd: &Command| -> bool {
        entries
            .iter()
            .find(|(_, _, c, _)| c == cmd)
            .map(|(_, _, _, e)| *e)
            .expect("command present (enablement never filters)")
    };

    // Editing: File items + Play (start) enabled; pause/stop/step disabled.
    let mut editing = PredicateContext::default();
    editing.is_editing = true;
    editing.can_play = true;
    let (file, _edit, play, _view) = project_main_menu(&default_editor_menu(), &editing);
    assert!(enabled_of(&file, &Command::Save));
    assert!(enabled_of(&file, &Command::OpenFile));
    assert!(enabled_of(&play, &Command::PlayStart));
    assert!(!enabled_of(&play, &Command::PlayPause));
    assert!(!enabled_of(&play, &Command::PlayStep));

    // Playing: File items DISABLED (greyed, still present); pause/stop enabled.
    let mut playing = PredicateContext::default();
    playing.can_pause = true;
    playing.can_stop = true;
    let (file, _edit, play, _view) = project_main_menu(&default_editor_menu(), &playing);
    assert!(
        !enabled_of(&file, &Command::Save),
        "Save greyed while playing"
    );
    assert_eq!(
        file.len(),
        3,
        "disabled File items stay present (3), not hidden"
    );
    assert!(enabled_of(&play, &Command::PlayPause));
    assert!(enabled_of(&play, &Command::PlayStop));
    assert!(!enabled_of(&play, &Command::PlayStart));
}

#[test]
fn file_and_edit_items_carry_their_real_accelerator_display_play_view_deferred() {
    // The accelerator-display column (middle tuple element) is sourced from each
    // resolved `MenuEntry.shortcut` via `Shortcut::display`. File + Edit carry the
    // canonical File/Edit accelerators (Ctrl+O/S/Shift+S, Ctrl+Z/Y) — the SAME
    // definition editor-shell's live keystroke routing resolves through (the W08
    // thread made the menu the single source of truth); Play + View carry NO
    // accelerator display (Play's real keys are the plain Space/Escape PIE binds;
    // Reset Camera has no binding). Pinning the exact strings here guards both.
    let (file, edit, play, view) = menu_entries();
    let accel = |entries: &[(String, Option<String>, Command)]| -> Vec<Option<String>> {
        entries.iter().map(|(_, s, _)| s.clone()).collect()
    };

    assert_eq!(
        accel(&file),
        vec![
            Some("Ctrl+O".to_owned()),
            Some("Ctrl+S".to_owned()),
            Some("Ctrl+Shift+S".to_owned()),
        ],
        "File items display Open=Ctrl+O, Save=Ctrl+S, Save-As=Ctrl+Shift+S"
    );
    assert_eq!(
        accel(&edit),
        vec![Some("Ctrl+Z".to_owned()), Some("Ctrl+Y".to_owned())],
        "Edit items display Undo=Ctrl+Z, Redo=Ctrl+Y"
    );
    assert!(
        play.iter().all(|(_, s, _)| s.is_none()),
        "Play items carry no accelerator display (Play's real keys are the plain \
         Space/Escape PIE binds, not menu accelerators)"
    );
    assert!(
        view.iter().all(|(_, s, _)| s.is_none()),
        "View items carry no accelerator display (Reset Camera has no binding)"
    );
}
