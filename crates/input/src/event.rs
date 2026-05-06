// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Unified `InputEvent` enum: every device — keyboard, mouse, gamepad, touch,
//! stylus, XR (reserved) — funnels into a single ordered stream consumed by
//! ECS systems. Per W13 §1: one event vocabulary, one queue, no per-device
//! dispatch in caller scope.
//!
//! Vec-style data is encoded as `[f32; 2]` to keep this crate free of any
//! math-crate dependency (matches the `components-spatial::Transform` v0
//! discipline of plain arrays). Downstream crates can reinterpret as their
//! own `Vec2` type at consume time.
//!
//! XR variant: `InputEvent::Xr` is a reserved slot. Payload type is the unit
//! `XrEvent` placeholder — Phase 5+ will replace with hand/eye/controller
//! pose deltas. Reserving the variant in v0 keeps the enum closed-shape
//! stable across the XR landing wave (no `#[non_exhaustive]` churn).

use crate::keyboard::KeyCode;
use crate::mouse::{MouseButton, ScrollDelta};

/// Logical pressed/released state for buttons (keyboard, mouse, gamepad).
///
/// Keeping this as a dedicated enum (rather than `bool`) makes call-sites
/// self-documenting: `Pressed::Down` reads cleaner than `true`, and the type
/// is open to a future `Repeat` variant without source-level breakage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// Button transitioned to or remains in the down state.
    Down,
    /// Button transitioned to the up state.
    Up,
}

impl Pressed {
    /// `true` if this is `Down`.
    #[inline]
    #[must_use]
    pub const fn is_down(self) -> bool {
        matches!(self, Pressed::Down)
    }
}

/// Stable identifier for a connected gamepad. Wraps `gilrs::GamepadId` as a
/// transparent `u32` so this type is `Copy + Eq + Hash` without dragging the
/// gilrs type through public API surface — keeps consumer crates from a
/// transitive gilrs dep when they only want to filter events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GamepadId(pub u32);

/// Standard gamepad button mapping (`XInput` / `DualShock` harmonized layout).
/// Mirrors the subset of `gilrs::Button` we surface in v0; vendor-specific
/// buttons (paddle, touchpad, share) are filtered out at translation time
/// to keep the event vocabulary platform-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    /// South face button (A on Xbox, X on `PlayStation`).
    South,
    /// East face button (B on Xbox, Circle on `PlayStation`).
    East,
    /// North face button (Y on Xbox, Triangle on `PlayStation`).
    North,
    /// West face button (X on Xbox, Square on `PlayStation`).
    West,
    /// Left shoulder bumper (LB / L1).
    LeftShoulder,
    /// Right shoulder bumper (RB / R1).
    RightShoulder,
    /// Left trigger as a digital button (post-threshold).
    LeftTrigger,
    /// Right trigger as a digital button (post-threshold).
    RightTrigger,
    /// Select / View / Share button.
    Select,
    /// Start / Menu / Options button.
    Start,
    /// Left analog stick click (L3).
    LeftStick,
    /// Right analog stick click (R3).
    RightStick,
    /// D-pad up.
    DPadUp,
    /// D-pad down.
    DPadDown,
    /// D-pad left.
    DPadLeft,
    /// D-pad right.
    DPadRight,
}

/// Standard gamepad axis. Sticks emit normalized `[-1.0, 1.0]`; triggers emit
/// normalized `[0.0, 1.0]`. Dead-zone applied upstream in `gamepad::poll`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    /// Left stick X axis (-1 = left, +1 = right).
    LeftStickX,
    /// Left stick Y axis (-1 = down, +1 = up).
    LeftStickY,
    /// Right stick X axis.
    RightStickX,
    /// Right stick Y axis.
    RightStickY,
    /// Left trigger as analog axis (0..1).
    LeftTrigger,
    /// Right trigger as analog axis (0..1).
    RightTrigger,
}

/// Stable identifier for a single touch contact. winit gives us
/// `winit::event::Touch::id` as `u64`; we wrap to keep the fan-in API
/// independent of winit's exact type alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TouchId(pub u64);

/// Reserved XR event placeholder. Phase 5+ replaces with hand/eye/controller
/// pose deltas. Held as a unit struct so `InputEvent::Xr` is a closed shape
/// today and can grow internally without an enum-variant break later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrEvent;

/// Unified input event. Single ordered stream covering every device the
/// engine recognises in v0.
///
/// Polling order (per `crate::lib`): keyboard + mouse + touch + stylus from
/// winit `WindowEvent`, then gamepad from `gilrs::poll`. Frame-stable order
/// is the consumer's responsibility (push events into ECS in poll order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// Keyboard key transitioned to pressed.
    KeyDown(KeyCode),
    /// Keyboard key transitioned to released.
    KeyUp(KeyCode),
    /// Cursor moved to absolute position (logical pixels, top-left origin).
    MouseMove([f32; 2]),
    /// Mouse button state change.
    MouseButton(MouseButton, Pressed),
    /// Scroll wheel delta. Pixel-accurate platforms report fractional units.
    Scroll(ScrollDelta),
    /// Gamepad button state change. `GamepadId` disambiguates multi-pad.
    GamepadButton(GamepadId, GamepadButton, Pressed),
    /// Gamepad axis position after dead-zone normalization.
    GamepadAxis(GamepadId, GamepadAxis, f32),
    /// Touch contact began at absolute position.
    TouchStart(TouchId, [f32; 2]),
    /// Touch contact moved to absolute position.
    TouchMove(TouchId, [f32; 2]),
    /// Touch contact ended (lifted or cancelled).
    TouchEnd(TouchId),
    /// Stylus pressure update (`[0.0, 1.0]`). Position arrives via `MouseMove`
    /// on platforms that funnel pen events through the cursor; v0 keeps the
    /// pressure channel separate so consumers can filter "stylus only".
    StylusPressure(f32),
    /// Reserved: XR controller / hand / eye event. Phase 5+.
    Xr(XrEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_is_down() {
        assert!(Pressed::Down.is_down());
        assert!(!Pressed::Up.is_down());
    }

    #[test]
    fn xr_variant_constructs() {
        // Smoke: the reserved XR slot must be reachable so downstream code
        // can match exhaustively against `InputEvent` today.
        let e = InputEvent::Xr(XrEvent);
        assert!(matches!(e, InputEvent::Xr(_)));
    }

    #[test]
    fn ids_are_copy_and_hash() {
        // GamepadId / TouchId are Copy + Eq + Hash so they slot into
        // HashMap keys (state.rs uses them).
        let a = GamepadId(0);
        let b = a;
        assert_eq!(a, b);

        let t = TouchId(7);
        let u = t;
        assert_eq!(t, u);
    }
}
