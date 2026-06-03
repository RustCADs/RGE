//! `PlayState` — the PIE state machine.
//!
//! Per PLAN.md §6.13 + W03 dispatch: three states (Editing / Playing /
//! Paused) plus a step counter for `FrameStep` semantics. Transitions:
//!
//! ```text
//!                        Play
//!         Editing -------------------> Playing
//!            ^                            |  ^
//!     Stop  |                       Pause |  | Resume
//!            |                            v  |
//!            +-------- Stop ----------- Paused
//!                          (also: Step from Paused → Paused
//!                                 FrameStep from Paused → Paused)
//! ```
//!
//! Step / `FrameStep` are *intra-state* operations: they advance simulation
//! by one tick / frame respectively without changing `PlayState`. The
//! distinction:
//!
//! - **Step** advances one *simulation tick* (game systems run once with
//!   the configured time-scaled `dt`).
//! - **`FrameStep`** advances one *render frame* (one redraw; game tick may
//!   or may not happen depending on whether tick accumulator crossed
//!   threshold). For W03 the two collapse to the same thing — schedule
//!   accumulator lives in W04+ — but the API is shaped to let them
//!   diverge later without a callsite refactor.
//!
//! Constitutional principle #8 (editor extends runtime, never replaces):
//! the state machine is the single source of truth for "are game systems
//! running this tick?" — every system queries [`PlayState::game_systems_run`]
//! rather than maintaining its own flag.

use std::fmt;

/// Three editor lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayState {
    /// Editor authoring; game systems frozen; editor systems run.
    #[default]
    Editing,
    /// Play-in-Editor active; both editor and game systems run; game
    /// systems advance per scaled `dt`.
    Playing,
    /// PIE active but ticks suppressed; editor systems run; game systems
    /// frozen unless explicitly stepped.
    Paused,
}

impl PlayState {
    /// Stable single-word label, primarily for the placeholder viewport
    /// overlay and audit-log records.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Editing => "Editing",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
        }
    }

    /// Whether *game* systems advance their schedule this tick.
    /// Editor systems always run regardless of `PlayState` (constitutional
    /// principle: the editor never freezes).
    #[must_use]
    pub const fn game_systems_run(self) -> bool {
        matches!(self, Self::Playing)
    }

    /// Whether the world is in a "PIE active" state (Playing or Paused).
    /// Used by `EditorShell` to decide whether `Stop` should restore the
    /// snapshot or no-op.
    #[must_use]
    pub const fn is_pie_active(self) -> bool {
        matches!(self, Self::Playing | Self::Paused)
    }

    /// Whether a `Play` press is valid from this state — `Editing` (start)
    /// or `Paused` (resume); false only while already `Playing`. The Play
    /// menu item greys out when this is false. The rule's authority is
    /// [`Self::play`]; this is the query form, pinned equal to it by
    /// `can_methods_match_transition_results`.
    #[must_use]
    pub const fn can_play(self) -> bool {
        !matches!(self, Self::Playing)
    }

    /// Whether a `Pause` press is valid — only while PIE is active
    /// (`Playing`, or `Paused` idempotently). Authority: [`Self::pause`].
    #[must_use]
    pub const fn can_pause(self) -> bool {
        self.is_pie_active()
    }

    /// Whether a `Stop` press is valid — only while PIE is active (so there
    /// is a snapshot to restore). Authority: [`Self::stop`].
    #[must_use]
    pub const fn can_stop(self) -> bool {
        self.is_pie_active()
    }

    /// Whether a `Step` press is valid — only from `Paused`. Authority:
    /// [`Self::step`].
    #[must_use]
    pub const fn can_step(self) -> bool {
        matches!(self, Self::Paused)
    }
}

