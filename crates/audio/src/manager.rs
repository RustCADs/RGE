//! ECS resource that wraps a [`kira::AudioManager`].
//!
//! One [`AudioManager`] per ECS world. Holds:
//! - the underlying Kira manager (driving cpal in production, `MockBackend`
//!   in tests),
//! - a single internal "anchor" listener that every spatial sub-track binds
//!   to (multi-listener / split-screen is a post-W12 reach item),
//! - a per-entity registry that maps ECS entities to engine-side state
//!   ([`SourceState`](crate::source::SourceState),
//!   [`ListenerState`](crate::listener::ListenerState)),
//! - a registry of pre-loaded sound clips keyed by the same `String` keys
//!   used in [`AudioSource::clip`](crate::AudioSource::clip).
//!
//! Kira 0.12 replaced the old `SpatialScene` model — listeners are now created
//! directly on the [`kira::AudioManager`], and emitters are spatial sub-tracks
//! ([`SpatialTrackHandle`]) that own their own position + attenuation curve
//! and accept sounds via [`SpatialTrackHandle::play`].
//!
//! The manager is intentionally not `Send`/`Sync` constrained at this layer
//! — Kira's own `AudioManager` is `Send` but not `Sync`, and the W12 schedule
//! (a single ECS system) consumes it `&mut`. Multi-threaded audio dispatch is
//! a post-W12 reach item.

use std::collections::HashMap;
use std::sync::Arc;

use kira::backend::{Backend, DefaultBackend};
use kira::listener::ListenerHandle;
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::track::SpatialTrackBuilder;
use kira::{AudioManager as KiraManager, AudioManagerSettings, Frame};
// thiserror is used by every other crate in the workspace already; we vendor
// the `Error` derive locally because adding it to the audio crate's deps was
// part of the same wave that introduced the schedule. Re-export so tests can
// match on it.
pub use thiserror;
use thiserror::Error;

use crate::components::{AudioSource, Entity, Transform};
use crate::listener::ListenerState;
use crate::source::SourceState;

/// Construction / playback errors surfaced through the [`AudioManager`].
#[derive(Debug, Error)]
pub enum ManagerError {
    /// Kira failed to start its backend. With `MockBackend` this is
    /// effectively unreachable; with `DefaultBackend` (cpal) it can fail when
    /// no audio device is present.
    #[error("kira backend failed to start: {0}")]
    Backend(String),

    /// One of Kira's resource pools (sounds, sub-tracks, listeners) is full.
    #[error("kira resource pool exhausted: {0}")]
    ResourceLimit(String),

    /// `play()` was called for an entity that has no registered
    /// [`AudioSource`](crate::AudioSource).
    #[error("no AudioSource registered for entity {0:?}")]
    UnknownSource(Entity),

    /// The clip referenced by an [`AudioSource::clip`](crate::AudioSource::clip)
    /// has not been pre-loaded with [`AudioManager::register_clip`].
    #[error("unknown clip: {0}")]
    UnknownClip(String),

    /// Kira refused to dispatch a sound — usually because the underlying
    /// sound resource pool is full.
    #[error("kira play() failed: {0}")]
    Play(String),
}

/// World-level ECS resource: the audio engine.
///
/// Generic over `B: Backend` so that tests can drive the manager with
/// [`kira::backend::mock::MockBackend`] without instantiating an actual audio
/// device. Production code uses [`AudioManager::default`] which resolves to
/// [`DefaultBackend`] (cpal).
pub struct AudioManager<B: Backend = DefaultBackend> {
    inner: KiraManager<B>,
    /// Always-on listener that every spatial sub-track is bound to. Multi-
    /// listener support (split-screen) is a post-W12 reach item — for now,
    /// any per-entity [`register_listener`](Self::register_listener) call
    /// updates this listener's pose via the per-entity [`ListenerState`].
    anchor_listener: ListenerHandle,
    sources: HashMap<Entity, SourceState>,
    listeners: HashMap<Entity, ListenerState>,
    clips: HashMap<String, StaticSoundData>,
}

// Hand-rolled because `KiraManager`, `ListenerHandle` and the per-entity
// Kira handles aren't `Debug` in a way that fits our diagnostic spans. We
// project to summary counts which are what diagnostic spans actually want.
#[allow(
    clippy::missing_fields_in_debug,
    reason = "KiraManager / ListenerHandle / per-entity Kira handles aren't useful in Debug spans; we project to summary counts which is what diagnostic spans actually want"
)]
impl<B: Backend> std::fmt::Debug for AudioManager<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioManager")
            .field("source_count", &self.sources.len())
            .field("listener_count", &self.listeners.len())
            .field("clip_count", &self.clips.len())
            .finish_non_exhaustive()
    }
}

