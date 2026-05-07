//! W12 exit criterion: 8 simultaneous sources mix without clipping.
//!
//! Each source is a 0.1 amplitude sine — 8-source naive max sum 0.8, with
//! headroom for Kira's spatial path which can apply an ear-pan boost slightly
//! above the geometric sum. The test asserts that Kira's mixer produces
//! output strictly within `[-1.0, 1.0]` (the canonical hard-clip threshold
//! for 16-bit sample formats).

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::{AudioManagerSettings, Frame};
use rge_audio::components::{AudioSource, Entity, Transform};
use rge_audio::test_support::FrameCapture;
use rge_audio::waveform::sine_wave;
use rge_audio::{audio_schedule_step, AudioManager, AudioSchedule, PlaybackState};

// Sources need to live for the duration of the schedule call; collect them
// all into this fixture struct so we can build the slice without temporary
// lifetime issues.
struct PreparedSource {
    entity: Entity,
    transform: Transform,
    source: AudioSource,
}

#[test]
fn eight_sources_do_not_clip() {
    const SR: u32 = 48_000;
    const N_SOURCES: usize = 8;
    // 8 sources at 0.1 amplitude → naive max sum 0.8, leaving headroom for
    // Kira's spatial path which can apply an ear-pan boost slightly above
    // the geometric sum. This represents the standard "play several
    // simultaneous game sounds at moderate volume" scenario.
    const PER_SOURCE_AMPLITUDE: f32 = 0.1;

    let (capture, main_track_builder) = FrameCapture::main_track_builder();
    let mut mgr = AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
        backend_settings: MockBackendSettings { sample_rate: SR },
        main_track_builder,
        ..Default::default()
    })
    .unwrap();

    // Register the 8 distinct clips. Each source is given a unique frequency
    // so phase-cancellation doesn't artificially keep us under 1.0.
    let frequencies: [f32; N_SOURCES] = [220.0, 277.0, 330.0, 392.0, 440.0, 523.0, 659.0, 784.0];
    for (i, &freq) in frequencies.iter().enumerate() {
        let key = format!("clip{i}");
        // Direct amplitude scale on samples — keeps the mixer's ear-panning
        // and falloff out of the comparison; we want raw mix-stage clipping.
        let raw = sine_wave(freq, SR, 0.25);
        let scaled: Vec<f32> = raw.iter().map(|s| s * PER_SOURCE_AMPLITUDE).collect();
        mgr.register_clip_from_samples(key, SR, &scaled);
    }

    // Listener and 8 co-located sources at the origin (no spatial
    // attenuation in play — this is a pure mixer-saturation check).
    let listener = Entity(1);
    mgr.register_listener(listener, &Transform::default())
        .unwrap();

    let mut prepared = Vec::with_capacity(N_SOURCES);
    for i in 0..N_SOURCES {
        let entity = Entity((i as u64) + 100);
        let transform = Transform::from_position([0.0, 0.0, -1.0]);
        let source = AudioSource {
            clip: format!("clip{i}"),
            desired_state: PlaybackState::Playing,
            volume: 1.0,
            distances: (1.0, 100.0),
            ..AudioSource::default()
        };
        mgr.register_source(entity, &transform, &source).unwrap();
        prepared.push(PreparedSource {
            entity,
            transform,
            source,
        });
    }

    let scheds: Vec<AudioSchedule<'_>> = prepared
        .iter()
        .map(|p| AudioSchedule {
            entity: p.entity,
            transform: &p.transform,
            source: &p.source,
        })
        .collect();
    audio_schedule_step(&mut mgr, &scheds, Some((listener, &Transform::default()))).unwrap();

    // Render until the capture has at least 4096 frames and watch for any
    // sample outside [-1, 1]. We also capture peak / RMS for diagnostics on
    // failure.
    mgr.kira().backend_mut().on_start_processing();
    while capture.len() < 4096 {
        mgr.kira().backend_mut().process();
    }
    let frames: Vec<Frame> = capture.take();
    let n = frames.len().min(4096);
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f32;
    for (i, frame) in frames.iter().take(n).enumerate() {
        for sample in [frame.left, frame.right] {
            let abs: f32 = sample.abs();
            if abs > peak {
                peak = abs;
            }
            sum_sq += sample * sample;
            assert!(
                abs <= 1.0,
                "sample at index {i} clips: |{sample}| > 1.0 (peak so far {peak})"
            );
        }
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "RMS divisor; n is bounded by frames.len().min(4096), far below f32 mantissa limit"
    )]
    let denom = 2.0 * n as f32;
    let rms = (sum_sq / denom).sqrt();
    // Sanity: the renderer actually produced mixed output. With 8 distinct
    // frequencies we expect noticeable RMS, well above noise floor.
    assert!(
        rms > 0.05,
        "mixer produced near-silence (rms = {rms}); did clips actually play?"
    );
    assert!(peak > 0.1, "peak = {peak}; mix appears flat");
}