impl fmt::Display for PlayState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Reasons a transition can be rejected. Errors are *recoverable* — bad
/// transitions are silently ignored at the toolbar callsite (no panic),
/// but the error type lets tests assert that e.g. `[Play]` from `Playing`
/// is a no-op rather than a double-snapshot bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStateError {
    /// Tried to start play while already in PIE.
    AlreadyPlaying,
    /// Tried to pause from Editing (no PIE in flight).
    NotInPie,
    /// Tried to stop from Editing (no snapshot to restore).
    NoSnapshot,
    /// Tried to resume from Playing (or Editing) — only valid from Paused.
    NotPaused,
    /// Tried to step from a state that doesn't allow stepping.
    StepNotAllowed,
}

impl fmt::Display for PlayStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::AlreadyPlaying => "already in PIE",
            Self::NotInPie => "not in PIE; nothing to pause",
            Self::NoSnapshot => "no snapshot to restore (already in Editing)",
            Self::NotPaused => "must be Paused to resume",
            Self::StepNotAllowed => "step is only valid from Paused",
        };
        f.write_str(s)
    }
}

impl std::error::Error for PlayStateError {}

/// Outcome of a transition request. Encodes "what should the caller do"
/// rather than "did it succeed" — the lifecycle code uses this to decide
/// whether to capture a snapshot, restore one, or no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStateTransition {
    /// Editing → Playing. Caller MUST capture a `WorldSnapshot` *before*
    /// invoking this transition's side-effect (this enum value is the
    /// signal that capture is required).
    StartedPlay,
    /// Playing → Paused. No snapshot work; just freeze game-tick driver.
    Paused,
    /// Paused → Playing.
    Resumed,
    /// Playing|Paused → Editing. Caller MUST restore the captured snapshot.
    StoppedAndRestored,
    /// Step request acknowledged in Paused state. Caller advances one
    /// game tick.
    Stepped,
    /// `FrameStep` request acknowledged. Caller renders one frame; game
    /// tick may or may not advance (caller decides per accumulator).
    FrameStepped,
}

impl PlayState {
    /// Attempt `Play` press from this state.
    ///
    /// # Errors
    /// Returns `Err(AlreadyPlaying)` if already in `Playing` or `Paused`.
    pub fn play(&mut self) -> Result<PlayStateTransition, PlayStateError> {
        match *self {
            Self::Editing => {
                *self = Self::Playing;
                Ok(PlayStateTransition::StartedPlay)
            }
            Self::Paused => {
                // Equivalent to Resume; toolbar's `[Play]` button should
                // double as Resume per common editor UX.
                *self = Self::Playing;
                Ok(PlayStateTransition::Resumed)
            }
            Self::Playing => Err(PlayStateError::AlreadyPlaying),
        }
    }

    /// Attempt `Pause` press.
    ///
    /// # Errors
    /// Returns `Err(NotInPie)` if in `Editing`.
    pub fn pause(&mut self) -> Result<PlayStateTransition, PlayStateError> {
        match *self {
            Self::Playing => {
                *self = Self::Paused;
                Ok(PlayStateTransition::Paused)
            }
            Self::Paused => Ok(PlayStateTransition::Paused), // idempotent
            Self::Editing => Err(PlayStateError::NotInPie),
        }
    }

    /// Attempt `Stop` press.
    ///
    /// # Errors
    /// Returns `Err(NoSnapshot)` if in `Editing` (nothing to restore).
    pub fn stop(&mut self) -> Result<PlayStateTransition, PlayStateError> {
        match *self {
            Self::Playing | Self::Paused => {
                *self = Self::Editing;
                Ok(PlayStateTransition::StoppedAndRestored)
            }
            Self::Editing => Err(PlayStateError::NoSnapshot),
        }
    }

    /// Attempt `Step` press (advance one game tick from Paused).
    ///
    /// # Errors
    /// Returns `Err(StepNotAllowed)` if not currently `Paused`.
    pub fn step(self) -> Result<PlayStateTransition, PlayStateError> {
        match self {
            Self::Paused => Ok(PlayStateTransition::Stepped),
            _ => Err(PlayStateError::StepNotAllowed),
        }
    }

