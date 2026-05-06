//! W13 exit: keyboard press/release transitions tracked correctly via
//! `Input<KeyCode>`. Mirrors mouse-button discipline in the same suite so
//! the shared `Input<T>` machinery is exercised on more than one button
//! family.

use rge_input::event::{InputEvent, Pressed};
use rge_input::keyboard::KeyCode;
use rge_input::mouse::MouseButton;
use rge_input::state::Input;

#[test]
fn keyboard_press_then_release() {
    let mut keys: Input<KeyCode> = Input::new();

    // Frame 1: press W
    keys.handle_event(&InputEvent::KeyDown(KeyCode::KeyW));
    assert!(keys.pressed(KeyCode::KeyW));
    assert!(keys.just_pressed(KeyCode::KeyW));
    assert!(!keys.just_released(KeyCode::KeyW));

    // Frame 2: roll the just-* windows; W still held but no longer "just"
    keys.clear_just();
    assert!(keys.pressed(KeyCode::KeyW));
    assert!(!keys.just_pressed(KeyCode::KeyW));
    assert!(!keys.just_released(KeyCode::KeyW));

    // Frame 3: release W
    keys.handle_event(&InputEvent::KeyUp(KeyCode::KeyW));
    assert!(!keys.pressed(KeyCode::KeyW));
    assert!(keys.just_released(KeyCode::KeyW));
}

#[test]
fn keyboard_repeat_does_not_double_emit() {
    // Auto-repeat / multiple KeyDown without intervening KeyUp must NOT
    // re-fire just_pressed — matches the rustforge editor-app convention
    // of "press already tracked, no fresh dispatch".
    let mut keys: Input<KeyCode> = Input::new();
    keys.handle_event(&InputEvent::KeyDown(KeyCode::KeyA));
    keys.clear_just();
    keys.handle_event(&InputEvent::KeyDown(KeyCode::KeyA));
    assert!(keys.pressed(KeyCode::KeyA));
    assert!(!keys.just_pressed(KeyCode::KeyA));
}

#[test]
fn keyboard_unrelated_release_no_op() {
    let mut keys: Input<KeyCode> = Input::new();
    // Releasing a key that was never pressed is a silent no-op.
    keys.handle_event(&InputEvent::KeyUp(KeyCode::KeyZ));
    assert!(!keys.pressed(KeyCode::KeyZ));
    assert!(!keys.just_released(KeyCode::KeyZ));
}

#[test]
fn keyboard_multiple_keys_independent() {
    let mut keys: Input<KeyCode> = Input::new();
    keys.handle_event(&InputEvent::KeyDown(KeyCode::ControlLeft));
    keys.handle_event(&InputEvent::KeyDown(KeyCode::KeyS));
    assert!(keys.pressed(KeyCode::ControlLeft));
    assert!(keys.pressed(KeyCode::KeyS));

    keys.handle_event(&InputEvent::KeyUp(KeyCode::KeyS));
    assert!(keys.pressed(KeyCode::ControlLeft));
    assert!(!keys.pressed(KeyCode::KeyS));
}

#[test]
fn mouse_button_press_then_release() {
    let mut buttons: Input<MouseButton> = Input::new();

    buttons.handle_event(&InputEvent::MouseButton(MouseButton::Left, Pressed::Down));
    assert!(buttons.pressed(MouseButton::Left));
    assert!(buttons.just_pressed(MouseButton::Left));

    buttons.clear_just();
    buttons.handle_event(&InputEvent::MouseButton(MouseButton::Left, Pressed::Up));
    assert!(!buttons.pressed(MouseButton::Left));
    assert!(buttons.just_released(MouseButton::Left));
}

#[test]
fn mouse_button_ignores_unrelated_events() {
    // Input<MouseButton> ignores keyboard events.
    let mut buttons: Input<MouseButton> = Input::new();
    buttons.handle_event(&InputEvent::KeyDown(KeyCode::KeyR));
    assert!(buttons.iter_pressed().count() == 0);
}
