//! `cad_core::operators` — operator type system + concrete operator
//! implementations.
//!
//! Failure class: snapshot-recoverable
//!
//! # Design
//!
//! * [`Operator`] trait — uniform contract every operator implements.
//! * [`OperatorNode`] enum — tagged union the operator graph stores; preserves
//!   serde round-trip via `#[serde(tag = "kind")]`.
//! * [`OpKind`] — discriminant enum, lightweight metadata.
//! * [`EdgeKind`] — typed edge payload identifying the input port at which
//!   the upstream node's tessellation feeds the downstream operator.
//!
//! Phase 7.1 D-prime shipped [`CuboidOp`] and [`TransformOp`]; Phase 7
//! D-Extrude added [`ExtrudeOp`] (with [`Polygon2D`] profile); Phase 7
//! D-Revolve added [`RevolveOp`] (sweep around Y-axis); Phase 7 D-Boolean
//! adds [`BooleanOp`] (union/intersection/difference of two upstream
//! tessellations via the csgrs CSG library — first cad-core operator with
//! a Tier-3 dependency, see ADR-112).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tessellation::Tessellation;

pub mod boolean;
pub mod cuboid;
pub mod extrude;
pub mod revolve;
pub mod transform;

pub use boolean::{BooleanMode, BooleanOp};
pub use cuboid::CuboidOp;
pub use extrude::{ExtrudeOp, Polygon2D, Polygon2DError};
pub use revolve::RevolveOp;
pub use transform::TransformOp;

// ---------------------------------------------------------------------------
// OpError
// ---------------------------------------------------------------------------

/// Errors produced during operator evaluation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpError {
    /// The number of inputs supplied did not match the operator's declared
    /// arity.
    #[error("wrong arity: expected {expected}, got {got}")]
    WrongArity {
        /// Number of inputs the operator declares.
        expected: usize,
        /// Number of inputs actually supplied.
        got: usize,
    },
    /// Evaluation produced no geometry where some was expected.
    #[error("operator produced empty result")]
    EmptyResult,
    /// An operator parameter is out of its valid domain.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
}

// ---------------------------------------------------------------------------
// OpKind
// ---------------------------------------------------------------------------

/// Discriminant tag for operator kinds.
///
/// Wired alongside [`OperatorNode`] for cheap dispatch in inspectors without
/// matching on the full payload-bearing enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpKind {
    /// `BooleanOp` — union/intersection/difference of two upstream
    /// tessellations.
    Boolean,
    /// `CuboidOp` — origin-centered axis-aligned box primitive.
    Cuboid,
    /// `ExtrudeOp` — sweep a 2D convex polygon profile along `+Z`.
    Extrude,
    /// `RevolveOp` — rotate a 2D profile around the Y-axis through 2π.
    Revolve,
    /// `TransformOp` — affine TRS applied to one upstream tessellation.
    Transform,
}

// ---------------------------------------------------------------------------
// EdgeKind
// ---------------------------------------------------------------------------

/// Edge payload stored on every operator-graph edge.
///
/// `Input(port)` says: this edge feeds the downstream operator's `port`-th
/// declared input. Future operators with multiple ordered inputs (e.g. a
/// Boolean union with `lhs=0` and `rhs=1`) reuse the same variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// The edge feeds the destination operator's `port`-th input.
    Input(u8),
}

// ---------------------------------------------------------------------------
// Operator trait
// ---------------------------------------------------------------------------

/// Uniform contract every CAD operator implements.
///
/// `evaluate` produces an output [`Tessellation`] given the upstream inputs;
/// `structural_hash` is the local hash (NOT the recursive-into-inputs hash —
/// the graph evaluator combines these). `arity` declares how many inputs
/// `evaluate` expects.
pub trait Operator: std::fmt::Debug + Send + Sync {
    /// Discriminant tag — see [`OpKind`].
    fn op_kind(&self) -> OpKind;

    /// Number of upstream tessellations `evaluate` expects.
    fn arity(&self) -> usize;

