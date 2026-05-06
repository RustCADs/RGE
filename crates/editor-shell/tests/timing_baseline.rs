//! Phase 5.3 BASELINE.md harness: capture round-trip wall-time at three scales.
//!
//! Measures `WorldSnapshot::capture` + `WorldSnapshot::restore` using the
//! **real `rge_kernel_ecs::World`** with two registered [`SnapshotComponent`]s
//! (Position + `TickCounter`). Replaces the W03 stub which used a flat
//! `BTreeMap<ComponentTypeId, Vec<u8>>` blob store.
//!
//! Output (via `cargo test -- --nocapture`) is copied into
//! `RGE/plans/BASELINE.md`.
//!
//! **Abort condition** (per `IMPLEMENTATION.md` Phase 5): 10k-entity round-trip
//! must complete under **500ms** in `--release` mode.
//!
//! Run mode:
//! ```
//! cargo test -p rge-editor-shell --release --test timing_baseline -- --nocapture
//! ```

use rge_editor_shell::snapshot::measure_round_trip;
use rge_editor_shell::world::World;
use rge_kernel_ecs::{Component, SnapshotComponent};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Typed fixture components (real kernel snapshot components)
// ---------------------------------------------------------------------------

/// 3-axis position component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}
impl Component for Position {}
impl SnapshotComponent for Position {}

/// Monotonic tick counter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TickCounter(u64);
impl Component for TickCounter {}
impl SnapshotComponent for TickCounter {}

// ---------------------------------------------------------------------------
// Scene builder
// ---------------------------------------------------------------------------

/// Spawn `n` entities each carrying a `Position` and a `TickCounter`.
///
/// Both components are registered as snapshot components so the kernel
/// snapshot path captures them. This is the scene used for the Phase 5 abort
/// gate measurement.
fn build_scene(n: usize) -> World {
    let mut w = World::new();
    w.register_snapshot_component::<Position>();
    w.register_snapshot_component::<TickCounter>();
    #[allow(clippy::cast_precision_loss)]
    for i in 0..n {
        let e = w.spawn();
        w.kernel_mut().insert(
            e,
            Position {
                x: i as f32,
                y: (i as f32) * 0.5,
                z: 1.0,
            },
        );
        w.kernel_mut().insert(e, TickCounter(i as u64));
    }
    w
}

// ---------------------------------------------------------------------------
// Reporter
// ---------------------------------------------------------------------------

fn report(label: &str, n: usize) {
    let mut w = build_scene(n);
    // One warmup pass (allocator + instruction cache warm-up).
    let _ = measure_round_trip(&mut w);
    // Take min-of-3 to suppress OS scheduling noise.
    let mut best = measure_round_trip(&mut w);
    for _ in 0..2 {
        let m = measure_round_trip(&mut w);
        if m.total() < best.total() {
            best = m;
        }
    }
    println!(
        "BASELINE [{}]: entities={} bytes={} capture={:?} restore={:?} total={:?}  abort_threshold_breached={}",
        label,
        best.entity_count,
        best.serialized_bytes,
        best.capture,
        best.restore,
        best.total(),
        best.exceeds_phase5_abort_threshold(),
    );
    assert!(
        !best.exceeds_phase5_abort_threshold(),
        "Phase 5 abort condition: 10k-entity round-trip ({:?}) exceeded 500ms — ECS redesign required",
        best.total()
    );
}

// ---------------------------------------------------------------------------
// Test targets
// ---------------------------------------------------------------------------

#[test]
fn baseline_100_entities() {
    report("100", 100);
}

#[test]
fn baseline_1000_entities() {
    report("1k", 1_000);
}

#[test]
fn baseline_10000_entities() {
    report("10k", 10_000);
}
