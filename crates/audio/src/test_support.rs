//! Test-only helpers for capturing rendered audio frames.
//!
//! Kira 0.12's [`MockBackend::process`](kira::backend::mock::MockBackend::process)
//! writes the rendered samples into a private `Vec<f32>` and exposes no public
//! getter. This module installs a small [`Effect`](kira::effect::Effect) on the
//! main mixer track that copies each batch of rendered [`Frame`]s into a
//! handle the test can read.
//!
//! Usage:
//! ```ignore
//! use rge_audio::test_support::FrameCapture;
//! use kira::{
//!     backend::mock::{MockBackend, MockBackendSettings},
//!     track::MainTrackBuilder,
//!     AudioManagerSettings,
//! };
//!
//! let (capture, builder) = FrameCapture::main_track_builder();
//! let mut mgr = rge_audio::AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
//!     backend_settings: MockBackendSettings { sample_rate: 48_000 },
//!     main_track_builder: builder,
//!     ..Default::default()
//! }).unwrap();
//! // ... play sounds, drive backend ...
//! mgr.kira().backend_mut().on_start_processing();
//! mgr.kira().backend_mut().process();
//! let frames = capture.take();
//! ```
//!
//! Public so that the `tests/` directory can drive RMS / saturation checks
//! without poking at Kira internals.

use std::sync::{Arc, Mutex};

use kira::effect::{Effect, EffectBuilder};
use kira::info::Info;
use kira::track::MainTrackBuilder;
use kira::Frame;

/// Test-only frame-capture sink. Constructed via
/// [`Self::main_track_builder`]; the returned handle is `Clone`/`Send` so a
/// test can keep a copy after handing the builder to the manager.
#[derive(Clone, Debug, Default)]
pub struct FrameCapture {
    inner: Arc<Mutex<Vec<Frame>>>,
}

impl FrameCapture {
    /// Construct a capture handle pre-installed on a fresh
    /// [`MainTrackBuilder`]. Wire the builder into
    /// [`AudioManagerSettings::main_track_builder`](
    /// kira::AudioManagerSettings::main_track_builder) before constructing the
    /// manager.
    #[must_use]
    pub fn main_track_builder() -> (Self, MainTrackBuilder) {
        let capture = Self::default();
        let mut builder = MainTrackBuilder::new();
        builder.add_effect(CaptureBuilder {
            sink: capture.inner.clone(),
        });
        (capture, builder)
    }

    /// Drain the captured frames, leaving the sink empty.
    ///
    /// # Panics
    ///
    /// Panics if the internal capture mutex has been poisoned by a previous
    /// panic in [`CaptureEffect::process`]. In test usage the effect's
    /// `process` body never panics so poisoning is unreachable in practice.
    #[must_use]
    pub fn take(&self) -> Vec<Frame> {
        let mut guard = self.inner.lock().expect("capture mutex poisoned");
        std::mem::take(&mut *guard)
    }

    /// Number of frames currently buffered.
    ///
    /// # Panics
    ///
    /// Panics if the internal capture mutex has been poisoned by a previous
    /// panic in [`CaptureEffect::process`]. In test usage the effect's
    /// `process` body never panics so poisoning is unreachable in practice.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("capture mutex poisoned").len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `EffectBuilder` companion for [`FrameCapture`]. Not exposed because callers
/// go through [`FrameCapture::main_track_builder`].
struct CaptureBuilder {
    sink: Arc<Mutex<Vec<Frame>>>,
}

impl EffectBuilder for CaptureBuilder {
    type Handle = ();

    fn build(self) -> (Box<dyn Effect>, Self::Handle) {
        (Box::new(CaptureEffect { sink: self.sink }), ())
    }
}

/// In-line, allocation-free copy from the rendered chunk into the capture
/// sink. The mutex is held for the chunk only — chunks are 128 samples by
/// default at 44.1 kHz so contention is negligible at test scale.
struct CaptureEffect {
    sink: Arc<Mutex<Vec<Frame>>>,
}

impl Effect for CaptureEffect {
    fn process(&mut self, input: &mut [Frame], _dt: f64, _info: &Info) {
        if let Ok(mut guard) = self.sink.lock() {
            guard.extend_from_slice(input);
        }
    }
}