    /// 32-byte BLAKE3 over `(op_kind discriminant, parameters)`.
    ///
    /// Must be deterministic across processes. Does NOT include input hashes
    /// — the [`crate::OperatorGraph::evaluate`] combines this with upstream
    /// hashes to produce the cache key.
    fn structural_hash(&self) -> [u8; 32];

    /// Run the operator. `inputs[i]` is the upstream tessellation feeding
    /// port `i`. The order matches the declared arity.
    ///
    /// # Errors
    ///
    /// * [`OpError::WrongArity`] if `inputs.len() != self.arity()`.
    /// * [`OpError::InvalidParameter`] for out-of-domain parameter values.
    /// * [`OpError::EmptyResult`] if evaluation succeeded but produced no
    ///   geometry (operator-specific — Cuboid/Transform never raise this).
    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError>;
}

// ---------------------------------------------------------------------------
// OperatorNode (tagged-union wrapper for graph storage)
// ---------------------------------------------------------------------------

/// Tagged-union enum the operator graph stores as its `N` payload.
///
/// `#[serde(tag = "kind")]` produces a stable wire representation
/// (`{ "kind": "Cuboid", "width": 1.0, ... }`) that is forward-compatible
/// when new variants are added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OperatorNode {
    /// Boolean combinator — see [`BooleanOp`].
    Boolean(BooleanOp),
    /// Cuboid primitive — see [`CuboidOp`].
    Cuboid(CuboidOp),
    /// Extrude — see [`ExtrudeOp`].
    Extrude(ExtrudeOp),
    /// Revolve — see [`RevolveOp`].
    Revolve(RevolveOp),
    /// Transform — see [`TransformOp`].
    Transform(TransformOp),
}

impl OperatorNode {
    /// Reborrow as a `&dyn Operator` for uniform dispatch.
    #[must_use]
    pub fn as_operator(&self) -> &dyn Operator {
        match self {
            OperatorNode::Boolean(op) => op,
            OperatorNode::Cuboid(op) => op,
            OperatorNode::Extrude(op) => op,
            OperatorNode::Revolve(op) => op,
            OperatorNode::Transform(op) => op,
        }
    }
}

impl Operator for OperatorNode {
    fn op_kind(&self) -> OpKind {
        self.as_operator().op_kind()
    }

    fn arity(&self) -> usize {
        self.as_operator().arity()
    }

    fn structural_hash(&self) -> [u8; 32] {
        self.as_operator().structural_hash()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        self.as_operator().evaluate(inputs)
    }
}

// ---------------------------------------------------------------------------
// Unit tests for the wrapper enum (operator-specific tests live in their
// own modules).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_node_dispatches_cuboid() {
        let node = OperatorNode::Cuboid(CuboidOp::default());
        assert_eq!(node.op_kind(), OpKind::Cuboid);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("eval");
        assert_eq!(mesh.vertex_count(), 8);
    }

    #[test]
    fn operator_node_dispatches_transform() {
        let node = OperatorNode::Transform(TransformOp::default());
        assert_eq!(node.op_kind(), OpKind::Transform);
        assert_eq!(node.arity(), 1);
    }

    #[test]
    fn operator_node_dispatches_extrude() {
        let profile = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("square profile");
        let node = OperatorNode::Extrude(ExtrudeOp::new(profile, 1.0).expect("extrude op"));
        assert_eq!(node.op_kind(), OpKind::Extrude);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4 ⇒ 8 vertices, 12 triangles, 36 indices.
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn operator_node_dispatches_revolve() {
        let profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
            .expect("revolve square profile");
        let node = OperatorNode::Revolve(RevolveOp::new(profile, 6).expect("revolve op"));
        assert_eq!(node.op_kind(), OpKind::Revolve);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4 × 6 segments ⇒ 24 vertices, 48 triangles, 144 indices.
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.triangle_count(), 48);
        assert_eq!(mesh.indices.len(), 144);
    }

    #[test]
    fn operator_node_dispatches_boolean() {
        let node = OperatorNode::Boolean(BooleanOp::union());
        assert_eq!(node.op_kind(), OpKind::Boolean);
        assert_eq!(node.arity(), 2);
        // Wrong arity (no inputs) yields WrongArity.
        let err = node.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 2,
                got: 0
            }
        ));
    }
}
