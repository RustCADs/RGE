//! [`AudioSource`](crate::AudioSource) playback control.
//!
//! `source.rs` does not own audio data — that lives in Kira behind a
//! [`StaticSoundHandle`]. This module exposes:
//!
//! - [`PlaybackState`] — the gameplay-facing finite state for an audio source
//!   (`Playing` / `Paused` / `Stopped`). Stored on the component so that ECS
//!   diffing / replication can pick up changes without poking inside the Kira
//!   handle.
//!
//! - [`SourceState`] — engine-side bookkeeping the [`AudioManager`] keeps for
//!   each entity-bound source: the active Kira handles, the last applied
//!   playback state, and the cached emitter/sound IDs used by the schedule
//!   step to reconcile.
//!
//! [`AudioManager`]: crate::AudioManager
//! [`StaticSoundHandle`]: kira::sound::static_sound::StaticSoundHandle

use kira::sound::static_sound::StaticSoundHandle;
use kira::sound::PlaybackState as KiraPlaybackState;
use kira::track::SpatialTrackHandle;
use serde::{Deserialize, Serialize};

/// What the gameplay layer wants this source to do. The schedule step in
/// [`crate::schedule`] reads this value, compares against the engine-side
/// [`SourceState`], and dispatches Kira commands to make them agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlaybackState {
    /// Source is silent and the playhead is at zero (or at `loop_region`
    /// start if a region is configured).
    Stopped,
    /// Source is paused — playhead held at its current position.
    Paused,
    /// Source is producing samples.
    Playing,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self::Stopped
    }
}

impl PlaybackState {
    /// Translate a Kira-level state into our coarser ECS-facing state.
    ///
    /// Kira distinguishes more states (pause-fading, stop-fading) but the
    /// W12 schedule treats all of those as the closest "settled" state for
    /// gameplay purposes.
    #[must_use]
    pub fn from_kira(state: KiraPlaybackState) -> Self {
        match state {
            KiraPlaybackState::Playing | KiraPlaybackState::Resuming => Self::Playing,
            KiraPlaybackState::Paused
            | KiraPlaybackState::Pausing
            | KiraPlaybackState::WaitingToResume => Self::Paused,
            KiraPlaybackState::Stopped | KiraPlaybackState::Stopping => Self::Stopped,
        }
    }
}

/// Per-source bookkeeping owned by the [`AudioManager`](crate::AudioManager).
///
/// One [`SourceState`] exists per `(World, Entity)` whose ECS row carries an
/// [`AudioSource`](crate::AudioSource) component. The struct lives in the
/// manager (not on the component) because Kira handles (`StaticSoundHandle`,
/// `SpatialTrackHandle`) are not `Clone` and rely on Kira's per-manager
/// command channels — embedding them in components would propagate that
/// lifetime constraint into the ECS layer.
#[derive(Debug)]
pub struct SourceState {
    /// Kira sound handle. `None` between calls to `stop()` and the next
    /// `play()`.
    pub(crate) sound: Option<StaticSoundHandle>,
    /// Kira spatial sub-track associated with this source. Always present
    /// while the source is registered with the manager — track lifetimes are
    /// tied to the [`AudioSource`] component, not to individual sound
    /// instances. In Kira 0.12 each emitter is its own spatial sub-track that
    /// owns the position + attenuation curve and is the playback target for
    /// the per-source [`StaticSoundHandle`].
    pub(crate) track: SpatialTrackHandle,
    /// Last playback state we successfully applied through Kira. The
    /// schedule step compares this against the component's
    /// [`AudioSource::desired_state`](crate::AudioSource::desired_state)
    /// to decide what command to issue.
    pub(crate) last_applied: PlaybackState,
    /// Last applied volume — mirror so we can skip command issue if unchanged.
    pub(crate) last_volume: f32,
    /// Last applied pitch — same rationale.
    pub(crate) last_pitch: f32,
    /// Last applied loop flag — same rationale.
    pub(crate) last_looped: bool,
}

impl SourceState {
    /// Construct a freshly-registered source. The spatial track handle must
    /// already be live (returned from [`AudioManager::add_spatial_sub_track`](
    /// kira::AudioManager::add_spatial_sub_track)). `sound` is `None` because
    /// nothing is playing yet — the schedule step calls `track.play(...)` on
    /// first transition into `Playing`.
    #[must_use]
    pub(crate) fn new(track: SpatialTrackHandle) -> Self {
        Self {
            sound: None,
            track,
            last_applied: PlaybackState::Stopped,
            last_volume: 1.0,
            last_pitch: 1.0,
            last_looped: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PlaybackState round-trip through RON for snapshot/replication friendliness.
    #[test]
    fn playback_state_default_is_stopped() {
        assert_eq!(PlaybackState::default(), PlaybackState::Stopped);
    }

    /// from_kira covers every kira state without panic.
    #[test]
    fn from_kira_is_total() {
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Playing),
            PlaybackState::Playing
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Resuming),
            PlaybackState::Playing
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Pausing),
            PlaybackState::Paused
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Paused),
            PlaybackState::Paused
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::WaitingToResume),
            PlaybackState::Paused
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Stopping),
            PlaybackState::Stopped
        );
        assert_eq!(
            PlaybackState::from_kira(KiraPlaybackState::Stopped),
            PlaybackState::Stopped
        );
    }
}
