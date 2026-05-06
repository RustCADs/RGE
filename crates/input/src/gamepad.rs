// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Gamepad fan-in: drain `gilrs::Gilrs::next_event()` and translate to
//! `InputEvent`. Dead-zone normalization is applied per-axis so fan-in
//! consumers see clean centered axes (no jitter at rest).
//!
//! The `GamepadPoller` wrapper holds a `gilrs::Gilrs` and a configurable
//! per-axis dead-zone. Default dead-zone is `0.1` per W13 exit criteria —
//! suppressing `|x| < 0.1` keeps stick centering noise out of the event
//! stream without losing perceptible motion.

use gilrs::{Axis, Button, EventType, Gilrs, GilrsBuilder};

use crate::event::{GamepadAxis, GamepadButton, GamepadId, InputEvent, Pressed};

/// Default radial dead-zone for stick + trigger axes. Values below this
/// magnitude are clamped to zero. Matches the W13 exit criterion
/// (`|x| < 0.1` suppressed).
pub const DEFAULT_DEAD_ZONE: f32 = 0.1;

/// Owns a `gilrs::Gilrs` instance and translates polled events. Construct
/// once at startup and call `poll` each frame to drain queued events.
///
/// Carrying our own dead-zone (rather than reusing gilrs's per-axis filter)
/// means the engine surface is consistent across platforms — gilrs's filter
/// is a hint that not every backend honours, while a post-poll clamp is
/// definitive.
pub struct GamepadPoller {
    gilrs: Gilrs,
    dead_zone: f32,
}

impl GamepadPoller {
    /// Construct with the default `0.1` dead-zone.
    ///
    /// Returns `None` when gilrs cannot initialise the platform backend
    /// (no gamepad subsystem available, e.g. CI containers, headless
    /// servers). Callers that want input on those targets should treat
    /// `None` as a graceful degradation, not a panic.
    #[must_use]
    pub fn new() -> Option<Self> {
        Self::with_dead_zone(DEFAULT_DEAD_ZONE)
    }

    /// Construct with a custom dead-zone in `[0.0, 1.0)`.
    ///
    /// Returns `None` on backend init failure — see `new` for rationale.
    #[must_use]
    pub fn with_dead_zone(dead_zone: f32) -> Option<Self> {
        let gilrs = GilrsBuilder::new().build().ok()?;
        Some(Self { gilrs, dead_zone })
    }

    /// Current dead-zone value.
    #[inline]
    #[must_use]
    pub const fn dead_zone(&self) -> f32 {
        self.dead_zone
    }

