//! W13 exit: gamepad axis dead-zone (default `0.1`) suppresses `|x| < 0.1`
//! and passes larger magnitudes through unchanged.
//!
//! These tests exercise `apply_dead_zone` directly (not via `GamepadPoller`)
//! because gilrs cannot initialise on headless CI runners — `GamepadPoller::new`
//! returns `None` in that environment, making a poll-based test flaky.

// `apply_dead_zone` produces *exact* `0.0` for in-band values and exact
// passthrough for out-of-band — the `float_cmp` lint is intentionally
// silenced for this test file because exact comparison is the contract
// being verified.
#![allow(clippy::float_cmp)]

use rge_input::{apply_dead_zone, DEFAULT_DEAD_ZONE};

#[test]
fn default_dead_zone_is_pointone() {
    assert!((DEFAULT_DEAD_ZONE - 0.1).abs() < 1e-6);
}

#[test]
fn small_positive_value_clamps_to_zero() {
    assert_eq!(apply_dead_zone(0.05, DEFAULT_DEAD_ZONE), 0.0);
}

#[test]
fn small_negative_value_clamps_to_zero() {
    assert_eq!(apply_dead_zone(-0.05, DEFAULT_DEAD_ZONE), 0.0);
}

#[test]
fn just_under_threshold_clamps() {
    // Resting-stick noise around 0.099 should not bleed into the event
    // stream — this is the headline W13 exit criterion.
    assert_eq!(apply_dead_zone(0.0999, DEFAULT_DEAD_ZONE), 0.0);
    assert_eq!(apply_dead_zone(-0.0999, DEFAULT_DEAD_ZONE), 0.0);
}

#[test]
fn at_threshold_passes_through() {
    // |x| == dead_zone is NOT inside the suppression band (|x| < dead_zone).
    assert!((apply_dead_zone(0.1, DEFAULT_DEAD_ZONE) - 0.1).abs() < 1e-6);
}

#[test]
fn over_threshold_passes_unchanged() {
    assert_eq!(apply_dead_zone(0.5, DEFAULT_DEAD_ZONE), 0.5);
    assert_eq!(apply_dead_zone(-0.75, DEFAULT_DEAD_ZONE), -0.75);
    assert_eq!(apply_dead_zone(1.0, DEFAULT_DEAD_ZONE), 1.0);
}

#[test]
fn custom_dead_zone_respected() {
    // A loose 0.25 dead-zone (gritty stick) suppresses values 0.1 / 0.2
    // that the default would pass.
    assert_eq!(apply_dead_zone(0.1, 0.25), 0.0);
    assert_eq!(apply_dead_zone(0.2, 0.25), 0.0);
    assert_eq!(apply_dead_zone(0.3, 0.25), 0.3);
}

#[test]
fn zero_dead_zone_is_passthrough() {
    // Disabling dead-zone entirely.
    assert_eq!(apply_dead_zone(0.001, 0.0), 0.001);
    assert_eq!(apply_dead_zone(-0.001, 0.0), -0.001);
}
