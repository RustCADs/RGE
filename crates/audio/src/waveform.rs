//! Test-helper waveform generator.
//!
//! Tiny pure functions that produce raw `f32` mono sample buffers — used by
//! the W12 sine-wave exit-criterion test (`tests/playback_test.rs`) and the
//! mix-saturation test (`tests/mix_test.rs`).
//!
//! Public so integration tests can call them; not part of the runtime API.

// Sample-rate arithmetic in audio code lives in f32-vs-u32 space by
// convention. Mantissa-precision-loss warnings here would just be noise.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

/// Generate a `frequency`-Hz sine wave at `sample_rate` Hz lasting
/// `duration_secs` seconds. Amplitude is ±1.0.
///
/// ```
/// let samples = rge_audio::waveform::sine_wave(440.0, 48_000, 1.0);
/// assert_eq!(samples.len(), 48_000);
/// // First sample of a sine starting at phase 0 is exactly 0.0.
/// assert!(samples[0].abs() < 1e-6);
/// ```
#[must_use]
pub fn sine_wave(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let n = (sample_rate as f32 * duration_secs).round() as usize;
    let mut out = Vec::with_capacity(n);
    let two_pi = std::f32::consts::TAU;
    let inc = two_pi * frequency / sample_rate as f32;
    let mut phase = 0.0f32;
    for _ in 0..n {
        out.push(phase.sin());
        phase += inc;
        if phase >= two_pi {
            phase -= two_pi;
        }
    }
    out
}

/// Reference sample for a 440 Hz sine at sample index `i`, sample rate
/// `sample_rate` — used by `tests/playback_test.rs` for tolerance checks.
#[must_use]
pub fn sine_reference(frequency: f32, sample_rate: u32, sample_index: usize) -> f32 {
    let phase = std::f32::consts::TAU * frequency * (sample_index as f32) / (sample_rate as f32);
    phase.sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generated buffer has the right length.
    #[test]
    fn length_matches_duration() {
        let samples = sine_wave(440.0, 48_000, 0.5);
        assert_eq!(samples.len(), 24_000);
    }

    /// First sample of a sine starting at phase 0 is 0.
    #[test]
    fn starts_at_zero() {
        let samples = sine_wave(440.0, 48_000, 0.01);
        assert!(samples[0].abs() < 1e-6);
    }

    /// Generated samples are within unit amplitude.
    #[test]
    fn amplitude_within_unit() {
        let samples = sine_wave(440.0, 48_000, 0.05);
        assert!(samples.iter().all(|s| s.abs() <= 1.0 + 1e-5));
    }

    /// Reference function and generator agree on early samples within
    /// the documented 1% tolerance.
    #[test]
    fn generator_matches_reference_within_one_percent() {
        let sr = 48_000_u32;
        let samples = sine_wave(440.0, sr, 0.01);
        for i in 0..100 {
            let actual = samples[i];
            let expected = sine_reference(440.0, sr, i);
            // 1% tolerance OR 1e-3 absolute, whichever larger — sine values
            // near zero crossings have small absolute error but huge relative.
            let abs_err = (actual - expected).abs();
            let rel_err = abs_err / expected.abs().max(1e-3);
            assert!(
                abs_err < 1e-3 || rel_err < 0.01,
                "i={i} actual={actual} expected={expected} abs={abs_err} rel={rel_err}"
            );
        }
    }
}
