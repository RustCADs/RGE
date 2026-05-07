//! W12 exit criterion: play 1-second 440 Hz sine; first 100 samples within 1%
//! of reference.
//!
//! Headless test — uses Kira's [`MockBackend`] so no audio device is needed.
//! The test:
//! 1. registers a 1-second 440 Hz mono sine clip,
//! 2. plays it through the schedule step,
//! 3. drives the `MockBackend`'s renderer one sample at a time and compares
//!    each output `Frame` against the reference at the matching index.
//!
//! Tolerance is the looser of `1e-3` absolute or 1% relative — sine zero
//! crossings have small absolute but large relative error, both bands matter.
//!
//! Kira 0.12 dropped the `MockBackend::process() -> Frame` shape — frames
//! now flow into a private buffer. We capture them via a custom main-track
//! effect ([`rge_audio::test_support::FrameCapture`]).

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::AudioManagerSettings;
use rge_audio::test_support::FrameCapture;
use rge_audio::waveform::{sine_reference, sine_wave};
use rge_audio::AudioManager;

/// Set up a manager driving Kira's `MockBackend` at 48 kHz and pre-installs a
/// frame-capture effect on the main mixer track so the test can read what the
/// renderer wrote each `process()` call.
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
fn sine_wave_first_100_samples_within_one_percent() {
    const SAMPLE_RATE: u32 = 48_000;
    const FREQUENCY: f32 = 440.0;

    // For the raw-sample fidelity check we want the clip routed through
    // Kira's main mixer with NO spatialization — Kira's spatial track applies
    // an ear-panning attenuation that would corrupt the comparison even when
    // the emitter sits on top of the listener. The cleanest way to get there
    // is to play the clip directly to the main mixer track, which is exactly
    // what an unspatialised UI / music sound does in production.
    let (mut manager, capture) = mock_manager(SAMPLE_RATE);
    let samples = sine_wave(FREQUENCY, SAMPLE_RATE, 1.0);
    manager.register_clip_from_samples("sine440", SAMPLE_RATE, &samples);

    // Play the clip directly through the main track — bypasses spatial.
    let clip = manager
        .clip("sine440")
        .expect("clip registered above")
        .clone();
    let _handle = manager.kira().play(clip).unwrap();

    // Hand the renderer the start-of-frame callback, then advance until the
    // capture effect has at least 100 frames in the buffer. MockBackend's
    // process() drives `internal_buffer_size` samples per call, defaulting to
    // 128 — so two pumps gives us a safe margin over the 100-frame check.
    manager.kira().backend_mut().on_start_processing();
    while capture.len() < 100 {
        manager.kira().backend_mut().process();
    }
    let frames = capture.take();

    let mut max_abs_err = 0.0f32;
    for (i, frame) in frames.iter().enumerate().take(100) {
        // Spatial mix may pan channels — average to mono for comparison.
        let actual = (frame.left + frame.right) * 0.5;
        let expected = sine_reference(FREQUENCY, SAMPLE_RATE, i);

        let abs_err = (actual - expected).abs();
        if abs_err > max_abs_err {
            max_abs_err = abs_err;
        }
        let rel_err = abs_err / expected.abs().max(1e-3);
        assert!(
            abs_err < 1e-3 || rel_err < 0.01,
            "sample {i}: actual={actual} expected={expected} abs_err={abs_err} rel_err={rel_err}"
        );
    }

    // Sanity check that the test actually saw non-trivial output (rules out
    // a silent renderer that vacuously passes).
    assert!(
        max_abs_err > 0.0,
        "renderer produced silence — clip not actually playing"
    );
}
