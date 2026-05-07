//! W12 exit criterion: `AudioSource` at 10 m with `InverseSquare` falloff has
//! 1/100 the amplitude of the same source at 1 m.
//!
//! Two flavours of the test:
//! 1. **Pure host-side**: confirm `AudioFalloff::amplitude` returns the
//!    physical inverse-square value. This is the source of truth — Kira's
//!    own spatializer uses an `Easing`-based approximation.
//! 2. **End-to-end through Kira**: drive an emitter through the
//!    `MockBackend` and confirm the spatial mix actually routes audio (rms > 0).
//!    The in-engine ratio is approximate (Kira uses `OutPowi(2)` against a
//!    normalised-distance window) but the routing smoke test is the part the
//!    schedule layer cares about.

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::AudioManagerSettings;
use rge_audio::components::{AudioSource, Entity, Transform};
use rge_audio::falloff::AudioFalloff;
use rge_audio::test_support::FrameCapture;
use rge_audio::waveform::sine_wave;
use rge_audio::{audio_schedule_step, AudioManager, AudioSchedule, PlaybackState};

fn mock_manager(sample_rate: u32) -> (AudioManager<MockBackend>, FrameCapture) {
    let (capture, main_track_builder) = FrameCapture::main_track_builder();
    let mgr = AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
        backend_settings: MockBackendSettings { sample_rate },
        main_track_builder,
        ..Default::default()
    })
    .expect("mock backend should always succeed");
    (mgr, capture)
}

#[test]
fn inverse_square_decade_yields_hundredth_amplitude() {
    // Host-side amplitude function — exit criterion in its strictest form.
    let near = AudioFalloff::InverseSquare.amplitude(1.0, 1.0, 100.0);
    let far = AudioFalloff::InverseSquare.amplitude(10.0, 1.0, 100.0);
    assert!((near - 1.0).abs() < 1e-6, "near = {near}, expected 1.0");
    assert!((far - 0.01).abs() < 1e-6, "far = {far}, expected 0.01");
    let ratio = far / near;
    assert!(
        (ratio - 0.01).abs() < 1e-6,
        "ratio = {ratio}, expected 0.01 (1/100)"
    );
}

/// Drive an emitter through Kira and verify it actually routes audio
/// through the spatial mix stage (i.e. spatial sub-track + listener are
/// wired and sample output is non-zero).
///
/// This is a smoke test for the `register_source` / `register_listener` /
/// `audio_schedule_step` happy path. The strict 1/100 ratio is asserted on
/// the host-side curve in [`inverse_square_decade_yields_hundredth_amplitude`]
/// — Kira's own attenuation is a curve approximation that doesn't match the
/// physical formula at every point in the (min, max) window.
#[test]
fn kira_spatial_mix_routes_emitter_to_listener() {
    const SR: u32 = 48_000;

    let (mut mgr, capture) = mock_manager(SR);
    let samples = sine_wave(440.0, SR, 0.5);
    mgr.register_clip_from_samples("sine", SR, &samples);

    let listener = Entity(1);
    mgr.register_listener(listener, &Transform::default())
        .unwrap();

    let xform = Transform::from_position([0.0, 0.0, -1.0]);
    let source = AudioSource {
        clip: "sine".into(),
        desired_state: PlaybackState::Playing,
        falloff: AudioFalloff::InverseSquare,
        distances: (1.0, 100.0),
        ..AudioSource::default()
    };
    let entity = Entity(2);
    let rms = render_one(&mut mgr, &capture, listener, entity, &xform, &source);
    assert!(rms > 0.0, "spatial mix produced silence — pipeline broken");
}

fn render_one(
    mgr: &mut AudioManager<MockBackend>,
    capture: &FrameCapture,
    listener: Entity,
    entity: Entity,
    xform: &Transform,
    source: &AudioSource,
) -> f32 {
    let listener_xform = Transform::default();
    mgr.register_source(entity, xform, source).unwrap();
    let sched = [AudioSchedule {
        entity,
        transform: xform,
        source,
    }];
    audio_schedule_step(mgr, &sched, Some((listener, &listener_xform))).unwrap();

    // Render at least 1024 samples (multiple internal_buffer_size chunks at
    // the default 128) and return RMS over the captured frames.
    mgr.kira().backend_mut().on_start_processing();
    while capture.len() < 1024 {
        mgr.kira().backend_mut().process();
    }
    let frames = capture.take();
    let mut sum_sq = 0.0f32;
    let n = frames.len().min(1024);
    for frame in frames.iter().take(n) {
        let mono = (frame.left + frame.right) * 0.5;
        sum_sq += mono * mono;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "RMS divisor; n is bounded by frames.len().min(1024), far below f32 mantissa limit"
    )]
    let denom = n as f32;
    (sum_sq / denom).sqrt()
}
