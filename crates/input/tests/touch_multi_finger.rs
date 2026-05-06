//! W13 exit: touch multi-finger (≥3 fingers) tracked independently with
//! stable IDs across the full `TouchStart` → `TouchMove` → `TouchEnd`
//! lifecycle.

// Loop counter `as f32` — exact small-integer round-trip, no precision concern.
#![allow(clippy::cast_precision_loss)]

use rge_input::event::{InputEvent, TouchId};
use rge_input::state::TouchState;

#[test]
fn three_fingers_independent() {
    let mut touch = TouchState::new();

    // Three fingers down.
    touch.handle_event(&InputEvent::TouchStart(TouchId(1), [10.0, 10.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(2), [20.0, 20.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(3), [30.0, 30.0]));
    assert_eq!(touch.count(), 3);
    assert!(touch.is_active(TouchId(1)));
    assert!(touch.is_active(TouchId(2)));
    assert!(touch.is_active(TouchId(3)));

    // Each finger reports its own position.
    assert_eq!(touch.position(TouchId(1)), Some([10.0, 10.0]));
    assert_eq!(touch.position(TouchId(2)), Some([20.0, 20.0]));
    assert_eq!(touch.position(TouchId(3)), Some([30.0, 30.0]));
}

#[test]
fn move_does_not_disturb_other_fingers() {
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchStart(TouchId(1), [10.0, 10.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(2), [20.0, 20.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(3), [30.0, 30.0]));

    touch.handle_event(&InputEvent::TouchMove(TouchId(2), [25.0, 25.0]));

    // Only finger 2 moved.
    assert_eq!(touch.position(TouchId(1)), Some([10.0, 10.0]));
    assert_eq!(touch.position(TouchId(2)), Some([25.0, 25.0]));
    assert_eq!(touch.position(TouchId(3)), Some([30.0, 30.0]));
    assert_eq!(touch.count(), 3);
}

#[test]
fn end_removes_only_specified_finger() {
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchStart(TouchId(1), [10.0, 10.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(2), [20.0, 20.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(3), [30.0, 30.0]));

    touch.handle_event(&InputEvent::TouchEnd(TouchId(2)));
    assert_eq!(touch.count(), 2);
    assert!(touch.is_active(TouchId(1)));
    assert!(!touch.is_active(TouchId(2)));
    assert!(touch.is_active(TouchId(3)));
}

#[test]
fn ids_remain_stable_across_moves() {
    // Repeated move on the same id keeps the id in the contact set —
    // no churn, no renaming.
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchStart(TouchId(42), [0.0, 0.0]));
    for x in 1..=10 {
        touch.handle_event(&InputEvent::TouchMove(TouchId(42), [x as f32, 0.0]));
    }
    assert_eq!(touch.count(), 1);
    assert_eq!(touch.position(TouchId(42)), Some([10.0, 0.0]));
}

#[test]
fn move_unknown_finger_ignored() {
    // Defensive: a TouchMove arriving without a prior TouchStart is dropped
    // (not synthesised into a phantom contact).
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchMove(TouchId(99), [5.0, 5.0]));
    assert_eq!(touch.count(), 0);
    assert!(!touch.is_active(TouchId(99)));
}

#[test]
fn iteration_order_is_insertion_order() {
    // First-finger-anchors-the-gesture pattern relies on this.
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchStart(TouchId(7), [1.0, 1.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(3), [2.0, 2.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(99), [3.0, 3.0]));

    let ids: Vec<TouchId> = touch.iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![TouchId(7), TouchId(3), TouchId(99)]);
}

#[test]
fn duplicate_start_ignored() {
    // Misbehaving backend that re-fires TouchStart for an already-active
    // contact must not double-add.
    let mut touch = TouchState::new();
    touch.handle_event(&InputEvent::TouchStart(TouchId(1), [0.0, 0.0]));
    touch.handle_event(&InputEvent::TouchStart(TouchId(1), [10.0, 10.0]));
    assert_eq!(touch.count(), 1);
    // Original position retained — re-fire is a no-op.
    assert_eq!(touch.position(TouchId(1)), Some([0.0, 0.0]));
}
