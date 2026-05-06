//! Boolean operator: union / intersection / difference of two upstream tessellations.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Per [ADR-112](../../../docs/adr/ADR-112-cad-boolean-csg-library.md). Backed
//! by `csgrs` (pure-Rust BSP-tree triangle-mesh CSG). The bridge converts
//! cad-core's triangle-soup [`Tessellation`] into csgrs's `Mesh`, runs the
//! boolean via [`CSG`], converts back, and preserves labels when present.
//!
//! # Unified labeled / unlabeled paths (2026-05-08 unified-mesh refactor)
//!
//! [`BooleanOp::evaluate`] handles both unlabeled and labeled inputs in a
//! single signature. Both unlabeled → unlabeled output (legacy bit-identical).
//! Both labeled → labeled output. Mixed → labeled output with the unlabeled
//! side synthesizing per-triangle [`TopologyFaceId::DEGENERATE`] (downstream
//! lineage classifies as Reinterpreted).
//!
//! # csgrs features / capability surface (per ADR-104 §"Initial field set")
//!
//! csgrs 0.20.1 `default-features = false` + `["f64", "earcut"]` (f64 avoids
//! rapier3d 0.24/0.32 conflict; bridge converts f32 ↔ f64). ADR-104's 6 canonical
//! fields: `boolean_robust_under_tolerance: false` (BSP, no exact arithmetic) /
//! `deterministic_triangulation: true` (200-iter soak gate) / `t_junction_handling:
//! false` (csgrs upstream TODO) / `concave_input_supported: true` / `arity: 2` /
//! `output_labeled_when_input_labeled: true` (matches default `any-labeled-input ⇒
//! labeled-output`). Supplemental: `healing_strategies: none` (csgrs runs no
//! mesh-healing passes; not in ADR-104 canonical set).
//!
//! # Failure handling
//!
//! csgrs's BSP can panic on degenerate input. Mitigation: pre-filter
//! degenerate triangles in [`csgrs_bridge::tessellation_to_csgrs`], wrap the op in
//! [`std::panic::catch_unwind`] surfacing as [`OpError::InvalidParameter`].
//! Snapshot-recoverable per PLAN §1.13.
//!
//! # Module layout
//!
//! * `csgrs_bridge` — f32 ↔ f64 conversion + tessellation/csgrs interconversion +
//!   degenerate-triangle pre-filter + outward-normal computation.
//! * `labeled_path` — labeled-Tessellation evaluation with per-triangle label
//!   propagation through csgrs's polygon metadata.
//! * `unlabeled_path` — unlabeled-Tessellation evaluation (simpler, bit-identical
//!   to the pre-refactor `evaluate`).

mod csgrs_bridge;
mod labeled_path;
#[cfg(test)]
mod tests;
mod unlabeled_path;

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;

// ---------------------------------------------------------------------------
// BooleanMode + BooleanOp
// ---------------------------------------------------------------------------

/// The boolean operation mode applied by [`BooleanOp`] to its two inputs.
///
/// Per ADR-112's API-shape recommendation. `Xor` is intentionally not exposed
/// at this milestone — csgrs supports it but ADR-112 did not include it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BooleanMode {
    /// `lhs ∪ rhs` — combined volume.
    Union,
    /// `lhs ∩ rhs` — overlapping volume.
    Intersection,
    /// `lhs − rhs` — `lhs` minus `rhs`. Non-commutative.
    Difference,
}

impl BooleanMode {
    /// Stable single-byte discriminant for use in [`BooleanOp::structural_hash`].
    #[must_use]
    pub(super) fn discriminant(self) -> u8 {
        match self {
            BooleanMode::Union => 0,
            BooleanMode::Intersection => 1,
            BooleanMode::Difference => 2,
        }
    }
}

/// Boolean combinator: union / intersection / difference of two upstream
/// tessellations.
///
/// Arity 2: `inputs[0]` is `lhs` (port 0), `inputs[1]` is `rhs` (port 1).
/// `Difference` is the only non-commutative mode (`lhs − rhs ≠ rhs − lhs`).
///
/// [`BooleanOp::structural_hash`] depends only on [`BooleanMode`]; upstream
/// hashes are folded in by [`crate::OperatorGraph::evaluate`]'s recursive
/// `effective_hash` per port index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BooleanOp {
    /// The boolean mode applied at this operator.
    pub mode: BooleanMode,
}

impl BooleanOp {
    /// Build a [`BooleanOp`] with the given [`BooleanMode`].
    #[must_use]
    pub const fn new(mode: BooleanMode) -> Self {
        Self { mode }
    }

    /// Convenience constructor for [`BooleanMode::Union`].
    #[must_use]
    pub const fn union() -> Self {
        Self::new(BooleanMode::Union)
    }

    /// Convenience constructor for [`BooleanMode::Intersection`].
    #[must_use]
    pub const fn intersection() -> Self {
        Self::new(BooleanMode::Intersection)
    }

    /// Convenience constructor for [`BooleanMode::Difference`].
    #[must_use]
    pub const fn difference() -> Self {
        Self::new(BooleanMode::Difference)
    }
}

impl Operator for BooleanOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Boolean
    }

    fn arity(&self) -> usize {
        2
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"boolean:");
        hasher.update(&[self.mode.discriminant()]);
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let lhs = inputs[0];
        let rhs = inputs[1];

        // Detect whether either side carries labels. If so, take the
        // labeled path (carry per-triangle TopologyFaceId metadata through
        // csgrs); otherwise the unlabeled path (no metadata, matches the
        // legacy `evaluate` behavior bit-identically).
        if lhs.is_labeled() || rhs.is_labeled() {
            labeled_path::evaluate_with_labels(self.mode, lhs, rhs)
        } else {
            unlabeled_path::evaluate_unlabeled(self.mode, lhs, rhs)
        }
    }
}
