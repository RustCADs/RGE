//! Rapier-backed physics world resource.
//!
//! Owns every Rapier piece that has to live for the duration of a sim:
//! `RigidBodySet`, `ColliderSet`, the broadphase/narrowphase/solver state, and
//! the integration parameters. One [`World`] per ECS world is the architectural
//! norm (PLAN.md §6.10) — multi-world scenarios use multiple `World`s with
//! distinct broadphases.
//!
//! ## Determinism
//!
//! The `enhanced-determinism` Cargo feature on `rapier3d` (set in this crate's
//! `Cargo.toml`) selects:
//!
//! - `BroadPhaseMultiSap` ordering that's bit-stable on the same architecture,
//! - the deterministic parallel solver (`solver::SolverDeterministic`),
//! - integration parameter defaults that don't depend on wall-clock.
//!
//! Combined with our fixed-dt step (see [`crate::step`]) this gives the
//! "Replay-Stable v1.0" guarantee per PLAN.md §1.6.8.

use rapier3d::dynamics::{
    CCDSolver, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, RigidBodyType,
};
use rapier3d::geometry::{ColliderBuilder, ColliderSet, DefaultBroadPhase, NarrowPhase};
use rapier3d::math::{Pose, Rotation, Vector};
use rapier3d::pipeline::PhysicsPipeline;

use crate::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};

/// Stable per-entity identity for cross-tick lookup. Wraps the Rapier
/// `RigidBodyHandle` plus the optional collider handle.
///
/// We don't expose the raw Rapier handle outside this crate — consumers
/// shouldn't need to reach into the internals, and hiding it gives us room
/// to change the back-end later.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhysicsHandle {
    /// Rapier body handle (opaque).
    pub(crate) body: RigidBodyHandle,
    /// Stable u64 identity, derived from the body handle's index. Used as the
    /// cross-reference key in the audit ledger so we don't bake Rapier's
    /// generational arena indexing into the recorded stream.
    pub(crate) id: u64,
}

impl PhysicsHandle {
    /// Stable per-body identity. Survives across reads of the same world.
    #[must_use]
    pub fn id(self) -> u64 {
        self.id
    }
}

/// Rapier physics world — single per-ECS-world resource.
pub struct World {
    /// Rapier rigid-body arena.
    pub bodies: RigidBodySet,
    /// Rapier collider arena.
    pub colliders: ColliderSet,
    /// Active-island tracking.
    pub islands: IslandManager,
    /// Spatial broadphase. We use the deterministic-feature-gated default.
    pub broadphase: DefaultBroadPhase,
    /// Narrow-phase contact manifold cache.
    pub narrowphase: NarrowPhase,
    /// Persistent impulse joints (single-DoF style).
    pub impulse_joints: ImpulseJointSet,
    /// Multibody joints (articulated chains).
    pub multibody_joints: MultibodyJointSet,
    /// Continuous-collision-detection solver.
    pub ccd: CCDSolver,
    /// Integration parameters (dt, solver iters, etc.).
    pub params: IntegrationParameters,
    /// Step pipeline. Reused across ticks.
    pub pipeline: PhysicsPipeline,
    /// Gravity in m/s². Default `-9.81 ŷ`.
    pub gravity: Vector,
    /// Monotonic tick index since world construction.
    pub tick: u64,
}

