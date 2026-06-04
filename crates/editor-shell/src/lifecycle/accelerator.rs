//! `lifecycle::accelerator` — keyboard → menu-accelerator bridge (W08.2).
//!
//! [`keycode_to_shortcut`] translates a physical `rge_input::KeyCode` + the
//! Ctrl/Shift modifier flags into an `rge_editor_ui::menus::Shortcut` — the
//! accelerator vocabulary the canonical menu (`default_editor_menu`) is keyed by.
//! It is the shell-local half of accelerator execution: the translation MUST live
//! here because `editor-ui` cannot depend on `rge-input` (`forbidden-dep` rule 4),
//! so editor-shell — which depends on both — owns the bridge.
//!
//! W08.2 lands the translation + a PARITY guard. The `#[cfg(test)]` tests assert
//! that `EditorKeyCommand::from_key_press` and the menu's `command_for_shortcut`
//! agree on the five shared accelerators (Open / Save / Save-As / Undo / Redo),
//! and pin the two intentional asymmetries — `Ctrl+O` routes via the inline
//! `window_event` arm (not `EditorKeyCommand`), and the `Ctrl+2/0/4` time-scale
//! binds are execution-only with no menu home. The live keystroke path is
//! UNCHANGED: `from_key_press` is still the executor; W08.3 routes keystrokes
//! through this bridge + `command_for_shortcut`.

use rge_editor_ui::menus::{Key, Modifiers, Shortcut};
use rge_input::KeyCode;

/// Translate a physical [`KeyCode`] + Ctrl/Shift flags into the [`Shortcut`] the
/// canonical menu is keyed by.
///
/// Returns `None` when `key` is itself a modifier key (Ctrl/Shift/Alt/Super have
/// no standalone shortcut form). Letters map to [`Key::Char`] (uppercase), digits
/// to [`Key::Digit`], function keys to [`Key::Function`], and the edit / nav /
/// arrow keys to their named [`Key`] variants.
///
/// `Alt` / `Super` are not represented in the flags today — the accelerator
/// surface is Ctrl/Shift only, mirroring `EditorKeyCommand::from_key_press`;
/// extend the signature additively if a bus-bound Alt/Super accelerator lands.
///
/// W08.2 ships this translation + a parity guard but does NOT route through it;
/// `EditorKeyCommand::from_key_press` remains the live executor until W08.3.
#[must_use]
pub fn keycode_to_shortcut(key: KeyCode, ctrl: bool, shift: bool) -> Option<Shortcut> {
    let mut modifiers = Modifiers::empty();
    if ctrl {
        modifiers |= Modifiers::CTRL;
    }
    if shift {
        modifiers |= Modifiers::SHIFT;
    }
    Some(Shortcut::new(modifiers, keycode_to_key(key)?))
}

/// Map a physical [`KeyCode`] to the menu's non-modifier [`Key`]. Returns `None`
/// for the eight modifier keys (they are never the `key` of a [`Shortcut`]).
///
/// Exhaustive over `KeyCode` on purpose: a new physical key added to the input
/// surface forces a deliberate decision here rather than silently mapping to
/// nothing.
fn keycode_to_key(key: KeyCode) -> Option<Key> {
    Some(match key {
        KeyCode::KeyA => Key::Char('A'),
        KeyCode::KeyB => Key::Char('B'),
        KeyCode::KeyC => Key::Char('C'),
        KeyCode::KeyD => Key::Char('D'),
        KeyCode::KeyE => Key::Char('E'),
        KeyCode::KeyF => Key::Char('F'),
        KeyCode::KeyG => Key::Char('G'),
        KeyCode::KeyH => Key::Char('H'),
        KeyCode::KeyI => Key::Char('I'),
        KeyCode::KeyJ => Key::Char('J'),
        KeyCode::KeyK => Key::Char('K'),
        KeyCode::KeyL => Key::Char('L'),
        KeyCode::KeyM => Key::Char('M'),
        KeyCode::KeyN => Key::Char('N'),
        KeyCode::KeyO => Key::Char('O'),
        KeyCode::KeyP => Key::Char('P'),
        KeyCode::KeyQ => Key::Char('Q'),
        KeyCode::KeyR => Key::Char('R'),
        KeyCode::KeyS => Key::Char('S'),
        KeyCode::KeyT => Key::Char('T'),
        KeyCode::KeyU => Key::Char('U'),
        KeyCode::KeyV => Key::Char('V'),
        KeyCode::KeyW => Key::Char('W'),
        KeyCode::KeyX => Key::Char('X'),
        KeyCode::KeyY => Key::Char('Y'),
        KeyCode::KeyZ => Key::Char('Z'),
        KeyCode::Digit0 => Key::Digit(0),
        KeyCode::Digit1 => Key::Digit(1),
        KeyCode::Digit2 => Key::Digit(2),
        KeyCode::Digit3 => Key::Digit(3),
        KeyCode::Digit4 => Key::Digit(4),
        KeyCode::Digit5 => Key::Digit(5),
        KeyCode::Digit6 => Key::Digit(6),
        KeyCode::Digit7 => Key::Digit(7),
        KeyCode::Digit8 => Key::Digit(8),
        KeyCode::Digit9 => Key::Digit(9),
        KeyCode::F1 => Key::Function(1),
        KeyCode::F2 => Key::Function(2),
        KeyCode::F3 => Key::Function(3),
        KeyCode::F4 => Key::Function(4),
        KeyCode::F5 => Key::Function(5),
        KeyCode::F6 => Key::Function(6),
        KeyCode::F7 => Key::Function(7),
        KeyCode::F8 => Key::Function(8),
        KeyCode::F9 => Key::Function(9),
        KeyCode::F10 => Key::Function(10),
        KeyCode::F11 => Key::Function(11),
        KeyCode::F12 => Key::Function(12),
        KeyCode::Space => Key::Space,
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        // The eight modifier keys are never a shortcut's `key`.
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::AltLeft
        | KeyCode::AltRight
        | KeyCode::SuperLeft
        | KeyCode::SuperRight => return None,
    })
}

