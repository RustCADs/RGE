//! Integration tests for `participate` — [`PieSnapshot`] + [`SnapshotParticipate`].
//!
//! These tests exercise the full capture/restore cycle including the ECS world
//! layer (via stub components) and multiple participant payloads, verifying
//! byte-identity and error paths per the Phase 6.13 spec.

use rge_kernel_ecs::participate::{
    ParticipantId, ParticipateError, PieSnapshot, SnapshotParticipate,
};
use rge_kernel_ecs::{Component, SnapshotComponent, World};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Stub participants
// ---------------------------------------------------------------------------

/// A participant whose state is a single `u64` counter.
struct CounterParticipant {
    id: ParticipantId,
    value: u64,
}

impl SnapshotParticipate for CounterParticipant {
    fn participant_id(&self) -> ParticipantId {
        self.id.clone()
    }

    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        Ok(self.value.to_le_bytes().to_vec())
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| ParticipateError::Custom("expected 8 bytes for u64".into()))?;
        self.value = u64::from_le_bytes(arr);
        Ok(())
    }
}

/// A participant whose state is a deterministic `Vec<String>`.
///
/// Uses `postcard` for the inner payload so it stays in the crate's existing
/// dependency set and avoids introducing `ron` directly in the test.
struct StringListParticipant {
    id: ParticipantId,
    lines: Vec<String>,
}

impl SnapshotParticipate for StringListParticipant {
    fn participant_id(&self) -> ParticipantId {
        self.id.clone()
    }

    /// Simple deterministic encoding: 4-byte LE count, then per string:
    /// 4-byte LE len + UTF-8 bytes.
    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        let mut buf = Vec::new();
        #[allow(clippy::cast_possible_truncation)]
        let count = self.lines.len() as u32;
        buf.extend_from_slice(&count.to_le_bytes());
        for s in &self.lines {
            let b = s.as_bytes();
            #[allow(clippy::cast_possible_truncation)]
            let len = b.len() as u32;
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(b);
        }
        Ok(buf)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let mut pos = 0usize;

        macro_rules! need {
            ($n:expr) => {{
                let end = pos + $n;
                if end > bytes.len() {
                    return Err(ParticipateError::Custom(format!(
                        "truncated at offset {pos}"
                    )));
                }
                let s = &bytes[pos..end];
                pos = end;
                s
            }};
        }

        let count_bytes = need!(4);
        let count = u32::from_le_bytes([
            count_bytes[0],
            count_bytes[1],
            count_bytes[2],
            count_bytes[3],
        ]) as usize;
        let mut lines = Vec::with_capacity(count);
        for _ in 0..count {
            let len_bytes = need!(4);
            let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]])
                as usize;
            let s_bytes = need!(len);
            let s = std::str::from_utf8(s_bytes)
                .map_err(|e| ParticipateError::Custom(e.to_string()))?
                .to_string();
            lines.push(s);
        }
        self.lines = lines;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixture component — used in pie_snapshot_with_real_ecs_world
// ---------------------------------------------------------------------------

/// A 3-D position component that participates in ECS snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

impl Component for Position {}
impl SnapshotComponent for Position {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Capture with two participants, capture again — bytes byte-identical.
/// Mutate state, restore, verify original values come back.
#[test]
fn pie_round_trip_byte_identical() {
    let world = World::new(); // empty world, no registered components

    let counter_a = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 42,
    };
    let strings_a = StringListParticipant {
        id: ParticipantId::new("physics.test-strings"),
        lines: vec!["a".into(), "b".into()],
    };