impl World {
    /// Construct an empty world with default earth gravity and the 60 Hz fixed
    /// timestep configured.
    #[must_use]
    pub fn new() -> Self {
        let params = IntegrationParameters {
            dt: crate::step::FIXED_DT,
            ..IntegrationParameters::default()
        };
        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            islands: IslandManager::new(),
            broadphase: DefaultBroadPhase::new(),
            narrowphase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            params,
            pipeline: PhysicsPipeline::new(),
            gravity: Vector::new(0.0, -9.81, 0.0),
            tick: 0,
        }
    }

    /// Insert a body + optional collider, returning the stable handle.
    ///
    /// Position is supplied in world space. Use [`crate::sync::pre_physics`]
    /// for ongoing updates.
    pub fn insert_body(
        &mut self,
        rigid: RigidBody,
        collider: Option<Collider>,
        position: [f32; 3],
        rotation: [f32; 4],
    ) -> PhysicsHandle {
        let body_type = match rigid.kind {
            BodyKind::Dynamic => RigidBodyType::Dynamic,
            BodyKind::Fixed => RigidBodyType::Fixed,
            BodyKind::KinematicPositionBased => RigidBodyType::KinematicPositionBased,
            BodyKind::KinematicVelocityBased => RigidBodyType::KinematicVelocityBased,
        };
        let translation = Vector::new(position[0], position[1], position[2]);
        let q = Rotation::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
        let pose = Pose::from_parts(translation, q);

        let mut builder = RigidBodyBuilder::new(body_type)
            .pose(pose)
            .linear_damping(rigid.linear_damping)
            .angular_damping(rigid.angular_damping)
            .can_sleep(!rigid.never_sleep);
        if rigid.kind == BodyKind::Dynamic && rigid.mass > 0.0 {
            builder = builder.additional_mass(rigid.mass);
        }
        let body_handle = self.bodies.insert(builder.build());

        if let Some(c) = collider {
            let shape_builder = match c.shape {
                ColliderShape::Cuboid { hx, hy, hz } => ColliderBuilder::cuboid(hx, hy, hz),
                ColliderShape::Ball { radius } => ColliderBuilder::ball(radius),
                ColliderShape::Capsule {
                    half_height,
                    radius,
                } => ColliderBuilder::capsule_y(half_height, radius),
                // Plane: model as a thin, large cuboid centred at the body
                // origin. Real "halfspace" colliders exist in Rapier but using
                // a flat cuboid lets cuboid<->cuboid contacts use the same
                // narrow-phase path, which keeps the determinism story simple.
                ColliderShape::Plane => ColliderBuilder::cuboid(500.0, 0.05, 500.0),
            };
            let collider = shape_builder
                .density(c.density)
                .friction(c.friction)
                .restitution(c.restitution)
                .sensor(c.is_sensor)
                .active_events(rapier3d::pipeline::ActiveEvents::COLLISION_EVENTS)
                .build();
            self.colliders
                .insert_with_parent(collider, body_handle, &mut self.bodies);
        }

        // Stable id == arena index. Rapier reuses indices on remove+insert
        // (with bumped generation) but the determinism contract forbids
        // remove-during-replay so this is the simplest stable identity.
        let id = u64::from(body_handle.into_raw_parts().0);
        PhysicsHandle {
            body: body_handle,
            id,
        }
    }

    /// Remove a body and its colliders.
    pub fn remove_body(&mut self, handle: PhysicsHandle) {
        self.bodies.remove(
            handle.body,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true, // wake_sleeping_neighbors
        );
    }

    /// Read the current world-space position + orientation of a body.
    ///
    /// Returns `None` if the handle has been removed.
    #[must_use]
    pub fn body_pose(&self, handle: PhysicsHandle) -> Option<([f32; 3], [f32; 4])> {
        let b = self.bodies.get(handle.body)?;
        let t = b.position().translation;
        let r = b.position().rotation;
        // Quaternion convention: [x, y, z, w] for the ECS side.
        Some(([t.x, t.y, t.z], [r.x, r.y, r.z, r.w]))
    }

    /// Read linear + angular velocity of a body.
    #[must_use]
    pub fn body_velocity(&self, handle: PhysicsHandle) -> Option<([f32; 3], [f32; 3])> {
        let b = self.bodies.get(handle.body)?;
        let l = b.linvel();
        let a = b.angvel();
        Some(([l.x, l.y, l.z], [a.x, a.y, a.z]))
    }

    /// Whether the body is currently sleeping (resting).
    pub fn is_body_sleeping(&self, handle: PhysicsHandle) -> bool {
        self.bodies
            .get(handle.body)
            .is_some_and(rapier3d::dynamics::RigidBody::is_sleeping)
    }

    /// Number of active (non-sleeping) bodies. Test convenience.
    #[must_use]
    pub fn active_body_count(&self) -> usize {
        self.bodies.iter().filter(|(_, b)| !b.is_sleeping()).count()
    }

    /// Number of bodies (active + sleeping).
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bodies.iter().count()
    }

    /// Serialise the deterministic-relevant slice of state for replay
    /// equality.
    ///
    /// We do **not** serialise Rapier's full internal state (broadphase trees,
    /// island manager, etc.) — that pulls in too much surface area and most
    /// of it is just an index over the body set. For the replay-equality
    /// contract we hash the per-body `(position, rotation, linvel, angvel,
    /// sleeping)` tuple in stable handle order. Two runs that diverge in any
    /// of those will produce different bytes; two runs that match will produce
    /// the same bytes.
    #[must_use]
    #[allow(
        clippy::type_complexity,
        reason = "local-only ad-hoc tuple is clearer than naming a one-shot record type for the per-body (idx, pos, rot, linvel, angvel, sleep) row"
    )]
    pub fn serialize_state(&self) -> Vec<u8> {
        // Stable order: collect (raw_index, slot) pairs and sort. Rapier's
        // arena reuses indices on remove+insert, but within a single run
        // without removals the order is monotonic. For replay purposes both
        // runs walk the same sequence of inserts so the order matches.
        let mut entries: Vec<(u32, [f32; 3], [f32; 4], [f32; 3], [f32; 3], u8)> = self
            .bodies
            .iter()
            .map(|(h, b)| {
                let t = b.position().translation;
                let r = b.position().rotation;
                let l = b.linvel();
                let a = b.angvel();
                (
                    h.into_raw_parts().0,
                    [t.x, t.y, t.z],
                    [r.x, r.y, r.z, r.w],
                    [l.x, l.y, l.z],
                    [a.x, a.y, a.z],
                    u8::from(b.is_sleeping()),
                )
            })
            .collect();
        entries.sort_by_key(|e| e.0);

        let mut out = Vec::with_capacity(entries.len() * 64 + 16);
        out.extend_from_slice(&self.tick.to_le_bytes());
        let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for (idx, t, r, l, a, sleep) in entries {
            out.extend_from_slice(&idx.to_le_bytes());
            for f in t
                .iter()
                .chain(r.iter())
                .chain(l.iter())
                .chain(a.iter())
                .copied()
            {
                let val: f32 = f;
                out.extend_from_slice(&val.to_le_bytes());
            }
            out.push(sleep);
        }
        out
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Bodies / active counts are derived; the rest of Rapier's internals
        // don't have meaningful Debug output for human readers.
        f.debug_struct("World")
            .field("tick", &self.tick)
            .field("bodies", &self.body_count())
            .field("active", &self.active_body_count())
            .field("gravity", &self.gravity)
            .finish_non_exhaustive()
    }
}
