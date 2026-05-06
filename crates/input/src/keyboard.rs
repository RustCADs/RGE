// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Keyboard fan-in: winit `KeyboardInput` → `InputEvent::KeyDown/KeyUp`.
//!
//! v0 surfaces a curated `KeyCode` enum that is a 1:1 subset of
//! `winit::keyboard::KeyCode`. Carrying our own type rather than re-exporting
//! winit's keeps consumer crates from a transitive winit dep when they only
//! want to switch on `KeyCode::KeyR`. Translation is a fast match and
//! returns `None` for keys outside the v0 surface — those are dropped at
//! the fan-in boundary so unrecognised codes don't pollute the event stream.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};

use crate::event::{InputEvent, Pressed};

/// Curated physical-key code surface. 1:1 names with `winit::keyboard::KeyCode`
/// for the subset v0 cares about — letters, digits, function keys, arrows,
/// the common modifier + edit + navigation set.
///
/// Codes outside this set are dropped by `translate_keyboard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // Letters
    /// `A` key.
    KeyA,
    /// `B` key.
    KeyB,
    /// `C` key.
    KeyC,
    /// `D` key.
    KeyD,
    /// `E` key.
    KeyE,
    /// `F` key.
    KeyF,
    /// `G` key.
    KeyG,
    /// `H` key.
    KeyH,
    /// `I` key.
    KeyI,
    /// `J` key.
    KeyJ,
    /// `K` key.
    KeyK,
    /// `L` key.
    KeyL,
    /// `M` key.
    KeyM,
    /// `N` key.
    KeyN,
    /// `O` key.
    KeyO,
    /// `P` key.
    KeyP,
    /// `Q` key.
    KeyQ,
    /// `R` key.
    KeyR,
    /// `S` key.
    KeyS,
    /// `T` key.
    KeyT,
    /// `U` key.
    KeyU,
    /// `V` key.
    KeyV,
    /// `W` key.
    KeyW,
    /// `X` key.
    KeyX,
    /// `Y` key.
    KeyY,
    /// `Z` key.
    KeyZ,

    // Digits (top row)
    /// Top-row `0`.
    Digit0,
    /// Top-row `1`.
    Digit1,
    /// Top-row `2`.
    Digit2,
    /// Top-row `3`.
    Digit3,
    /// Top-row `4`.
    Digit4,
    /// Top-row `5`.
    Digit5,
    /// Top-row `6`.
    Digit6,
    /// Top-row `7`.
    Digit7,
    /// Top-row `8`.
    Digit8,
    /// Top-row `9`.
    Digit9,

    // Function keys
    /// `F1`.
    F1,
    /// `F2`.
    F2,
    /// `F3`.
    F3,
    /// `F4`.
    F4,
    /// `F5`.
    F5,
    /// `F6`.
    F6,
    /// `F7`.
    F7,
    /// `F8`.
    F8,
    /// `F9`.
    F9,
    /// `F10`.
    F10,
    /// `F11`.
    F11,
    /// `F12`.
    F12,

    // Modifiers
    /// Left Shift.
    ShiftLeft,
    /// Right Shift.
    ShiftRight,
    /// Left Ctrl.
    ControlLeft,
    /// Right Ctrl.
    ControlRight,
    /// Left Alt.
    AltLeft,
    /// Right Alt.
    AltRight,
    /// Left Super (Windows / Cmd).
    SuperLeft,
    /// Right Super (Windows / Cmd).
    SuperRight,

    // Edit / nav
    /// Spacebar.
    Space,
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,

    // Arrows
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
}

/// Translate a winit `KeyEvent` into an `InputEvent::KeyDown`/`KeyUp`.
///
/// Returns `None` when:
/// - the physical key is outside our v0 `KeyCode` surface (e.g. `IntlYen`,
///   numeric keypad), or
/// - the key event is a virtual `Code::Unidentified` / dead-key composition.
///
/// Repeat events (`KeyEvent::repeat == true`) are forwarded as `KeyDown`
/// regardless of repeat — `state.rs` filters duplicates via `pressed_set`,
/// matching the rustforge editor-app pattern of "press already tracked,
/// no state change".
#[must_use]
pub fn translate_keyboard(evt: &KeyEvent) -> Option<InputEvent> {
    let PhysicalKey::Code(code) = evt.physical_key else {
        return None;
    };
    let key = winit_keycode_to_rge(code)?;
    let pressed = match evt.state {
        ElementState::Pressed => Pressed::Down,
        ElementState::Released => Pressed::Up,
    };
    Some(match pressed {
        Pressed::Down => InputEvent::KeyDown(key),
        Pressed::Up => InputEvent::KeyUp(key),
    })
}

