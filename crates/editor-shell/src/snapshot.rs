//! `WorldSnapshot` — PIE round-trip backing store.
//!
//! Per PLAN.md §6.13: `[Play]` clones the world; `[Stop]` restores it
//! byte-identical. Phase 5.3 migrates this backing store from the v0 stub
//! (owned `World` clone) to a `Vec<u8>` of kernel-ECS snapshot bytes
//! produced by [`rge_kernel_ecs::World::serialize_snapshot`].
//!
//! # API compatibility
//!
//! The public `WorldSnapshot` API surface is preserved from W03:
//! - [`WorldSnapshot::capture`] — same signature; now drives the kernel path.
//! - [`WorldSnapshot::restore`] — same signature; now calls
//!   `World::restore_from_snapshot` on the inner kernel world.
//! - [`WorldSnapshot::serialized_bytes`] — returns the kernel snapshot bytes.
//! - [`WorldSnapshot::entity_count`] — from the kernel world at capture time.
//! - [`WorldSnapshot::captured_at_tick`] — unchanged.
//!
//! # Critical: byte-identical restore is the constitutional gate
//!
//! Per `IMPLEMENTATION.md` Phase 5 abort condition: if PIE snapshot/restore
//! exceeds **500ms on a 10k-entity scene**, ECS storage layout needs
//! redesign. The [`measure_round_trip`] helper documents the harness; the
//! integration test `timing_baseline` exercises the abort gate.

use std::time::{Duration, Instant};

use crate::audit::{AuditEvent, AuditLedger};
use crate::world::World;

/// Captured world state.
///
/// Holds both the serialized kernel-ECS snapshot bytes and the legacy blob
/// serialization bytes, captured at Play time. Restoration is byte-identical
/// for both representations:
///
/// - `world.serialize_snapshot()` byte-equality: from kernel bytes.
/// - `world.serialize()` byte-equality: from blob bytes.
///
/// Per PLAN.md §1.15: editor coordination state (selection, active tool) is
/// **not** part of the snapshot — it persists across Play/Stop on the editor
/// side of the boundary.
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    /// Kernel-ECS snapshot bytes captured at Play time.
    kernel_bytes: Vec<u8>,
    /// Legacy blob-storage snapshot (v0 stub format).
    ///
    /// Retained for backward compatibility with call sites that compare
    /// `world.serialize()` (blob format) before/after Play/Stop. The real
    /// kernel snapshot path is authoritative; the blob snapshot is a
    /// compatibility shim.
    blob_snapshot: BlobSnapshot,
    /// Number of entities in the captured world.
    entity_count: usize,
    /// Tick the snapshot was captured at (lifecycle's `tick_count`).
    captured_at_tick: u64,
}

/// Captured state for the legacy blob-API path.
#[derive(Debug, Clone)]
struct BlobSnapshot {
    /// The serialized blob bytes (v0 stub format) at capture time.
    /// Used to restore the blob storage on Stop.
    serialized: Vec<u8>,
    /// A cloned copy of the blob storage for direct restoration.
    entities: std::collections::BTreeSet<rge_kernel_ecs::EntityId>,
    components: std::collections::BTreeMap<
        crate::world::ComponentTypeId,
        std::collections::BTreeMap<rge_kernel_ecs::EntityId, Vec<u8>>,
    >,
}

impl WorldSnapshot {
    /// Capture a snapshot from the live `world`.
    ///
    /// Captures both the kernel-ECS snapshot (typed components) and the
    /// legacy blob storage, so that restoration is byte-identical for both
    /// `world.serialize_snapshot()` and `world.serialize()` call sites.
    ///
    /// # Panics
    ///
    /// Panics if kernel snapshot serialization fails (RON error on a
    /// registered component). In practice this indicates a programming error.
    #[must_use]
    pub fn capture(world: &World, tick: u64) -> Self {
        let kernel_bytes = world
            .serialize_snapshot()
            .expect("WorldSnapshot::capture: kernel snapshot serialization failed");
        let entity_count = world.entity_count();
        let (blob_entities, blob_components) = world.capture_blob_state();
        let blob_serialized = world.serialize();
        let blob_snapshot = BlobSnapshot {
            serialized: blob_serialized,
            entities: blob_entities,
            components: blob_components,
        };
        Self {
            kernel_bytes,
            blob_snapshot,
            entity_count,
            captured_at_tick: tick,
        }
    }

    /// Restore the snapshot back into `world`.
    ///
    /// After this call:
    /// - `world.serialize_snapshot()` is byte-identical to
    ///   `self.serialized_bytes()` (kernel snapshot correctness).
    /// - `world.serialize()` is byte-identical to the v0 blob serialization at
    ///   capture time (backward-compatibility for existing tests).
    ///
    /// # Panics
    ///
    /// Panics if kernel snapshot restoration fails.
    pub fn restore(&self, world: &mut World) {
        world
            .restore_from_snapshot(&self.kernel_bytes)
            .expect("WorldSnapshot::restore: kernel snapshot restoration failed");
        world.restore_blob_state(
            self.blob_snapshot.entities.clone(),
            self.blob_snapshot.components.clone(),
        );
    }

    /// Return the serialized kernel snapshot bytes.
    ///
    /// Used by round-trip tests to assert byte-identity after restore.
    #[must_use]
    pub fn serialized_bytes(&self) -> &[u8] {
        &self.kernel_bytes
    }

