// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! `Input<T>` ECS-resource — answers "is `T` currently pressed?" from a
//! stream of `InputEvent`s. Bevy-shape API (`pressed` / `just_pressed` /
//! `just_released`) so the surface is familiar.
//!
//! Generic over the button type so the same struct serves keyboard
//! (`Input<KeyCode>`), mouse (`Input<MouseButton>`), and gamepad
//! (`Input<GamepadButton>`). Caller drives state by piping each
//! `InputEvent` through `Input::handle_event` per frame and calling
//! `clear_just` between frames to roll the just-pressed / just-released
//! transition sets.
//!
//! Implementation note: we use `Vec<T>` storage with linear scans rather
//! than `HashSet` because the typical "currently held" set is tiny (≤8
//! keys, 2 mouse buttons, 4 gamepad faces). Linear is faster than hash
//! at that size and keeps the crate dep-free of `hashbrown`/`std::collections`
//! beyond what's already available.

use crate::event::{GamepadButton, GamepadId, InputEvent, Pressed, TouchId};
use crate::keyboard::KeyCode;
use crate::mouse::MouseButton;

/// "Is this button currently pressed?" tracker. Generic over the button
/// type. Construct with `default()`; call `handle_event` each time a new
/// `InputEvent` arrives; call `clear_just` once per frame to advance the
/// just-pressed / just-released windows.
#[derive(Debug, Clone)]
pub struct Input<T: Copy + PartialEq> {
    pressed: Vec<T>,
    just_pressed: Vec<T>,
    just_released: Vec<T>,
}

impl<T: Copy + PartialEq> Default for Input<T> {
    fn default() -> Self {
        Self {
            pressed: Vec::new(),
            just_pressed: Vec::new(),
            just_released: Vec::new(),
        }
    }
}

impl<T: Copy + PartialEq> Input<T> {
    /// Construct an empty tracker. Same as `default`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` if `value` is in the held-down set.
    #[must_use]
    pub fn pressed(&self, value: T) -> bool {
        self.pressed.contains(&value)
    }

    /// `true` if `value` transitioned to pressed during the current frame.
    #[must_use]
    pub fn just_pressed(&self, value: T) -> bool {
        self.just_pressed.contains(&value)
    }

    /// `true` if `value` transitioned to released during the current frame.
    #[must_use]
    pub fn just_released(&self, value: T) -> bool {
        self.just_released.contains(&value)
    }