/// Map `winit::keyboard::KeyCode` to the v0 `KeyCode` surface. Pattern lifted
/// from `rustforge::apps::editor-app::app::winit_keycode_to_rcad` and widened
/// from the 2-key editor subset to the full v0 surface.
#[allow(clippy::too_many_lines)]
fn winit_keycode_to_rge(code: WinitKeyCode) -> Option<KeyCode> {
    Some(match code {
        // Letters
        WinitKeyCode::KeyA => KeyCode::KeyA,
        WinitKeyCode::KeyB => KeyCode::KeyB,
        WinitKeyCode::KeyC => KeyCode::KeyC,
        WinitKeyCode::KeyD => KeyCode::KeyD,
        WinitKeyCode::KeyE => KeyCode::KeyE,
        WinitKeyCode::KeyF => KeyCode::KeyF,
        WinitKeyCode::KeyG => KeyCode::KeyG,
        WinitKeyCode::KeyH => KeyCode::KeyH,
        WinitKeyCode::KeyI => KeyCode::KeyI,
        WinitKeyCode::KeyJ => KeyCode::KeyJ,
        WinitKeyCode::KeyK => KeyCode::KeyK,
        WinitKeyCode::KeyL => KeyCode::KeyL,
        WinitKeyCode::KeyM => KeyCode::KeyM,
        WinitKeyCode::KeyN => KeyCode::KeyN,
        WinitKeyCode::KeyO => KeyCode::KeyO,
        WinitKeyCode::KeyP => KeyCode::KeyP,
        WinitKeyCode::KeyQ => KeyCode::KeyQ,
        WinitKeyCode::KeyR => KeyCode::KeyR,
        WinitKeyCode::KeyS => KeyCode::KeyS,
        WinitKeyCode::KeyT => KeyCode::KeyT,
        WinitKeyCode::KeyU => KeyCode::KeyU,
        WinitKeyCode::KeyV => KeyCode::KeyV,
        WinitKeyCode::KeyW => KeyCode::KeyW,
        WinitKeyCode::KeyX => KeyCode::KeyX,
        WinitKeyCode::KeyY => KeyCode::KeyY,
        WinitKeyCode::KeyZ => KeyCode::KeyZ,

        // Digits
        WinitKeyCode::Digit0 => KeyCode::Digit0,
        WinitKeyCode::Digit1 => KeyCode::Digit1,
        WinitKeyCode::Digit2 => KeyCode::Digit2,
        WinitKeyCode::Digit3 => KeyCode::Digit3,
        WinitKeyCode::Digit4 => KeyCode::Digit4,
        WinitKeyCode::Digit5 => KeyCode::Digit5,
        WinitKeyCode::Digit6 => KeyCode::Digit6,
        WinitKeyCode::Digit7 => KeyCode::Digit7,
        WinitKeyCode::Digit8 => KeyCode::Digit8,
        WinitKeyCode::Digit9 => KeyCode::Digit9,

        // F-keys
        WinitKeyCode::F1 => KeyCode::F1,
        WinitKeyCode::F2 => KeyCode::F2,
        WinitKeyCode::F3 => KeyCode::F3,
        WinitKeyCode::F4 => KeyCode::F4,
        WinitKeyCode::F5 => KeyCode::F5,
        WinitKeyCode::F6 => KeyCode::F6,
        WinitKeyCode::F7 => KeyCode::F7,
        WinitKeyCode::F8 => KeyCode::F8,
        WinitKeyCode::F9 => KeyCode::F9,
        WinitKeyCode::F10 => KeyCode::F10,
        WinitKeyCode::F11 => KeyCode::F11,
        WinitKeyCode::F12 => KeyCode::F12,

        // Modifiers
        WinitKeyCode::ShiftLeft => KeyCode::ShiftLeft,
        WinitKeyCode::ShiftRight => KeyCode::ShiftRight,
        WinitKeyCode::ControlLeft => KeyCode::ControlLeft,
        WinitKeyCode::ControlRight => KeyCode::ControlRight,
        WinitKeyCode::AltLeft => KeyCode::AltLeft,
        WinitKeyCode::AltRight => KeyCode::AltRight,
        WinitKeyCode::SuperLeft => KeyCode::SuperLeft,
        WinitKeyCode::SuperRight => KeyCode::SuperRight,

        // Edit / nav
        WinitKeyCode::Space => KeyCode::Space,
        WinitKeyCode::Enter => KeyCode::Enter,
        WinitKeyCode::Escape => KeyCode::Escape,
        WinitKeyCode::Tab => KeyCode::Tab,
        WinitKeyCode::Backspace => KeyCode::Backspace,
        WinitKeyCode::Delete => KeyCode::Delete,
        WinitKeyCode::Insert => KeyCode::Insert,
        WinitKeyCode::Home => KeyCode::Home,
        WinitKeyCode::End => KeyCode::End,
        WinitKeyCode::PageUp => KeyCode::PageUp,
        WinitKeyCode::PageDown => KeyCode::PageDown,

        // Arrows
        WinitKeyCode::ArrowUp => KeyCode::ArrowUp,
        WinitKeyCode::ArrowDown => KeyCode::ArrowDown,
        WinitKeyCode::ArrowLeft => KeyCode::ArrowLeft,
        WinitKeyCode::ArrowRight => KeyCode::ArrowRight,

        // Anything outside the v0 surface drops at the fan-in boundary.
        _ => return None,
    })
}
