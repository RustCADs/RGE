//! `rge-components-audio` — audio-side ECS components.
//!
//! Failure class: recoverable
//!
//! Audio source (the emitter), audio listener (typically attached to the
//! camera), audio falloff curve. Audio mixing lives in `crates/audio` (W12);
//! this crate is state-only.
//!
//! ## Wave W01 stub
//!
//! `AssetId` ships locally — same shape as the eventual `kernel/asset` type.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod asset_id;
mod audio_falloff;
mod audio_listener;
mod audio_source;

pub use asset_id::{AssetId, NULL_ASSET_ID};
pub use audio_falloff::{AudioFalloff, FalloffShape};
pub use audio_listener::AudioListener;
pub use audio_source::AudioSource;
