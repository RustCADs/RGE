// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Mouse fan-in: winit `WindowEvent::CursorMoved` / `MouseInput` /
//! `MouseWheel` → `InputEvent::MouseMove` / `MouseButton` / `Scroll`.
//!
//! Cursor positions are forwarded as logical pixels (winit's
//! `PhysicalPosition<f64>` cast to `f32`). Scroll deltas distinguish
//! line-vs-pixel mode so high-DPI trackpads don't lose precision when a
//! caller wants smooth pan vs notched zoom.

use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta};

use crate::event::{InputEvent, Pressed};

/// Mouse button identifier. Surface mirrors `winit::event::MouseButton` but
/// drops `Other(u16)` — vendor-specific buttons are not part of the v0
/// vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Primary (left) button.
    Left,
    /// Secondary (right) button.
    Right,
    /// Middle button (typically scroll-wheel click).
    Middle,
    /// Browser-back / side button 4.
    Back,
    /// Browser-forward / side button 5.
    Forward,
}

/// Scroll delta. Pixel-mode is what high-precision trackpads and most
/// modern mice emit; line-mode is the legacy notched-wheel signal. Carrying
/// both means consumers can pick precision (zoom) or coarse step (item list)
/// per call-site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    /// Logical pixels scrolled in (x, y).
    Pixels([f32; 2]),
    /// Lines scrolled in (x, y). Sign convention: positive y = scroll up.
    Lines([f32; 2]),
}

/// Translate a winit cursor-moved position into `InputEvent::MouseMove`.
///
/// winit reports `f64` for cursor coordinates; we narrow to `f32` because
/// downstream consumers (egui, viewport gizmos) work in `f32` viewport
/// space anyway and the precision loss is sub-pixel for any conceivable
/// monitor resolution.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn translate_cursor_moved(pos: PhysicalPosition<f64>) -> InputEvent {
    InputEvent::MouseMove([pos.x as f32, pos.y as f32])
}

/// Translate a winit mouse-button event. Returns `None` for buttons outside
/// the v0 surface (vendor-specific extras).
#[must_use]
pub fn translate_mouse_button(button: WinitMouseButton, state: ElementState) -> Option<InputEvent> {
    let b = match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Back,
        WinitMouseButton::Forward => MouseButton::Forward,
        WinitMouseButton::Other(_) => return None,
    };
    let p = match state {
        ElementState::Pressed => Pressed::Down,
        ElementState::Released => Pressed::Up,
    };
    Some(InputEvent::MouseButton(b, p))
}

/// Translate a winit scroll-wheel event.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn translate_mouse_wheel(delta: MouseScrollDelta) -> InputEvent {
    let scroll = match delta {
        MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines([x, y]),
        MouseScrollDelta::PixelDelta(p) => ScrollDelta::Pixels([p.x as f32, p.y as f32]),
    };
    InputEvent::Scroll(scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_moved_to_f32() {
        let evt = translate_cursor_moved(PhysicalPosition::new(123.5, 456.25));
        assert!(
            matches!(evt, InputEvent::MouseMove([x, y]) if (x - 123.5).abs() < 1e-3 && (y - 456.25).abs() < 1e-3)
        );
    }

    #[test]
    fn mouse_other_button_drops() {
        // Vendor-specific extras outside the v0 surface drop silently.
        let evt = translate_mouse_button(WinitMouseButton::Other(9), ElementState::Pressed);
        assert!(evt.is_none());
    }

    #[test]
    fn scroll_lines_vs_pixels_distinct() {
        let l = translate_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
        let p = translate_mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
            0.0, 100.0,
        )));
        assert!(matches!(l, InputEvent::Scroll(ScrollDelta::Lines(_))));
        assert!(matches!(p, InputEvent::Scroll(ScrollDelta::Pixels(_))));
    }
}
