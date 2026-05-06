// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Stylus fan-in: pressure / tilt extraction from winit pointer-bearing
//! events. v0 surfaces only the pressure channel (`InputEvent::StylusPressure`)
//! — tilt and twist are tracked in the Phase 5+ spec for `input-gestures`
//! when stylus-aware tooling lands.
//!
//! winit 0.30 exposes pen pressure differently per backend:
//! - On Windows / iPadOS / Android the pressure rides on `Touch::force`
//!   when the touch device reports as a stylus.
//! - On Wayland / X11 it arrives via dedicated tablet protocols not yet
//!   surfaced in winit's stable API.
//!
//! The v0 fan-in API is `translate_force` — given a winit `Force` value,
//! return `InputEvent::StylusPressure(f32)` normalized to `[0.0, 1.0]`.
//! Callers gate this behind a "this contact is from a stylus" check; the
//! input crate itself does not classify pen-vs-finger (that's a gesture
//! concern owned by `input-gestures`).

use winit::event::Force;

use crate::event::InputEvent;

/// Translate a winit `Force` to a normalized stylus pressure event.
///
/// `Force::Calibrated` is divided by `max_possible_force` to project into
/// `[0.0, 1.0]`. `Force::Normalized` is already `[0.0, 1.0]`. Result is
/// clamped — some backends report transient over-range during stylus
/// flicks, and consumers (UI brushes, CAD pressure-modulated stroke width)
/// expect a strict unit interval.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn translate_force(force: Force) -> InputEvent {
    let raw = match force {
        Force::Calibrated {
            force,
            max_possible_force,
            ..
        } => {
            if max_possible_force > 0.0 {
                (force / max_possible_force) as f32
            } else {
                0.0
            }
        }
        Force::Normalized(p) => p as f32,
    };
    InputEvent::StylusPressure(raw.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_passes_through() {
        let evt = translate_force(Force::Normalized(0.5));
        assert!(matches!(evt, InputEvent::StylusPressure(p) if (p - 0.5).abs() < 1e-6));
    }

    #[test]
    fn calibrated_divides_by_max() {
        let evt = translate_force(Force::Calibrated {
            force: 5.0,
            max_possible_force: 10.0,
            altitude_angle: None,
        });
        assert!(matches!(evt, InputEvent::StylusPressure(p) if (p - 0.5).abs() < 1e-6));
    }

    #[test]
    fn out_of_range_clamps() {
        let evt = translate_force(Force::Normalized(1.5));
        assert!(matches!(evt, InputEvent::StylusPressure(p) if (p - 1.0).abs() < 1e-6));
        let evt = translate_force(Force::Normalized(-0.2));
        assert!(matches!(evt, InputEvent::StylusPressure(p) if p.abs() < 1e-6));
    }

    #[test]
    fn calibrated_zero_max_safe() {
        // Defensive: never divide by zero even if a backend lies.
        let evt = translate_force(Force::Calibrated {
            force: 5.0,
            max_possible_force: 0.0,
            altitude_angle: None,
        });
        assert!(matches!(evt, InputEvent::StylusPressure(p) if p.abs() < 1e-6));
    }
}
