//! `PlayToolbar` — registration shape for the play-mode toolbar buttons.
//!
//! Per W03 dispatch: register Play / Pause / Stop / Step / `FrameStep`
//! buttons in the `editor.play_mode.toolbar` extension point. The real
//! menu/toolbar registry lives in `editor-ui/menus` (W08); when W08 lands,
//! this module's [`PlayToolbar::register_into`] will hand its
//! [`ToolbarButton`] entries to the registry. Until W08, the registration
//! is a **stub** — buttons are stored in this struct and the
//! `EditorShell` lifecycle queries them directly.
//!
//! Per PLAN.md §6.3 + §6.8 (UE → RGE crate map): `UToolMenus` becomes
//! `MenuRegistry`. Extension-point IDs follow the dotted-path convention
//! seen in rustforge's menu wiring (`editor.file.menu`,
//! `editor.play_mode.toolbar`, etc.).

use std::fmt;

/// Stable ID for one of the five PIE toolbar buttons. Closed enum because
/// these five are constitutionally fixed by PLAN.md §6.13; new buttons
/// would require an ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolbarButtonId {
    /// Toggle to Playing (also acts as Resume from Paused).
    Play,
    /// Toggle to Paused.
    Pause,
    /// Restore snapshot, return to Editing.
    Stop,
    /// Advance one game tick (only valid in Paused).
    Step,
    /// Advance one render frame (only valid in Paused).
    FrameStep,
}

impl ToolbarButtonId {
    /// Stable string ID for the menu/toolbar registry. Matches the dotted
    /// convention (`editor.play_mode.toolbar.<button>`).
    #[must_use]
    pub const fn registry_id(self) -> &'static str {
        match self {
            Self::Play => "editor.play_mode.toolbar.play",
            Self::Pause => "editor.play_mode.toolbar.pause",
            Self::Stop => "editor.play_mode.toolbar.stop",
            Self::Step => "editor.play_mode.toolbar.step",
            Self::FrameStep => "editor.play_mode.toolbar.frame_step",
        }
    }

    /// Default human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Step => "Step",
            Self::FrameStep => "Frame Step",
        }
    }

    /// Default keyboard shortcut hint string. Real keymap binds happen in
    /// `editor-ui/menus` (W08); these are advisory until then.
    #[must_use]
    pub const fn default_shortcut_hint(self) -> &'static str {
        match self {
            Self::Play => "F5",
            Self::Pause => "Shift+F5",
            Self::Stop => "Esc",
            Self::Step => "F10",
            Self::FrameStep => "F11",
        }
    }

    /// Iterate all five button IDs in stable order.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Play,
            Self::Pause,
            Self::Stop,
            Self::Step,
            Self::FrameStep,
        ]
    }
}

impl fmt::Display for ToolbarButtonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One toolbar-button entry. Carries the registry shape that
/// `editor-ui/menus` will consume (W08+). The [`enabled_in_state`]
/// closure is a `fn` pointer rather than a `Box<dyn Fn>` to keep the
/// type Copy + Send + Sync without arena allocation; PIE button
/// enablement is a pure function of `PlayState` so the closure shape
/// is right.
#[derive(Clone, Copy)]
pub struct ToolbarButton {
    /// Stable button identifier used by the registry + audit log.
    pub id: ToolbarButtonId,
    /// Display label rendered into the toolbar widget.
    pub label: &'static str,
    /// Hint string shown next to the label (advisory; real binds live in
    /// `editor-ui/menus`).
    pub shortcut_hint: &'static str,
    /// Predicate: is the button clickable in this `PlayState`? Returns
    /// `true` if the button should be rendered enabled. Maps directly to
    /// the transition rules in [`crate::play_state`].
    pub enabled_in_state: fn(crate::PlayState) -> bool,
}

impl fmt::Debug for ToolbarButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolbarButton")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("shortcut_hint", &self.shortcut_hint)
            .field("enabled_in_state", &"<fn>")
            .finish()
    }
}

/// Registration container. Holds the five buttons in stable order; the
/// `EditorShell` builds one of these at startup and the lifecycle code
/// consults it on every redraw to compute current button enablement.
#[derive(Debug, Clone)]
pub struct PlayToolbar {
    buttons: [ToolbarButton; 5],
    /// The extension point this toolbar registers into. Per PLAN.md §6.3.
    extension_point: &'static str,
}

impl PlayToolbar {
    /// The canonical extension-point ID (PLAN.md §6.3 / W03 dispatch).
    pub const EXTENSION_POINT: &'static str = "editor.play_mode.toolbar";