impl AudioManager<DefaultBackend> {
    /// Default-backend constructor (cpal in non-wasm builds).
    ///
    /// # Errors
    ///
    /// Returns [`ManagerError::Backend`] when the host has no audio device.
    pub fn new() -> Result<Self, ManagerError> {
        Self::with_settings(AudioManagerSettings::default())
    }
}

impl<B: Backend> AudioManager<B>
where
    B::Error: std::fmt::Debug,
{
    /// Backend-generic constructor — used by tests to thread
    /// `MockBackendSettings` through.
    ///
    /// # Errors
    ///
    /// Returns [`ManagerError::Backend`] if Kira fails to set up the backend.
    pub fn with_settings(settings: AudioManagerSettings<B>) -> Result<Self, ManagerError> {
        let mut inner = KiraManager::<B>::new(settings)
            .map_err(|err| ManagerError::Backend(format!("{err:?}")))?;
        // Anchor listener at the world origin facing -Z. Subsequent
        // register_listener calls reposition it through ListenerState.
        let anchor_listener = inner
            .add_listener(
                mint::Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                mint::Quaternion {
                    v: mint::Vector3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    s: 1.0,
                },
            )
            .map_err(|err| ManagerError::ResourceLimit(format!("anchor listener: {err}")))?;
        Ok(Self {
            inner,
            anchor_listener,
            sources: HashMap::new(),
            listeners: HashMap::new(),
            clips: HashMap::new(),
        })
    }

    /// Pre-load a sound clip under the key referenced by
    /// [`AudioSource::clip`](crate::AudioSource::clip).
    ///
    /// Cheap to call repeatedly — `StaticSoundData` is internally `Arc`'d and
    /// re-uses underlying sample storage on clone.
    pub fn register_clip(&mut self, key: impl Into<String>, data: StaticSoundData) {
        self.clips.insert(key.into(), data);
    }

    /// Pre-load a clip from raw `f32` mono samples at `sample_rate`.
    ///
    /// This is the testing path used by the W12 sine-wave fixture.
    /// Production code typically goes through `register_clip` after the
    /// asset pipeline (W17/W18) has decoded an OGG/WAV/FLAC asset.
    pub fn register_clip_from_samples(
        &mut self,
        key: impl Into<String>,
        sample_rate: u32,
        samples: &[f32],
    ) {
        let frames: Arc<[Frame]> = samples
            .iter()
            .map(|&s| Frame::from_mono(s))
            .collect::<Vec<_>>()
            .into();
        let data = StaticSoundData {
            sample_rate,
            frames,
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        self.register_clip(key, data);
    }

    /// Look up a clip — used by tests to verify registration round-trips.
    #[must_use]
    pub fn clip(&self, key: &str) -> Option<&StaticSoundData> {
        self.clips.get(key)
    }

    /// Register a spatial sub-track for the given `entity` with bounds derived
    /// from `source.distances`. Idempotent — calling twice replaces the prior
    /// track.
    ///
    /// # Errors
    ///
    /// [`ManagerError::ResourceLimit`] when Kira's sub-track pool is full.
    pub fn register_source(
        &mut self,
        entity: Entity,
        transform: &Transform,
        source: &AudioSource,
    ) -> Result<(), ManagerError> {
        let position = mint::Vector3 {
            x: transform.position[0],
            y: transform.position[1],
            z: transform.position[2],
        };
        let builder = SpatialTrackBuilder::new()
            .distances(source.distances)
            .attenuation_function(Some(source.falloff.to_kira_easing()));
        let handle = self
            .inner
            .add_spatial_sub_track(&self.anchor_listener, position, builder)
            .map_err(|err| ManagerError::ResourceLimit(format!("spatial sub-track: {err}")))?;
        self.sources.insert(entity, SourceState::new(handle));
        Ok(())
    }

    /// Drop an entity's spatial track — called when the [`AudioSource`]
    /// component is removed from the ECS.
    pub fn unregister_source(&mut self, entity: Entity) -> bool {
        self.sources.remove(&entity).is_some()
    }

    /// Register a listener at the given pose. Idempotent.
    ///
    /// In Kira 0.12 a single anchor listener is shared across the whole
    /// manager (created in [`Self::with_settings`]). Calling this method
    /// records a per-entity [`ListenerState`] which the schedule step uses to
    /// re-pose the anchor listener each tick.
    ///
    /// # Errors
    ///
    /// Currently infallible at this layer; returns `Result` for forward-compat
    /// with multi-listener support in a later wave.
    #[allow(
        clippy::unnecessary_wraps,
        reason = "intentionally fallible at the surface even though today's single-anchor implementation cannot fail; multi-listener support in a later wave will materialise the error path without breaking callers"
    )]
    pub fn register_listener(
        &mut self,
        entity: Entity,
        transform: &Transform,
    ) -> Result<(), ManagerError> {
        // Push the initial pose into the anchor listener so the first
        // sync_pose call doesn't have to special-case construction.
        let position = mint::Vector3 {
            x: transform.position[0],
            y: transform.position[1],
            z: transform.position[2],
        };
        let orientation = mint::Quaternion {
            v: mint::Vector3 {
                x: transform.rotation[0],
                y: transform.rotation[1],
                z: transform.rotation[2],
            },
            s: transform.rotation[3],
        };
        self.anchor_listener
            .set_position(position, kira::Tween::default());
        self.anchor_listener
            .set_orientation(orientation, kira::Tween::default());
        // Each registered listener gets its own ListenerState referencing the
        // shared anchor; in single-listener mode the schedule walks the only
        // entry on each tick.
        let state = ListenerState::new_anchor(transform);
        self.listeners.insert(entity, state);
        Ok(())
    }

    /// Drop a listener registration.
    pub fn unregister_listener(&mut self, entity: Entity) -> bool {
        self.listeners.remove(&entity).is_some()
    }

    /// Number of currently-registered sources.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Number of currently-registered listeners.
    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Direct access to the underlying Kira manager. Reserved for low-level
    /// integration code (e.g. snapshot save/restore in W03 PIE).
    pub fn kira(&mut self) -> &mut KiraManager<B> {
        &mut self.inner
    }

    /// Shared listener handle — useful for low-level callers that want to
    /// attach their own spatial sub-tracks bound to the same listener pose.
    pub fn anchor_listener_mut(&mut self) -> &mut ListenerHandle {
        &mut self.anchor_listener
    }

    /// `pub(crate)` access for the schedule module. Avoids exposing
    /// `SourceState` mutability outside the crate.
    #[allow(
        clippy::type_complexity,
        reason = "tuple matches the four caller sites in schedule.rs; introducing a struct adds more noise than it saves at the destructure sites"
    )]
    pub(crate) fn parts_mut(
        &mut self,
    ) -> (
        &mut KiraManager<B>,
        &mut HashMap<String, StaticSoundData>,
        &mut HashMap<Entity, SourceState>,
        &mut HashMap<Entity, ListenerState>,
        &mut ListenerHandle,
    ) {
        (
            &mut self.inner,
            &mut self.clips,
            &mut self.sources,
            &mut self.listeners,
            &mut self.anchor_listener,
        )
    }
}