    // Capture twice — bytes must be byte-identical.
    let snap1 = PieSnapshot::capture(
        &world,
        &[
            &counter_a as &dyn SnapshotParticipate,
            &strings_a as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture1");
    let snap2 = PieSnapshot::capture(
        &world,
        &[
            &counter_a as &dyn SnapshotParticipate,
            &strings_a as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture2");
    assert_eq!(
        snap1.to_bytes(),
        snap2.to_bytes(),
        "two captures of identical state must produce byte-identical output"
    );

    // Mutate state in fresh instances.
    let mut counter_b = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 999,
    };
    let mut strings_b = StringListParticipant {
        id: ParticipantId::new("physics.test-strings"),
        lines: vec![],
    };

    let mut world_after = World::new();
    let id_counter = counter_b.participant_id();
    let id_strings = strings_b.participant_id();
    snap1
        .restore(
            &mut world_after,
            &mut [
                (&id_counter, &mut counter_b as &mut dyn SnapshotParticipate),
                (&id_strings, &mut strings_b as &mut dyn SnapshotParticipate),
            ],
        )
        .expect("restore");

    assert_eq!(counter_b.value, 42, "counter must be restored to 42");
    assert_eq!(
        strings_b.lines,
        vec!["a".to_string(), "b".to_string()],
        "string list must be restored"
    );
}

/// Build a snapshot, serialize via `to_bytes`, parse via `from_bytes`, compare.
#[test]
fn pie_snapshot_envelope_round_trip() {
    let world = World::new();
    let counter = CounterParticipant {
        id: ParticipantId::new("test.counter"),
        value: 12_345_678,
    };
    let snap =
        PieSnapshot::capture(&world, &[&counter as &dyn SnapshotParticipate]).expect("capture");

    let bytes = snap.to_bytes();
    let recovered = PieSnapshot::from_bytes(&bytes).expect("from_bytes");
    assert_eq!(snap, recovered, "envelope round-trip must be lossless");

    // Serializing the recovered snapshot must also be byte-identical.
    assert_eq!(
        bytes,
        recovered.to_bytes(),
        "recovered → to_bytes must equal original bytes"
    );
}

/// Capture with [counter, strings]. Restore with only [counter].
/// EXPECTED: error `UnknownParticipant("physics.test-strings")`.
#[test]
fn pie_snapshot_unknown_participant_at_restore() {
    let world = World::new();
    let counter = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 1,
    };
    let strings = StringListParticipant {
        id: ParticipantId::new("physics.test-strings"),
        lines: vec!["x".into()],
    };

    let snap = PieSnapshot::capture(
        &world,
        &[
            &counter as &dyn SnapshotParticipate,
            &strings as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture");

    // Restore with only the counter handler — strings handler is absent.
    let mut counter_b = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 0,
    };
    let mut world_b = World::new();
    let id_counter = counter_b.participant_id();
    let err = snap
        .restore(
            &mut world_b,
            &mut [(&id_counter, &mut counter_b as &mut dyn SnapshotParticipate)],
        )
        .unwrap_err();

    assert!(
        matches!(
            &err,
            ParticipateError::UnknownParticipant(id) if id.as_str() == "physics.test-strings"
        ),
        "expected UnknownParticipant(physics.test-strings), got {err:?}"
    );
}

/// Capture with [counter] only. Restore with [counter, strings] — strings
/// stays at its default state; no error.
#[test]
fn pie_snapshot_extra_participant_at_restore_is_fine() {
    let world = World::new();
    let counter = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 77,
    };

    let snap =
        PieSnapshot::capture(&world, &[&counter as &dyn SnapshotParticipate]).expect("capture");

    let mut counter_b = CounterParticipant {
        id: ParticipantId::new("audio.test-counter"),
        value: 0,
    };
    // Extra participant not in snapshot — starts at default (empty lines).
    let mut strings_b = StringListParticipant {
        id: ParticipantId::new("physics.test-strings"),
        lines: vec!["existing".into()],
    };

    let mut world_b = World::new();
    let id_counter = counter_b.participant_id();
    let id_strings = strings_b.participant_id();
    snap.restore(
        &mut world_b,
        &mut [
            (&id_counter, &mut counter_b as &mut dyn SnapshotParticipate),
            (&id_strings, &mut strings_b as &mut dyn SnapshotParticipate),
        ],
    )
    .expect("restore must succeed even with extra handler");

    assert_eq!(counter_b.value, 77, "counter restored from snapshot");
    // strings_b was NOT in the snapshot; it must be untouched.
    assert_eq!(
        strings_b.lines,
        vec!["existing".to_string()],
        "extra participant not in snapshot must be left untouched"
    );
}

/// Capture with two participants sharing the same [`ParticipantId`].
/// EXPECTED: error `DuplicateParticipant`.
#[test]
fn pie_snapshot_duplicate_participant_id_errors() {
    let world = World::new();
    let p1 = CounterParticipant {
        id: ParticipantId::new("shared.id"),
        value: 1,
    };
    let p2 = CounterParticipant {
        id: ParticipantId::new("shared.id"),
        value: 2,
    };

    let err = PieSnapshot::capture(
        &world,
        &[
            &p1 as &dyn SnapshotParticipate,
            &p2 as &dyn SnapshotParticipate,
        ],
    )
    .unwrap_err();

    assert!(
        matches!(
            &err,
            ParticipateError::DuplicateParticipant(id) if id.as_str() == "shared.id"
        ),
        "expected DuplicateParticipant(shared.id), got {err:?}"
    );
}

/// Define a `Position` component, spawn 5 entities, capture via
/// `PieSnapshot::capture`, mutate + despawn all, restore, verify values match
/// originals. This exercises both the ECS world snapshot layer AND the
/// participant payload layer together.
#[test]
fn pie_snapshot_with_real_ecs_world() {
    let mut world = World::new();
    world.register_snapshot_component::<Position>();

    // A participant that tracks a separate counter alongside the ECS state.
    let participant = CounterParticipant {
        id: ParticipantId::new("test.counter"),
        value: 100,
    };

    // Spawn 5 entities with distinct positions.
    #[allow(clippy::cast_precision_loss)]
    for i in 0..5u32 {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: i as f32 * 2.0,
                z: 0.0,
            },
        );
    }
    assert_eq!(world.entity_count(), 5);