    /// Attempt `FrameStep` press (advance one render frame).
    ///
    /// # Errors
    /// Same as [`Self::step`].
    pub fn frame_step(self) -> Result<PlayStateTransition, PlayStateError> {
        match self {
            Self::Paused => Ok(PlayStateTransition::FrameStepped),
            _ => Err(PlayStateError::StepNotAllowed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_methods_match_transition_results() {
        // The `can_*` queries are the Play-menu enablement authority; pin each
        // to whether the canonical transition actually succeeds, so they cannot
        // drift from `play` / `pause` / `stop` / `step`.
        for state in [PlayState::Editing, PlayState::Playing, PlayState::Paused] {
            let mut s = state;
            assert_eq!(
                state.can_play(),
                s.play().is_ok(),
                "can_play disagrees with play() in {state:?}"
            );
            let mut s = state;
            assert_eq!(
                state.can_pause(),
                s.pause().is_ok(),
                "can_pause disagrees with pause() in {state:?}"
            );
            let mut s = state;
            assert_eq!(
                state.can_stop(),
                s.stop().is_ok(),
                "can_stop disagrees with stop() in {state:?}"
            );
            assert_eq!(
                state.can_step(),
                state.step().is_ok(),
                "can_step disagrees with step() in {state:?}"
            );
        }
    }

    #[test]
    fn default_is_editing() {
        let p = PlayState::default();
        assert_eq!(p, PlayState::Editing);
        assert!(!p.game_systems_run());
        assert!(!p.is_pie_active());
    }

    #[test]
    fn play_from_editing_starts_play() {
        let mut p = PlayState::Editing;
        let t = p.play().unwrap();
        assert_eq!(t, PlayStateTransition::StartedPlay);
        assert_eq!(p, PlayState::Playing);
        assert!(p.game_systems_run());
    }

    #[test]
    fn play_from_playing_errors() {
        let mut p = PlayState::Playing;
        assert_eq!(p.play(), Err(PlayStateError::AlreadyPlaying));
    }

    #[test]
    fn play_from_paused_resumes() {
        let mut p = PlayState::Paused;
        let t = p.play().unwrap();
        assert_eq!(t, PlayStateTransition::Resumed);
        assert_eq!(p, PlayState::Playing);
    }

    #[test]
    fn pause_from_playing() {
        let mut p = PlayState::Playing;
        p.pause().unwrap();
        assert_eq!(p, PlayState::Paused);
        assert!(!p.game_systems_run());
    }

    #[test]
    fn pause_from_editing_errors() {
        let mut p = PlayState::Editing;
        assert_eq!(p.pause(), Err(PlayStateError::NotInPie));
    }

    #[test]
    fn stop_from_playing_restores() {
        let mut p = PlayState::Playing;
        let t = p.stop().unwrap();
        assert_eq!(t, PlayStateTransition::StoppedAndRestored);
        assert_eq!(p, PlayState::Editing);
    }

    #[test]
    fn stop_from_editing_errors() {
        let mut p = PlayState::Editing;
        assert_eq!(p.stop(), Err(PlayStateError::NoSnapshot));
    }

    #[test]
    fn step_only_valid_from_paused() {
        assert!(PlayState::Editing.step().is_err());
        assert!(PlayState::Playing.step().is_err());
        assert_eq!(
            PlayState::Paused.step().unwrap(),
            PlayStateTransition::Stepped
        );
    }

    #[test]
    fn frame_step_only_valid_from_paused() {
        assert!(PlayState::Editing.frame_step().is_err());
        assert_eq!(
            PlayState::Paused.frame_step().unwrap(),
            PlayStateTransition::FrameStepped
        );
    }

    #[test]
    fn label_is_stable() {
        assert_eq!(PlayState::Editing.label(), "Editing");
        assert_eq!(PlayState::Playing.label(), "Playing");
        assert_eq!(PlayState::Paused.label(), "Paused");
    }
}