    /// Drain all queued gilrs events into `out`. Axis values are passed
    /// through `apply_dead_zone` before emission. Connection / disconnect /
    /// dropped events are ignored at v0 — the gamepad set is implicit in
    /// whatever `GamepadId` shows up in axis / button events.
    pub fn poll(&mut self, out: &mut Vec<InputEvent>) {
        while let Some(evt) = self.gilrs.next_event() {
            // gilrs::GamepadId converts via `usize::from`. We squash to `u32`
            // because GamepadId carries Copy + Hash for HashMap keys; the
            // truncation is benign in practice (no platform reports more
            // than `u32::MAX` simultaneous pads).
            #[allow(clippy::cast_possible_truncation)]
            let id = GamepadId(usize::from(evt.id) as u32);
            match evt.event {
                EventType::ButtonPressed(btn, _) => {
                    if let Some(b) = button_from_gilrs(btn) {
                        out.push(InputEvent::GamepadButton(id, b, Pressed::Down));
                    }
                }
                EventType::ButtonReleased(btn, _) => {
                    if let Some(b) = button_from_gilrs(btn) {
                        out.push(InputEvent::GamepadButton(id, b, Pressed::Up));
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    if let Some(a) = axis_from_gilrs(axis) {
                        let v = apply_dead_zone(value, self.dead_zone);
                        out.push(InputEvent::GamepadAxis(id, a, v));
                    }
                }
                // Connected/Disconnected/ButtonChanged/Dropped/ButtonRepeated:
                // drop at v0. Connection state is implicit (events arrive
                // from a connected pad). Repeats are filtered upstream by
                // the consumer's `Input<GamepadButton>` set.
                _ => {}
            }
        }
    }
}

/// Apply a radial dead-zone clamp. Values within `[-dead_zone, dead_zone]`
/// snap to `0.0`. Outside the dead-zone the value passes through unchanged
/// (raw, not re-scaled — re-scaling is a downstream policy choice).
///
/// Public so tests in `tests/gamepad_dead_zone.rs` can call without
/// constructing a `GamepadPoller` (which fails in headless CI).
#[inline]
#[must_use]
pub fn apply_dead_zone(value: f32, dead_zone: f32) -> f32 {
    if value.abs() < dead_zone {
        0.0
    } else {
        value
    }
}

/// Translate `gilrs::Button` to our `GamepadButton`. Vendor-specific extras
/// (`Mode`, `Unknown`, paddle / share) drop to `None`.
fn button_from_gilrs(b: Button) -> Option<GamepadButton> {
    Some(match b {
        Button::South => GamepadButton::South,
        Button::East => GamepadButton::East,
        Button::North => GamepadButton::North,
        Button::West => GamepadButton::West,
        Button::LeftTrigger => GamepadButton::LeftShoulder,
        Button::RightTrigger => GamepadButton::RightShoulder,
        Button::LeftTrigger2 => GamepadButton::LeftTrigger,
        Button::RightTrigger2 => GamepadButton::RightTrigger,
        Button::Select => GamepadButton::Select,
        Button::Start => GamepadButton::Start,
        Button::LeftThumb => GamepadButton::LeftStick,
        Button::RightThumb => GamepadButton::RightStick,
        Button::DPadUp => GamepadButton::DPadUp,
        Button::DPadDown => GamepadButton::DPadDown,
        Button::DPadLeft => GamepadButton::DPadLeft,
        Button::DPadRight => GamepadButton::DPadRight,
        // C, Z, Mode, Unknown — outside the v0 vocabulary.
        _ => return None,
    })
}

/// Translate `gilrs::Axis` to our `GamepadAxis`. Unknown / vendor extras drop.
fn axis_from_gilrs(a: Axis) -> Option<GamepadAxis> {
    Some(match a {
        Axis::LeftStickX => GamepadAxis::LeftStickX,
        Axis::LeftStickY => GamepadAxis::LeftStickY,
        Axis::RightStickX => GamepadAxis::RightStickX,
        Axis::RightStickY => GamepadAxis::RightStickY,
        Axis::LeftZ => GamepadAxis::LeftTrigger,
        Axis::RightZ => GamepadAxis::RightTrigger,
        // DPadX/DPadY are surfaced as digital DPad* buttons; Unknown drops.
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn dead_zone_default_is_pointone() {
        assert!((DEFAULT_DEAD_ZONE - 0.1).abs() < 1e-6);
    }

    #[test]
    fn dead_zone_suppresses_below_threshold() {
        // `apply_dead_zone` returns *exact* `0.0` for in-band values —
        // exact comparison is the contract.
        assert_eq!(apply_dead_zone(0.05, DEFAULT_DEAD_ZONE), 0.0);
        assert_eq!(apply_dead_zone(-0.05, DEFAULT_DEAD_ZONE), 0.0);
        assert_eq!(apply_dead_zone(0.099_999, DEFAULT_DEAD_ZONE), 0.0);
    }

    #[test]
    fn dead_zone_passes_above_threshold() {
        assert_eq!(apply_dead_zone(0.5, DEFAULT_DEAD_ZONE), 0.5);
        assert_eq!(apply_dead_zone(-0.5, DEFAULT_DEAD_ZONE), -0.5);
        assert_eq!(apply_dead_zone(1.0, DEFAULT_DEAD_ZONE), 1.0);
    }
}