    // Capture.
    let snap = PieSnapshot::capture(&world, &[&participant as &dyn SnapshotParticipate])
        .expect("capture with 5 entities");

    // Mutate world: despawn all by restoring a blank world, then re-check.
    {
        let all: Vec<_> = world.query::<Position>().map(|(id, _)| id).collect();
        for e in all {
            world.despawn(e);
        }
    }
    assert_eq!(world.entity_count(), 0, "all entities despawned");

    // Restore.
    let mut counter_after = CounterParticipant {
        id: ParticipantId::new("test.counter"),
        value: 999,
    };
    let id_counter = counter_after.participant_id();
    snap.restore(
        &mut world,
        &mut [(
            &id_counter,
            &mut counter_after as &mut dyn SnapshotParticipate,
        )],
    )
    .expect("restore with 5 entities");

    // ECS layer: 5 entities with original positions.
    assert_eq!(world.entity_count(), 5, "5 entities restored");
    let mut xs: Vec<f32> = world.query::<Position>().map(|(_, p)| p.x).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    #[allow(clippy::cast_precision_loss)]
    for (i, &x) in xs.iter().enumerate() {
        assert!(
            (x - i as f32).abs() < 1e-6,
            "entity {i}: expected x={i}, got {x}"
        );
    }

    // Participant layer: counter restored to original.
    assert_eq!(
        counter_after.value, 100,
        "counter participant restored from snapshot"
    );
}

/// Registration order of participants must not affect the byte output —
/// sorted-by-id output is deterministic regardless.
#[test]
fn pie_snapshot_registration_order_does_not_affect_bytes() {
    let world = World::new();
    let counter = CounterParticipant {
        id: ParticipantId::new("zzz.counter"),
        value: 7,
    };
    let strings = StringListParticipant {
        id: ParticipantId::new("aaa.strings"),
        lines: vec!["hello".into()],
    };

    // Capture in one order.
    let snap_ab = PieSnapshot::capture(
        &world,
        &[
            &counter as &dyn SnapshotParticipate,
            &strings as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture ab");

    // Capture in reversed order.
    let snap_ba = PieSnapshot::capture(
        &world,
        &[
            &strings as &dyn SnapshotParticipate,
            &counter as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture ba");

    assert_eq!(
        snap_ab.to_bytes(),
        snap_ba.to_bytes(),
        "registration order must not affect serialized bytes"
    );
}
