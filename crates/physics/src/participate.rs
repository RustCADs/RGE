//! `SnapshotParticipate` implementation for [`World`].
//!
//! Failure class: snapshot-recoverable (mirrors the crate-level declaration
//! at `crates/physics/src/lib.rs`).
//!
//! Closes one of the five PLAN §13.2 v1.0-gate TODOs flagged by the
//! `snapshot-participate` supplementary architecture lint
//! (`tools/architecture-lints/src/snapshot_participate.rs`). Without this
//! impl, PIE snapshots captured the cad-core / cad-projection participants
//! but NOT the physics simulation state — replaying a saved scene would
//! restart all rigid-body trajectories from scratch (rapier `World::new()`
//! defaults), silently diverging from the captured tick.
//!
//! # Wire format
//!
//! Capture/restore use **postcard** — the workspace default per
//! `kernel/ecs/src/participate.rs` "Serialization-format policy" doc-comment.
//! Physics state contains no internally-tagged enums (the `cad-core`
//! exception that pushed `cad-graph` to RON), so postcard is the right
//! choice: compact, fast, and the format already ships in the workspace
//! `[dependencies]` table for the `cad-projection` participant.
//!
//! Rapier types serialize via the `serde-serialize` Cargo feature, which
//! gates `derive(Serialize, Deserialize)` on `RigidBodySet`, `ColliderSet`,
//! `IslandManager`, `DefaultBroadPhase` (= `BroadPhaseBvh`), `NarrowPhase`,
//! `ImpulseJointSet`, `MultibodyJointSet`, `CCDSolver`, and
//! `IntegrationParameters`. The feature is enabled in this crate's
//! `Cargo.toml` alongside `enhanced-determinism` (the two are not mutually
//! exclusive — only the SIMD ↔ enhanced-determinism combo is forbidden by
//! `rapier3d/src/lib.rs`).
//!
//! # What's captured / what's reconstructed
//!
//! Every `World` field is captured **except** [`PhysicsPipeline`]: rapier's
//! own source comment (`pipeline/physics_pipeline.rs:39`) reads "this
//! contains only workspace data, so there is no point in making this
//! serializable." The pipeline is reconstructed via `PhysicsPipeline::new()`
//! at restore time. This matches the rapier-recommended snapshot pattern
//! documented at <https://rapier.rs/docs/user_guides/rust/serialization/>.
//!
//! Captured fields (mirrors `World` struct definition):
//!
//! | Field | Type | Why captured |
//! |---|---|---|
//! | `bodies` | `RigidBodySet` | Per-body pose / velocity / mass / damping — the core sim state |
//! | `colliders` | `ColliderSet` | Per-collider shape / friction / restitution / sensor flags |
//! | `islands` | `IslandManager` | Active-island tracking; needed so post-restore step doesn't wake spuriously |
//! | `broadphase` | `DefaultBroadPhase` | Spatial-pair cache; expensive to rebuild from scratch |
//! | `narrowphase` | `NarrowPhase` | Contact manifolds; restoring preserves friction/restitution accumulators |
//! | `impulse_joints` | `ImpulseJointSet` | Single-DoF joints |
//! | `multibody_joints` | `MultibodyJointSet` | Articulated chains |
//! | `ccd` | `CCDSolver` | Currently a unit struct but captured for forward-compat |
//! | `params` | `IntegrationParameters` | dt, solver iterations, etc. |
//! | `gravity` | `Vector` (nalgebra `Vector3<f32>`) | Per-world gravity override |
//! | `tick` | `u64` | Monotonic tick index — restoring drops back to the captured tick |
//!
//! Reconstructed (NOT captured):
//!
//! | Field | Reconstruction |
//! |---|---|
//! | `pipeline` | `PhysicsPipeline::new()` — workspace-data-only per rapier's own comment |
//!
//! # Determinism
//!
//! The `enhanced-determinism` feature pins broadphase ordering and solver
//! iteration order to bit-stable on the same architecture (per the
//! `crate::world` module-level doc and PLAN §1.6.8). Postcard's wire format
//! is fully deterministic — no map iteration, no float text formatting, no
//! cross-platform endianness divergence. So `capture` -> `restore` -> `capture`
//! produces byte-identical bytes assuming no intervening mutations.
//!
//! # Convention
//!
//! Callers SHOULD register `World` alongside whatever ECS-side `Transform`
//! data the projection layer holds. This impl restores the rapier internal
//! state; the caller's downstream `Transform` components are restored via
//! the ECS `world_bytes` layer of the `PieSnapshot` envelope.