    /// Build the standard PIE toolbar with the five constitutional buttons.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            buttons: [
                ToolbarButton {
                    id: ToolbarButtonId::Play,
                    label: ToolbarButtonId::Play.label(),
                    shortcut_hint: ToolbarButtonId::Play.default_shortcut_hint(),
                    // Play is enabled in Editing (start) and Paused (resume).
                    enabled_in_state: |s| !matches!(s, crate::PlayState::Playing),
                },
                ToolbarButton {
                    id: ToolbarButtonId::Pause,
                    label: ToolbarButtonId::Pause.label(),
                    shortcut_hint: ToolbarButtonId::Pause.default_shortcut_hint(),
                    enabled_in_state: |s| matches!(s, crate::PlayState::Playing),
                },
                ToolbarButton {
                    id: ToolbarButtonId::Stop,
                    label: ToolbarButtonId::Stop.label(),
                    shortcut_hint: ToolbarButtonId::Stop.default_shortcut_hint(),
                    enabled_in_state: |s| s.is_pie_active(),
                },
                ToolbarButton {
                    id: ToolbarButtonId::Step,
                    label: ToolbarButtonId::Step.label(),
                    shortcut_hint: ToolbarButtonId::Step.default_shortcut_hint(),
                    enabled_in_state: |s| matches!(s, crate::PlayState::Paused),
                },
                ToolbarButton {
                    id: ToolbarButtonId::FrameStep,
                    label: ToolbarButtonId::FrameStep.label(),
                    shortcut_hint: ToolbarButtonId::FrameStep.default_shortcut_hint(),
                    enabled_in_state: |s| matches!(s, crate::PlayState::Paused),
                },
            ],
            extension_point: Self::EXTENSION_POINT,
        }
    }

    /// Iterate the five registered buttons in stable order.
    pub fn buttons(&self) -> impl Iterator<Item = &ToolbarButton> + '_ {
        self.buttons.iter()
    }

    /// Look up a button by ID.
    #[must_use]
    pub fn button(&self, id: ToolbarButtonId) -> &ToolbarButton {
        &self.buttons[match id {
            ToolbarButtonId::Play => 0,
            ToolbarButtonId::Pause => 1,
            ToolbarButtonId::Stop => 2,
            ToolbarButtonId::Step => 3,
            ToolbarButtonId::FrameStep => 4,
        }]
    }

    /// Extension point ID this toolbar registers into.
    #[must_use]
    pub fn extension_point(&self) -> &'static str {
        self.extension_point
    }

    /// True if the button identified by `id` is currently enabled given
    /// `state`.
    #[must_use]
    pub fn is_enabled(&self, id: ToolbarButtonId, state: crate::PlayState) -> bool {
        (self.button(id).enabled_in_state)(state)
    }

    /// Stub for the W08 menu-registry handoff: when the registry exists,
    /// this method will iterate `self.buttons` and call
    /// `registry.register(extension_point, button)`. Until then it logs the
    /// intended registration via `tracing` and stores the count.
    ///
    /// Returns the number of buttons that *would have been* registered
    /// (always 5; provided for the W08 integration test to stub-verify).
    pub fn register_into_stub(&self) -> usize {
        for b in &self.buttons {
            tracing::debug!(
                target: "rge::editor-shell::toolbar",
                ext_point = self.extension_point,
                id = b.id.registry_id(),
                shortcut = b.shortcut_hint,
                "would register PIE toolbar button"
            );
        }
        self.buttons.len()
    }
}

impl Default for PlayToolbar {
    fn default() -> Self {
        Self::standard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayState;

    #[test]
    fn registers_five_buttons() {
        let bar = PlayToolbar::standard();
        assert_eq!(bar.buttons().count(), 5);
        assert_eq!(bar.register_into_stub(), 5);
    }

    #[test]
    fn extension_point_is_stable() {
        let bar = PlayToolbar::default();
        assert_eq!(bar.extension_point(), "editor.play_mode.toolbar");
    }

    #[test]
    fn play_enabled_in_editing_and_paused_only() {
        let bar = PlayToolbar::standard();
        assert!(bar.is_enabled(ToolbarButtonId::Play, PlayState::Editing));
        assert!(!bar.is_enabled(ToolbarButtonId::Play, PlayState::Playing));
        assert!(bar.is_enabled(ToolbarButtonId::Play, PlayState::Paused));
    }

    #[test]
    fn pause_enabled_only_when_playing() {
        let bar = PlayToolbar::standard();
        assert!(!bar.is_enabled(ToolbarButtonId::Pause, PlayState::Editing));
        assert!(bar.is_enabled(ToolbarButtonId::Pause, PlayState::Playing));
        assert!(!bar.is_enabled(ToolbarButtonId::Pause, PlayState::Paused));
    }

    #[test]
    fn stop_enabled_only_in_pie() {
        let bar = PlayToolbar::standard();
        assert!(!bar.is_enabled(ToolbarButtonId::Stop, PlayState::Editing));
        assert!(bar.is_enabled(ToolbarButtonId::Stop, PlayState::Playing));
        assert!(bar.is_enabled(ToolbarButtonId::Stop, PlayState::Paused));
    }

    #[test]
    fn step_and_frame_step_only_in_paused() {
        let bar = PlayToolbar::standard();
        for id in [ToolbarButtonId::Step, ToolbarButtonId::FrameStep] {
            assert!(!bar.is_enabled(id, PlayState::Editing));
            assert!(!bar.is_enabled(id, PlayState::Playing));
            assert!(bar.is_enabled(id, PlayState::Paused));
        }
    }

    #[test]
    fn registry_ids_unique() {
        let mut ids: Vec<&str> = ToolbarButtonId::all()
            .iter()
            .map(|b| b.registry_id())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }
}