#[cfg(test)]
mod tests {
    use rge_editor_ui::menus::{
        default_editor_menu, Command, Key, Modifiers, PredicateContext, Shortcut,
    };
    use rge_input::KeyCode;

    use super::keycode_to_shortcut;
    use crate::EditorKeyCommand;

    #[test]
    fn keycode_to_shortcut_maps_letters_digits_and_no_modifiers() {
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyO, true, false),
            Some(Shortcut::new(Modifiers::CTRL, Key::Char('O')))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyS, true, true),
            Some(Shortcut::new(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Char('S')
            ))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::Digit2, true, false),
            Some(Shortcut::new(Modifiers::CTRL, Key::Digit(2)))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyR, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Char('R'))),
            "no modifiers held -> a plain shortcut"
        );
    }

    #[test]
    fn keycode_to_shortcut_maps_function_and_nav_keys() {
        assert_eq!(
            keycode_to_shortcut(KeyCode::F5, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Function(5)))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::ArrowUp, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Up))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::Delete, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Delete))
        );
    }

    #[test]
    fn keycode_to_shortcut_rejects_modifier_keys() {
        // A modifier key has no standalone shortcut form.
        assert_eq!(keycode_to_shortcut(KeyCode::ControlLeft, true, false), None);
        assert_eq!(keycode_to_shortcut(KeyCode::ShiftRight, false, true), None);
        assert_eq!(keycode_to_shortcut(KeyCode::AltLeft, false, false), None);
        assert_eq!(keycode_to_shortcut(KeyCode::SuperLeft, false, false), None);
    }

    #[test]
    fn keyboard_map_and_menu_agree_on_shared_accelerators() {
        // The five shared accelerators have BOTH an executable editor-shell
        // binding and a canonical-menu binding. This test pins that they resolve
        // to the SAME logical command so the two maps cannot silently diverge.
        // W08.2 only adds the guard; `from_key_press` is still the live executor
        // (the cutover is W08.3).
        let menu = default_editor_menu().resolve(&PredicateContext::default());

        // The four binds EditorKeyCommand routes (Ctrl+S / Ctrl+Shift+S / Ctrl+Z /
        // Ctrl+Y). Each drives BOTH maps with the same (KeyCode, ctrl, shift); the
        // resulting commands must correspond.
        let shared = [
            (
                KeyCode::KeyS,
                true,
                false,
                EditorKeyCommand::Save,
                Command::Save,
            ),
            (
                KeyCode::KeyS,
                true,
                true,
                EditorKeyCommand::SaveAsProject,
                Command::SaveAs,
            ),
            (
                KeyCode::KeyZ,
                true,
                false,
                EditorKeyCommand::Undo,
                Command::Undo,
            ),
            (
                KeyCode::KeyY,
                true,
                false,
                EditorKeyCommand::Redo,
                Command::Redo,
            ),
        ];
        for (key, ctrl, shift, key_command, menu_command) in shared {
            assert_eq!(
                EditorKeyCommand::from_key_press(key, ctrl, shift),
                Some(key_command),
                "editor-shell keyboard map for {key:?} ctrl={ctrl} shift={shift}"
            );
            let shortcut = keycode_to_shortcut(key, ctrl, shift)
                .expect("a shared accelerator translates to a Shortcut");
            assert_eq!(
                menu.command_for_shortcut(&shortcut),
                Some(&menu_command),
                "canonical menu binding for {key:?} ctrl={ctrl} shift={shift} \
                 must match the editor-shell keyboard command"
            );
        }

        // Open (Ctrl+O) is a shared bind, but editor-shell routes it via the
        // inline `window_event` arm (`handle_open_request`), NOT EditorKeyCommand,
        // so `from_key_press` returns None while the menu binds Ctrl+O -> OpenFile.
        // W08.3's cutover collapses the inline arm into the command_for_shortcut
        // path.
        assert_eq!(
            EditorKeyCommand::from_key_press(KeyCode::KeyO, true, false),
            None,
            "Open is the inline window_event arm, not an EditorKeyCommand"
        );
        let ctrl_o = keycode_to_shortcut(KeyCode::KeyO, true, false).unwrap();
        assert_eq!(
            menu.command_for_shortcut(&ctrl_o),
            Some(&Command::OpenFile),
            "the canonical menu binds Ctrl+O to OpenFile"
        );

        // Time-scale binds (Ctrl+2/0/4) are execution-only: EditorKeyCommand
        // routes them, but they have NO menu entry, so the canonical menu does not
        // bind them. A future menu entry for any of these would (correctly) fail
        // this assertion, forcing the parity question to be answered.
        for (key, key_command) in [
            (KeyCode::Digit2, EditorKeyCommand::SetTimeScaleDoubleSpeed),
            (KeyCode::Digit0, EditorKeyCommand::ResetTimeScaleDefault),
            (
                KeyCode::Digit4,
                EditorKeyCommand::SetTimeScaleMaxFastForward,
            ),
        ] {
            assert_eq!(
                EditorKeyCommand::from_key_press(key, true, false),
                Some(key_command)
            );
            let shortcut = keycode_to_shortcut(key, true, false).unwrap();
            assert_eq!(
                menu.command_for_shortcut(&shortcut),
                None,
                "time-scale binds are execution-only — no menu home"
            );
        }
    }
}