use rapier3d::dynamics::{
    CCDSolver, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet,
    RigidBodySet,
};
use rapier3d::geometry::{ColliderSet, DefaultBroadPhase, NarrowPhase};
use rapier3d::math::Vector;
use rapier3d::pipeline::PhysicsPipeline;
use rge_kernel_ecs::participate::{ParticipantId, ParticipateError, SnapshotParticipate};
use serde::{Deserialize, Serialize};

use crate::world::World;

/// Stable participant id for [`World`] in PIE snapshots.
///
/// Naming follows the `<crate-name>.<subsystem>` convention documented in
/// `kernel/ecs/src/participate.rs`. The `rapier-rigid-bodies` qualifier
/// matches the anticipated id documented in `docs/§18/PIE_SNAPSHOT.md` §11
/// "Future participants".
pub const PHYSICS_WORLD_PARTICIPANT_ID: &str = "physics.rapier-rigid-bodies";

/// Wire-format payload captured / restored by [`World`]'s
/// [`SnapshotParticipate`] impl.
///
/// Field order mirrors [`World`] minus the non-serializable
/// [`PhysicsPipeline`] field. See module-level doc for the field-by-field
/// rationale.
///
/// # Determinism
///
/// All field types derive `Serialize` / `Deserialize` via rapier3d's
/// `serde-serialize` feature gate. The serializer (postcard) is fully
/// deterministic given identical input, so two captures of byte-equal world
/// state always produce byte-equal payloads.
#[derive(Serialize, Deserialize)]
struct WorldPayload {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    islands: IslandManager,
    broadphase: DefaultBroadPhase,
    narrowphase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    params: IntegrationParameters,
    gravity: Vector,
    tick: u64,
}