    /// Iterate the currently-held set.
    pub fn iter_pressed(&self) -> impl Iterator<Item = T> + '_ {
        self.pressed.iter().copied()
    }

    /// Mark `value` as pressed. Idempotent — repeats from key auto-repeat
    /// don't double-emit `just_pressed` (matches the rustforge editor-app
    /// pattern: a key already-held generates no fresh action dispatch).
    pub fn press(&mut self, value: T) {
        if !self.pressed(value) {
            self.pressed.push(value);
            self.just_pressed.push(value);
        }
    }

    /// Mark `value` as released. No-op if not currently pressed.
    pub fn release(&mut self, value: T) {
        if let Some(pos) = self.pressed.iter().position(|x| *x == value) {
            self.pressed.swap_remove(pos);
            self.just_released.push(value);
        }
    }

    /// Roll the just-pressed / just-released windows. Call once per frame
    /// after consumers have observed the current frame's transitions.
    pub fn clear_just(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

impl Input<KeyCode> {
    /// Update from a single `InputEvent` — keyboard variants only; others
    /// are ignored.
    pub fn handle_event(&mut self, evt: &InputEvent) {
        match *evt {
            InputEvent::KeyDown(k) => self.press(k),
            InputEvent::KeyUp(k) => self.release(k),
            _ => {}
        }
    }
}

impl Input<MouseButton> {
    /// Update from a single `InputEvent` — mouse-button variants only.
    pub fn handle_event(&mut self, evt: &InputEvent) {
        if let InputEvent::MouseButton(b, p) = *evt {
            match p {
                Pressed::Down => self.press(b),
                Pressed::Up => self.release(b),
            }
        }
    }
}

impl Input<GamepadButton> {
    /// Update from a single `InputEvent` — gamepad-button variants only.
    /// Note: this ignores `GamepadId`, so a multi-pad caller should keep
    /// one `Input<GamepadButton>` per pad (or upgrade to a wrapper that
    /// keys on `(GamepadId, GamepadButton)`).
    pub fn handle_event(&mut self, evt: &InputEvent) {
        if let InputEvent::GamepadButton(_id, b, p) = *evt {
            match p {
                Pressed::Down => self.press(b),
                Pressed::Up => self.release(b),
            }
        }
    }
}

/// Multi-finger touch state. Records the live position of every active
/// contact, keyed by stable `TouchId`. Callers query `iter()` for "all
/// fingers currently down" or `position(id)` for a specific contact.
///
/// Stored as `Vec<(TouchId, [f32; 2])>` (linear scan) — typical multi-touch
/// gestures peak at 5 contacts on phones, 10 on tablets; Vec wins over
/// `HashMap` up through that range.
#[derive(Debug, Clone, Default)]
pub struct TouchState {
    contacts: Vec<(TouchId, [f32; 2])>,
}

impl TouchState {
    /// Construct an empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently-active contacts.
    #[must_use]
    pub fn count(&self) -> usize {
        self.contacts.len()
    }

    /// Iterate `(id, position)` pairs for every active contact. Order is
    /// the insertion order of `TouchStart` events — useful for "first
    /// finger anchors the gesture" patterns.
    pub fn iter(&self) -> impl Iterator<Item = (TouchId, [f32; 2])> + '_ {
        self.contacts.iter().copied()
    }

    /// Look up a contact's current position by id.
    #[must_use]
    pub fn position(&self, id: TouchId) -> Option<[f32; 2]> {
        self.contacts
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, p)| *p)
    }

    /// `true` if `id` is currently in the contact set.
    #[must_use]
    pub fn is_active(&self, id: TouchId) -> bool {
        self.contacts.iter().any(|(i, _)| *i == id)
    }

    /// Update from a single `InputEvent` — touch variants only.
    pub fn handle_event(&mut self, evt: &InputEvent) {
        match *evt {
            InputEvent::TouchStart(id, pos) => {
                if !self.is_active(id) {
                    self.contacts.push((id, pos));
                }
            }
            InputEvent::TouchMove(id, pos) => {
                if let Some(slot) = self.contacts.iter_mut().find(|(i, _)| *i == id) {
                    slot.1 = pos;
                }
            }
            InputEvent::TouchEnd(id) => {
                if let Some(idx) = self.contacts.iter().position(|(i, _)| *i == id) {
                    self.contacts.swap_remove(idx);
                }
            }
            _ => {}
        }
    }
}

/// Per-pad analog axis state. Tracks the most recent value per axis on the
/// pad — trigger / stick callers do `axes.value(GamepadAxis::LeftStickX)`
/// per frame rather than re-deriving from the event stream.
#[derive(Debug, Clone, Default)]
pub struct GamepadAxes {
    /// Owner identifier; v0 single-pad convention puts pad 0 here.
    pub id: Option<GamepadId>,
    values: Vec<(crate::event::GamepadAxis, f32)>,
}

impl GamepadAxes {
    /// Construct an empty axis state for a specific pad.
    #[must_use]
    pub fn new(id: GamepadId) -> Self {
        Self {
            id: Some(id),
            values: Vec::new(),
        }
    }

    /// Most recent value for `axis`. `0.0` if never observed.
    #[must_use]
    pub fn value(&self, axis: crate::event::GamepadAxis) -> f32 {
        self.values
            .iter()
            .find(|(a, _)| *a == axis)
            .map_or(0.0, |(_, v)| *v)
    }

    /// Update from a single `InputEvent` — gamepad-axis variants only,
    /// filtered to this pad's `id` when set.
    pub fn handle_event(&mut self, evt: &InputEvent) {
        if let InputEvent::GamepadAxis(id, axis, value) = *evt {
            if self.id.is_some() && self.id != Some(id) {
                return;
            }
            if let Some(slot) = self.values.iter_mut().find(|(a, _)| *a == axis) {
                slot.1 = value;
            } else {
                self.values.push((axis, value));
            }
        }
    }
}
