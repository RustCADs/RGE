//! `Viewport` — placeholder viewport widget for W03.
//!
//! Per W03 dispatch §6: "viewport widget skeleton (no rendering yet —
//! display 'Editing' or 'Playing' text overlay)". Real wgpu rendering
//! lives in W21+ (gfx wave); this is a *headless* widget that just
//! computes the overlay string the eventual gfx wave will draw.

use crate::time_scale::TimeScale;
use crate::PlayState;

/// Headless viewport widget. Tracks dimensions + the text overlay the
/// real renderer will eventually rasterize. W03 keeps this in the
/// editor-shell crate (rather than `editor-ui`) because the viewport
/// host is a lifecycle-owned construct (one-per-window in v1.0; per
/// PLAN.md §5.1: "Viewport (one, no multi-viewport yet)").
#[derive(Debug, Clone)]
pub struct Viewport {
    width: u32,
    height: u32,
    overlay: String,
}

impl Viewport {
    /// Construct with explicit initial dimensions. Defaults to a no-op
    /// "Editing" overlay — `EditorShell::redraw` calls
    /// [`Self::update_overlay`] each frame to refresh.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            overlay: PlayState::Editing.label().to_string(),
        }
    }

    /// Resize the viewport (called from winit `Resized` window event).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    /// Current width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Current height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Recompute the overlay string from current state. Format:
    ///
    /// - Editing: `"Editing"`
    /// - Playing: `"Playing  ×<scale>"`  (e.g. `Playing  ×1.00`)
    /// - Paused:  `"Paused (×<scale>)"`
    pub fn update_overlay(&mut self, state: PlayState, scale: TimeScale) {
        self.overlay = match state {
            PlayState::Editing => "Editing".to_string(),
            PlayState::Playing => format!("Playing  ×{:.2}", scale.value()),
            PlayState::Paused => format!("Paused (×{:.2})", scale.value()),
        };
    }

    /// Current overlay text (read by gfx wave / inspector / tests).
    #[must_use]
    pub fn overlay(&self) -> &str {
        &self.overlay
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(800, 600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_size_and_overlay() {
        let v = Viewport::default();
        assert_eq!(v.width(), 800);
        assert_eq!(v.height(), 600);
        assert_eq!(v.overlay(), "Editing");
    }

    #[test]
    fn resize_updates_dims() {
        let mut v = Viewport::new(100, 100);
        v.resize(1920, 1080);
        assert_eq!(v.width(), 1920);
        assert_eq!(v.height(), 1080);
    }

    #[test]
    fn overlay_updates_for_state() {
        let mut v = Viewport::default();
        let scale = TimeScale::with_value(0.5);

        v.update_overlay(PlayState::Editing, scale);
        assert_eq!(v.overlay(), "Editing");

        v.update_overlay(PlayState::Playing, scale);
        assert_eq!(v.overlay(), "Playing  ×0.50");

        v.update_overlay(PlayState::Paused, scale);
        assert_eq!(v.overlay(), "Paused (×0.50)");
    }
}
