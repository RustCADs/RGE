//! Joint kinds → Rapier `ImpulseJoint`.
//!
//! Wave W11 only ships the **impulse joint** path (single-`DoF` and few-`DoF`
//! constraints between two bodies). Multi-body articulated joints
//! (`MultibodyJoint`) ship in a later wave once humanoid IK lands.
//!
//! ## Authoring vs. solver representation
//!
//! The ECS [`Joint`] component holds the *authoring* parameters. Insertion
//! into the world translates those into Rapier's parameterised
//! `ImpulseJointBuilder` form. We don't expose the Rapier handle outside this
//! crate; consumers reference joints through the [`JointHandle`] returned
//! from [`World`](crate::World)`::insert_joint`.

use rapier3d::dynamics::{
    FixedJointBuilder, ImpulseJointHandle, PrismaticJointBuilder, RevoluteJointBuilder,
    SphericalJointBuilder,
};
use rapier3d::math::Vector;
use serde::{Deserialize, Serialize};

use crate::world::{PhysicsHandle, World};

/// Joint kind; mirror of the ECS authoring vocabulary.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JointKind {
    /// Single-axis hinge. `axis` is the rotation axis in the local frame of
    /// body A.
    Revolute {
        /// Hinge axis (unit, will be normalised on insert).
        axis: [f32; 3],
        /// Optional `(min, max)` rotation in radians.
        limits: Option<(f32, f32)>,
    },
    /// Single-axis slider. `axis` is the slide direction.
    Prismatic {
        /// Slide axis.
        axis: [f32; 3],
        /// Optional `(min, max)` distance limits.
        limits: Option<(f32, f32)>,
    },
    /// Ball-and-socket. Three rotational `DoF`; no translational.
    Spherical,
    /// Welded (zero `DoF`).
    Fixed,
}

/// ECS joint component. Anchor points are in each body's local space.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    /// First body.
    pub body_a: u64,
    /// Second body.
    pub body_b: u64,
    /// Anchor in `body_a`'s local frame.
    pub anchor_a: [f32; 3],
    /// Anchor in `body_b`'s local frame.
    pub anchor_b: [f32; 3],
    /// What kind of joint.
    pub kind: JointKind,
}

/// Stable joint handle returned to consumers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct JointHandle {
    pub(crate) inner: ImpulseJointHandle,
    /// Stable u64 derived from the underlying arena index — used as the joint
    /// id in the audit ledger.
    pub id: u64,
}

impl World {
    /// Insert a joint between two existing bodies.
    ///
    /// Returns `None` if either body handle has been removed.
    pub fn insert_joint(
        &mut self,
        a: PhysicsHandle,
        b: PhysicsHandle,
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
        kind: JointKind,
    ) -> Option<JointHandle> {
        // Verify bodies exist.
        self.bodies.get(a.body)?;
        self.bodies.get(b.body)?;

        let pa = Vector::new(anchor_a[0], anchor_a[1], anchor_a[2]);
        let pb = Vector::new(anchor_b[0], anchor_b[1], anchor_b[2]);

        let inner = match kind {
            JointKind::Revolute { axis, limits } => {
                // rapier 0.32 takes the axis as a `Vector` (Vec3) directly and
                // normalises internally; the prior `UnitVector3` wrapper is
                // gone.
                let axis_v = Vector::new(axis[0], axis[1], axis[2]).normalize();
                let mut builder = RevoluteJointBuilder::new(axis_v)
                    .local_anchor1(pa)
                    .local_anchor2(pb);
                if let Some((min, max)) = limits {
                    builder = builder.limits([min, max]);
                }
                self.impulse_joints.insert(a.body, b.body, builder, true)
            }
            JointKind::Prismatic { axis, limits } => {
                let axis_v = Vector::new(axis[0], axis[1], axis[2]).normalize();
                let mut builder = PrismaticJointBuilder::new(axis_v)
                    .local_anchor1(pa)
                    .local_anchor2(pb);
                if let Some((min, max)) = limits {
                    builder = builder.limits([min, max]);
                }
                self.impulse_joints.insert(a.body, b.body, builder, true)
            }
            JointKind::Spherical => {
                let builder = SphericalJointBuilder::new()
                    .local_anchor1(pa)
                    .local_anchor2(pb);
                self.impulse_joints.insert(a.body, b.body, builder, true)
            }
            JointKind::Fixed => {
                let builder = FixedJointBuilder::new().local_anchor1(pa).local_anchor2(pb);
                self.impulse_joints.insert(a.body, b.body, builder, true)
            }
        };
        let id = u64::from(inner.into_raw_parts().0);
        Some(JointHandle { inner, id })
    }

    /// Remove a joint.
    pub fn remove_joint(&mut self, handle: JointHandle) {
        self.impulse_joints.remove(handle.inner, true);
    }

    /// Number of live joints. Test convenience.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.impulse_joints.iter().count()
    }
}