#[cfg(test)]
mod tests {
    use kira::backend::mock::{MockBackend, MockBackendSettings};

    use super::*;

    fn mock_manager() -> AudioManager<MockBackend> {
        AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
            backend_settings: MockBackendSettings {
                sample_rate: 48_000,
            },
            ..Default::default()
        })
        .expect("mock backend always succeeds")
    }

    /// Constructor establishes a working manager with one anchor listener.
    #[test]
    fn manager_starts_clean() {
        let mgr = mock_manager();
        assert_eq!(mgr.source_count(), 0);
        assert_eq!(mgr.listener_count(), 0);
    }

    /// register / unregister source round-trips and updates `source_count`.
    #[test]
    fn source_registration_round_trip() {
        let mut mgr = mock_manager();
        let entity = Entity(7);
        let xform = Transform::default();
        let src = AudioSource::default();
        mgr.register_source(entity, &xform, &src).unwrap();
        assert_eq!(mgr.source_count(), 1);
        assert!(mgr.unregister_source(entity));
        assert_eq!(mgr.source_count(), 0);
        // Second unregister is a no-op.
        assert!(!mgr.unregister_source(entity));
    }

    /// Listener registration analogous to source.
    #[test]
    fn listener_registration_round_trip() {
        let mut mgr = mock_manager();
        let entity = Entity(99);
        let xform = Transform::default();
        mgr.register_listener(entity, &xform).unwrap();
        assert_eq!(mgr.listener_count(), 1);
        assert!(mgr.unregister_listener(entity));
        assert_eq!(mgr.listener_count(), 0);
    }

    /// Clips registered from raw samples come back with the same length.
    #[test]
    #[allow(
        clippy::cast_precision_loss,
        reason = "test fixture; i is bounded by 1000, far below f32 mantissa limit"
    )]
    fn register_clip_from_samples_preserves_length() {
        let mut mgr = mock_manager();
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();
        mgr.register_clip_from_samples("sine", 48_000, &samples);
        let data = mgr.clip("sine").expect("clip exists after register");
        assert_eq!(data.frames.len(), 1000);
        assert_eq!(data.sample_rate, 48_000);
    }
}
