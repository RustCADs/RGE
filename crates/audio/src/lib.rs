//! `rge-audio` — Kira wrap, ECS-integrated audio source/listener, mixer.
//!
//! Failure class: recoverable
//!
//! Wave **W12** deliverable per [`tasks/W12/PLAN.md`](../../tasks/W12/PLAN.md).
//! Subsystem owner per [`PLAN.md`](../../plans/PLAN.md) §6 (Phase 5+).
//!
//! ## Layering
//!
//! ```text
//! rge-audio (this crate, host-side wrap)
//!     |
//!     v
//! kira (cross-platform audio engine, MockBackend used in tests)
//! ```
//!
//! `rge-audio` does not own its own DSP: every sample that ships ends up
//! through Kira's mixer. The crate's job is to bridge ECS world state into
//! Kira resources (one [`AudioManager`] per world; one Kira spatial scene per
//! world) and to expose a small, ECS-friendly component API for gameplay code.
//!
//! ## ECS-shaped surface
//!
//! Per [`PLAN.md`](../../plans/PLAN.md) §1.5.1 the canonical entity roles map
//! one-to-one onto components in the W01 stub crate `rge-components-audio`.
//! That crate is still empty at the time of writing, so this wave ships a
//! local stub of [`AudioSource`], [`AudioListener`], [`AudioFalloff`], plus a
//! placeholder [`Transform`] from the spatial wave. When W01 lands the local
//! stubs become re-exports.
//!
//! ## Module map
//!
//! | Module        | Role                                                   |
//! |---------------|--------------------------------------------------------|
//! | [`manager`]   | [`AudioManager`] resource — one per ECS world.         |
//! | [`source`]    | [`AudioSource`] component — play/pause/stop, vol, pitch|
//! | [`listener`]  | [`AudioListener`] component — typically on a Camera.   |
//! | [`falloff`]   | [`AudioFalloff`] curves + reference amplitude function.|
//! | [`schedule`]  | Per-frame mixer update; Transform → Kira sync.         |
//! | [`components`]| Local W01 stub — replace with re-exports post-W01.     |
//! | [`waveform`]  | Test-helper sine generator + [`Frame`] buffer.         |
//!
//! [`Frame`]: kira::Frame

#![forbid(unsafe_code)]

pub mod components;
pub mod falloff;
pub mod listener;
pub mod manager;
pub mod plugin_adapter;
pub mod schedule;
pub mod source;
pub mod test_support;
pub mod waveform;

pub use components::{AudioListener, AudioSource, Transform};
pub use falloff::AudioFalloff;
/// Re-export of [`kira`] so downstream callers don't need a direct dep.
pub use kira;
pub use listener::ListenerState;
pub use manager::{AudioManager, ManagerError};
pub use plugin_adapter::{
    AudioFrame, AudioPlugin, FrameRecord, OwnedAudioSchedule, AUDIO_PLUGIN_ID,
};
pub use schedule::{audio_schedule_step, AudioSchedule};
pub use source::{PlaybackState, SourceState};