impl SnapshotParticipate for World {
    fn participant_id(&self) -> ParticipantId {
        ParticipantId::new(PHYSICS_WORLD_PARTICIPANT_ID)
    }

    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        // Note: every captured field is `Clone` — the alternatives would be
        // (a) consume `self` (which would break the trait signature) or
        // (b) write a borrowed-fields wrapper struct (which would double
        // the impl surface). Cloning happens once per snapshot and the
        // sets reuse arena-backed storage, so the cost is negligible.
        let payload = WorldPayload {
            bodies: self.bodies.clone(),
            colliders: self.colliders.clone(),
            islands: self.islands.clone(),
            broadphase: self.broadphase.clone(),
            narrowphase: self.narrowphase.clone(),
            impulse_joints: self.impulse_joints.clone(),
            multibody_joints: self.multibody_joints.clone(),
            ccd: self.ccd.clone(),
            params: self.params,
            gravity: self.gravity,
            tick: self.tick,
        };
        postcard::to_allocvec(&payload).map_err(|e| ParticipateError::CaptureFailed {
            id: self.participant_id(),
            message: format!("postcard serialize physics::World: {e}"),
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let payload: WorldPayload =
            postcard::from_bytes(bytes).map_err(|e| ParticipateError::RestoreFailed {
                id: self.participant_id(),
                message: format!("postcard deserialize physics::World: {e}"),
            })?;

        // Replace every captured field. `pipeline` is reconstructed via
        // `PhysicsPipeline::new()` per the rapier-snapshot pattern (its
        // contents are workspace-only — no persistent state).
        self.bodies = payload.bodies;
        self.colliders = payload.colliders;
        self.islands = payload.islands;
        self.broadphase = payload.broadphase;
        self.narrowphase = payload.narrowphase;
        self.impulse_joints = payload.impulse_joints;
        self.multibody_joints = payload.multibody_joints;
        self.ccd = payload.ccd;
        self.params = payload.params;
        self.gravity = payload.gravity;
        self.tick = payload.tick;
        self.pipeline = PhysicsPipeline::new();

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — round-trip + PIE-aggregator integration
//
// The substrate goal: physics `World` round-trips losslessly through the
// `SnapshotParticipate` trait so PIE snapshots include physics state
// alongside cad-core / cad-projection — closing one of the five v1.0
// missing-impl flags surfaced by the snapshot-participate lint.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_ecs::participate::SnapshotParticipate;
    use rge_kernel_ecs::{ParticipantId, PieSnapshot, World as EcsWorld};

    use super::PHYSICS_WORLD_PARTICIPANT_ID;
    use crate::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
    use crate::world::World;

    /// Build a small populated `World` with a fixed body + a dynamic cube
    /// stacked above it, deterministic across runs.
    fn populated_world() -> World {
        let mut w = World::new();
        // Ground.
        w.insert_body(
            RigidBody {
                kind: BodyKind::Fixed,
                ..RigidBody::default()
            },
            Some(Collider {
                shape: ColliderShape::Plane,
                ..Collider::default()
            }),
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        // Cube at y=2.0.
        w.insert_body(
            RigidBody {
                kind: BodyKind::Dynamic,
                mass: 1.0,
                ..RigidBody::default()
            },
            Some(Collider {
                shape: ColliderShape::Cuboid {
                    hx: 0.5,
                    hy: 0.5,
                    hz: 0.5,
                },
                ..Collider::default()
            }),
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        w
    }

    /// `participant_id` returns the documented stable string.
    #[test]
    fn physics_snapshot_participate_id_is_stable() {
        let w = World::new();
        let id = w.participant_id();
        assert_eq!(id.as_str(), PHYSICS_WORLD_PARTICIPANT_ID);
        assert_eq!(id.as_str(), "physics.rapier-rigid-bodies");
        // Calling twice on the same instance returns identical id (trait
        // contract per `kernel/ecs/src/participate.rs`).
        assert_eq!(w.participant_id(), w.participant_id());
    }

    /// Empty `World::new()` capture/restore round-trips: serialize_state
    /// digest matches before and after restore on a fresh instance.
    #[test]
    fn physics_snapshot_participate_round_trip_empty_world() {
        let original = World::new();
        let bytes = original.capture().expect("capture empty");

        // Restore into a fresh world that's already had a body inserted —
        // proves restore is a clean overwrite, not a merge.
        let mut fresh = populated_world();
        assert_eq!(fresh.body_count(), 2, "fresh has bodies before restore");
        fresh.restore(&bytes).expect("restore");
        assert_eq!(
            fresh.body_count(),
            0,
            "restore overwrites — fresh now has zero bodies"
        );
        assert_eq!(fresh.tick, 0, "tick reverts to original (=0)");

        // Digest equality: `serialize_state` is the existing replay-equality
        // digest (`crate::world::World::serialize_state`). Two worlds with
        // byte-equal state produce byte-equal digests.
        assert_eq!(
            original.serialize_state(),
            fresh.serialize_state(),
            "post-restore digest matches original empty world"
        );
    }

    /// Populated `World` capture/restore round-trips with byte-identical
    /// post-restore state per the `serialize_state` replay-digest contract.
    /// Uses BLAKE3 hash on the capture output before + after restore to
    /// detect any encoding non-determinism.
    #[test]
    fn physics_snapshot_participate_round_trip_populated_world() {
        let original = populated_world();
        let bytes_before = original.capture().expect("capture original");
        let hash_before = blake3::hash(&bytes_before);

        // Restore into a different world (same structural seed via
        // `populated_world()`, but mutated post-construction so the
        // `serialize_state` digest WILL diverge if restore is broken).
        let mut fresh = populated_world();
        // Mutate fresh: bump tick to prove restore overwrites it.
        fresh.tick = 999;
        fresh.restore(&bytes_before).expect("restore");
        assert_eq!(fresh.tick, original.tick, "tick reverted via restore");
        assert_eq!(
            fresh.body_count(),
            original.body_count(),
            "body count matches original"
        );

        // Re-capture the restored world; bytes must be byte-identical
        // (post-restore `serialize_state` digest should match too).
        let bytes_after = fresh.capture().expect("capture fresh");
        let hash_after = blake3::hash(&bytes_after);
        assert_eq!(
            hash_before, hash_after,
            "BLAKE3 of capture before vs after restore must match"
        );
        assert_eq!(
            bytes_before, bytes_after,
            "capture bytes must be byte-identical across round-trip"
        );
        assert_eq!(
            original.serialize_state(),
            fresh.serialize_state(),
            "post-restore replay-digest matches original"
        );
    }

    /// Restore handles malformed payload gracefully — returns
    /// `ParticipateError::RestoreFailed` rather than panicking.
    #[test]
    fn physics_snapshot_participate_restore_rejects_malformed_payload() {
        let mut w = World::new();
        let err = w.restore(&[0xFFu8; 16]).expect_err("malformed must err");
        let msg = err.to_string();
        assert!(
            msg.contains("postcard deserialize physics::World")
                || msg.contains("physics.rapier-rigid-bodies"),
            "unexpected error: {msg}"
        );
    }

    /// Full PIE round-trip via `PieSnapshot::capture`/`restore`. Wraps
    /// `World` as a participant; restores into a fresh ECS world + fresh
    /// physics `World`. The physics state survives the full `PieSnapshot`
    /// envelope, not just direct postcard.
    #[test]
    fn physics_snapshot_participate_pie_integration() {
        let physics = populated_world();
        let original_digest = physics.serialize_state();
        let original_body_count = physics.body_count();

        // Capture via PieSnapshot — the ECS world has no entities; physics
        // is the sole participant.
        let ecs = EcsWorld::new();
        let snap = PieSnapshot::capture(&ecs, &[&physics as &dyn SnapshotParticipate])
            .expect("pie capture");
        assert_eq!(snap.participants.len(), 1, "exactly one participant");
        let pid = ParticipantId::new(PHYSICS_WORLD_PARTICIPANT_ID);
        assert!(
            snap.participants.contains_key(&pid),
            "physics.rapier-rigid-bodies participant present"
        );

        // Restore into a fresh ECS world + fresh physics World.
        let mut fresh_ecs = EcsWorld::new();
        let mut fresh_physics = World::new();
        snap.restore(
            &mut fresh_ecs,
            &mut [(&pid, &mut fresh_physics as &mut dyn SnapshotParticipate)],
        )
        .expect("pie restore");

        // Physics state recovered.
        assert_eq!(
            fresh_physics.body_count(),
            original_body_count,
            "body count preserved through PIE envelope"
        );
        assert_eq!(
            fresh_physics.serialize_state(),
            original_digest,
            "post-restore physics digest matches original"
        );
    }

    /// Envelope round-trip via `to_bytes` + `from_bytes`: the full RGEP
    /// envelope is byte-stable across two captures of byte-equal physics
    /// state, and a from_bytes/restore cycle produces a working physics
    /// world.
    #[test]
    fn physics_snapshot_participate_envelope_round_trip() {
        let physics = populated_world();

        let ecs = EcsWorld::new();
        let snap1 =
            PieSnapshot::capture(&ecs, &[&physics as &dyn SnapshotParticipate]).expect("capture1");
        let bytes1 = snap1.to_bytes();

        // Round-trip via from_bytes.
        let snap2 = PieSnapshot::from_bytes(&bytes1).expect("from_bytes");
        let bytes2 = snap2.to_bytes();
        assert_eq!(
            bytes1, bytes2,
            "envelope bytes byte-identical after to_bytes/from_bytes/to_bytes"
        );

        // Restore from the rehydrated envelope.
        let pid = ParticipantId::new(PHYSICS_WORLD_PARTICIPANT_ID);
        let mut fresh_ecs = EcsWorld::new();
        let mut fresh_physics = World::new();
        snap2
            .restore(
                &mut fresh_ecs,
                &mut [(&pid, &mut fresh_physics as &mut dyn SnapshotParticipate)],
            )
            .expect("pie restore");
        assert_eq!(
            fresh_physics.serialize_state(),
            physics.serialize_state(),
            "envelope round-trip preserves physics digest"
        );
    }
}