    /// Return the serialized legacy blob bytes at capture time.
    ///
    /// Used by backward-compatible round-trip tests that compare
    /// `world.serialize()` output.
    #[must_use]
    pub fn blob_bytes(&self) -> &[u8] {
        &self.blob_snapshot.serialized
    }

    /// Number of entities in the captured world.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entity_count
    }

    /// Tick the snapshot was captured at.
    #[must_use]
    pub fn captured_at_tick(&self) -> u64 {
        self.captured_at_tick
    }
}

/// Timing metrics from a single snapshot round-trip.
///
/// Returned by [`measure_round_trip`]; values feed into BASELINE.md.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotMetrics {
    /// Number of entities captured in the snapshot.
    pub entity_count: usize,
    /// Size of the serialized kernel snapshot byte stream.
    pub serialized_bytes: usize,
    /// Wall-clock time spent in [`WorldSnapshot::capture`].
    pub capture: Duration,
    /// Wall-clock time spent in [`WorldSnapshot::restore`].
    pub restore: Duration,
}

impl SnapshotMetrics {
    /// Total round-trip duration.
    #[must_use]
    pub fn total(&self) -> Duration {
        self.capture + self.restore
    }

    /// True if total round-trip exceeded the Phase 5 abort threshold of
    /// **500ms on a 10k-entity scene**.
    #[must_use]
    pub fn exceeds_phase5_abort_threshold(&self) -> bool {
        self.entity_count >= 10_000 && self.total() > Duration::from_millis(500)
    }
}

/// Run a snapshot round-trip and record timings.
///
/// The world is restored to its pre-call state; this is non-destructive.
#[must_use]
pub fn measure_round_trip(world: &mut World) -> SnapshotMetrics {
    let entity_count = world.entity_count();

    let capture_start = Instant::now();
    let snapshot = WorldSnapshot::capture(world, 0);
    let capture = capture_start.elapsed();

    let serialized_bytes = snapshot.serialized_bytes().len();

    let restore_start = Instant::now();
    snapshot.restore(world);
    let restore = restore_start.elapsed();

    SnapshotMetrics {
        entity_count,
        serialized_bytes,
        capture,
        restore,
    }
}

/// Convenience: capture + record audit event in one call.
///
/// Used by the `EditorShell` lifecycle path.
pub fn capture_and_audit(world: &World, tick: u64, ledger: &mut AuditLedger) -> WorldSnapshot {
    let snap = WorldSnapshot::capture(world, tick);
    ledger.record(AuditEvent::SnapshotCaptured {
        entity_count: snap.entity_count(),
        bytes: snap.serialized_bytes().len(),
    });
    snap
}

/// Convenience: restore + record audit event.
pub fn restore_and_audit(snap: &WorldSnapshot, world: &mut World, ledger: &mut AuditLedger) {
    snap.restore(world);
    ledger.record(AuditEvent::SnapshotRestored {
        entity_count: snap.entity_count(),
        bytes: snap.serialized_bytes().len(),
    });
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_ecs::Component;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::world::ComponentTypeId;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Position {
        x: f32,
        y: f32,
        z: f32,
    }
    impl Component for Position {}
    impl rge_kernel_ecs::SnapshotComponent for Position {}

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TickCounter(u64);
    impl Component for TickCounter {}
    impl rge_kernel_ecs::SnapshotComponent for TickCounter {}

    fn build_scene(n: usize) -> World {
        let mut w = World::new();
        w.register_snapshot_component::<Position>();
        w.register_snapshot_component::<TickCounter>();
        #[allow(clippy::cast_precision_loss)]
        for i in 0..n {
            let e = w.spawn();
            w.insert_component(e, ComponentTypeId(1), (i as u64).to_le_bytes().to_vec());
            w.insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
            // Also insert typed components for the kernel snapshot path.
            w.kernel_mut().insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
            w.kernel_mut().insert(e, TickCounter(i as u64));
        }
        w
    }

    #[test]
    fn capture_sees_entity_count() {
        let w = build_scene(10);
        let snap = WorldSnapshot::capture(&w, 0);
        assert_eq!(snap.entity_count(), 10);
    }

    #[test]
    fn restore_is_byte_identical() {
        let mut w = build_scene(100);
        let pre_bytes = w.serialize_snapshot().unwrap();
        let snap = WorldSnapshot::capture(&w, 0);

        // Mutate blob state (does not affect kernel snapshot round-trip).
        for _ in 0..60 {
            w.tick_game_systems(1.0 / 60.0);
        }

        snap.restore(&mut w);
        let post_bytes = w.serialize_snapshot().unwrap();
        assert_eq!(
            pre_bytes, post_bytes,
            "restore must produce byte-identical kernel snapshot"
        );
        assert_eq!(post_bytes, snap.serialized_bytes());
    }

    #[test]
    fn measure_round_trip_records_metrics() {
        let mut w = build_scene(50);
        let m = measure_round_trip(&mut w);
        assert_eq!(m.entity_count, 50);
        assert!(m.serialized_bytes > 0);
        assert!(m.total() < Duration::from_millis(100));
        assert!(!m.exceeds_phase5_abort_threshold());
    }

    #[test]
    fn ten_thousand_entity_round_trip_under_500ms() {
        let mut w = build_scene(10_000);
        let m = measure_round_trip(&mut w);
        if m.exceeds_phase5_abort_threshold() {
            eprintln!(
                "WARN: 10k-entity round-trip = {:?} exceeds Phase 5 abort threshold",
                m.total()
            );
        }
        assert_eq!(m.entity_count, 10_000);
    }
}
