//! Wave-W12 local stub of the ECS components defined by W01
//! (`rge-components-audio` + `rge-components-spatial`).
//!
//! Both upstream crates exist in the workspace but are still empty stubs at
//! the time of this wave. To keep W12 self-contained per the dispatch package's
//! "Touch ONLY crates/audio/" non-interference rule, we mirror the canonical
//! shapes here. When W01 / the spatial side land, this module is replaced by
//! `pub use rge_components_audio::*; pub use rge_components_spatial::*;`.
//!
//! State-only — no behavior, no schedule wiring. The audio update loop reaches
//! into these via [`crate::schedule::audio_schedule_step`].

use serde::{Deserialize, Serialize};

use crate::falloff::AudioFalloff;

/// Opaque ECS entity handle. Matches the W01 `Entity(u64)` newtype shape so
/// that swapping for the canonical type is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Entity(pub u64);

impl Entity {
    /// Construct from a raw integer.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// 3D pose component — position + orientation. Mirrors the
/// `rge-components-spatial::Transform` shape that the spatial wave will publish.
///
/// `position` is metres in world space. `rotation` is a unit quaternion `(x,y,z,w)`.
/// An unrotated [`AudioListener`] faces `-Z`, with `+X` to the right and `+Y`
/// up — same convention as Kira so we can pipe the components straight through.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// World-space position in metres.
    pub position: [f32; 3],
    /// Unit quaternion in `(x, y, z, w)` order.
    pub rotation: [f32; 4],
    /// Per-axis world-space scale. Audio ignores scale, but the field is here
    /// for shape-equivalence with the canonical `Transform`.
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform {
    /// Convenience: position-only constructor.
    #[must_use]
    pub const fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// `AudioSource` ECS component — handle to a Kira emitter + per-source
/// playback parameters (volume, pitch, loop-flag).
///
/// Per [`crate::manager::AudioManager`] one source maps to exactly one
/// emitter in the world's spatial scene. The matching Kira `EmitterHandle`
/// lives inside the [`AudioManager`] resource keyed by [`Entity`] —
/// component data here is plain old data, no `Arc<Mutex<...>>` smuggled into
/// the ECS world.
///
/// The schedule step in [`crate::schedule`] is responsible for picking up
/// changed [`AudioSource::desired_state`] and turning it into Kira commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSource {
    /// Asset key — opaque handle into the cooked audio store. Stub uses a
    /// `String` so tests can drive it without a real asset pipeline.
    pub clip: String,

    /// Linear amplitude multiplier in `0.0..=2.0` (1.0 = unity gain). Mapped
    /// to `kira::Volume::Amplitude` on dispatch.
    pub volume: f32,

    /// Pitch / playback-rate factor. `1.0` = original pitch & speed, `2.0` =
    /// up an octave & double-speed. Mapped to `kira::sound::PlaybackRate::Factor`.
    pub pitch: f32,

    /// Whether playback loops at `clip` end.
    pub looped: bool,

    /// What gameplay code wants this source to do; the schedule step
    /// reconciles this against the current Kira state.
    pub desired_state: super::PlaybackState,

    /// Falloff curve — controls distance attenuation in [`crate::listener`].
    pub falloff: AudioFalloff,

    /// Min and max distance (metres) for the falloff curve. Below `min` the
    /// source is at full volume; above `max` it's silent.
    pub distances: (f32, f32),
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            clip: String::new(),
            volume: 1.0,
            pitch: 1.0,
            looped: false,
            desired_state: super::PlaybackState::Stopped,
            falloff: AudioFalloff::default(),
            distances: (1.0, 100.0),
        }
    }
}

/// `AudioListener` ECS component — per [`PLAN.md`](../../plans/PLAN.md) §1.5.1
/// typically attached to the `Camera` entity.
///
/// Holds nothing other than per-listener gain (mute / unmute via gain `0`).
/// Position + orientation are read from the entity's [`Transform`] every
/// schedule tick.
///
/// Multiple listeners are legal at the type level (multi-viewport) but the
/// W12 schedule routes only the first one it encounters per tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioListener {
    /// Master gain for this listener, linear amplitude. `1.0` = unity.
    pub gain: f32,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}
