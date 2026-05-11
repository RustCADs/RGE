//! Phase 6 §6.3 Gate B — editor idle-frame timing harness (hybrid).
//!
//! Measures the CURRENT EMPTY-SHELL CPU-idle baseline using batch
//! timing to clear the Windows `Instant` resolution floor. PLAN §13.2
//! gate: idle editor frame ≤ 8 ms.
//!
//! **NOT a loaded-editor measurement.** The empty `EditorShell::new()`
//! shell's `tick_redraw` is ~100 ns per call — far below any realistic
//! loaded-editor frame. This harness records that baseline so Gate B
//! is closed for the current CPU-idle interpretation; a future dispatch
//! re-measures once non-trivial editor systems / idle scene are wired.

use std::time::Instant;

use rge_editor_shell::EditorShell;

const N: usize = 1000; // frames per batch
const K: usize = 10; // batches

#[test]
#[ignore = "release-only timing harness — invoke via `cargo test -p rge-editor-shell --release --test editor_frame_idle -- --ignored --nocapture`; debug builds produce >30% variance and falsely trip the variance gate"]
fn idle_frame_p95_within_gate_batched() {
    let mut shell = EditorShell::new();

    let mut batch_means_ms: Vec<f64> = Vec::with_capacity(K);
    for _ in 0..K {
        let start = Instant::now();
        for _ in 0..N {
            shell.tick_redraw();
        }
        let batch_total_ms = start.elapsed().as_secs_f64() * 1000.0;
        batch_means_ms.push(batch_total_ms / N as f64);
    }

    batch_means_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = batch_means_ms[K / 2];
    let p95 = batch_means_ms[(K * 95) / 100]; // K=10 -> idx 9 -> max
    let min = batch_means_ms[0];
    let max = batch_means_ms[K - 1];
    let variance_pct = (max - min) / p50 * 100.0;

    eprintln!(
        "Gate B (hybrid, empty-shell baseline): batch N={N}, K={K} \
         -> P50 = {p50:.6} ms, P95 = {p95:.6} ms, \
         variance across batch means = {variance_pct:.1}%"
    );

    assert!(
        variance_pct < 30.0,
        "batch-mean variance {variance_pct:.1}% exceeds 30% — measurement unstable"
    );
    assert!(
        p95 <= 8.0,
        "P95 = {p95:.6} ms exceeds Gate B threshold of 8 ms — record and escalate, do not tune"
    );
}
