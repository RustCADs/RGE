//! Unlabeled-tessellation evaluation path for [`crate::operators::BooleanOp`].
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Sub-module of [`crate::operators::boolean`]; see that module's `//!` docs
//! for the design rationale (ADR-112) + the unified labeled / unlabeled paths
//! overview.
//!
//! This file owns the unlabeled fast path: both inputs lack labels, output is
//! unlabeled, csgrs operates on `Mesh<()>` (the no-payload metadata type).
//! Bit-identical to the pre-refactor `evaluate`.

use csgrs::mesh::Mesh as CsgrsMesh;

use crate::operators::boolean::csgrs_bridge::{
    csgrs_to_tessellation, run_boolean, tessellation_to_csgrs,
};
use crate::operators::boolean::BooleanMode;
use crate::operators::OpError;
use crate::tessellation::Tessellation;

/// Unlabeled fast path — both inputs lack labels, output is unlabeled.
/// Bit-identical to the pre-refactor `evaluate`.
pub(super) fn evaluate_unlabeled(
    mode: BooleanMode,
    lhs: &Tessellation,
    rhs: &Tessellation,
) -> Result<Tessellation, OpError> {
    // Convert both inputs to csgrs Mesh<()>. () is the no-payload metadata
    // type; this is the unlabeled path that drops any lineage info.
    let lhs_mesh: CsgrsMesh<()> = tessellation_to_csgrs(&lhs.positions, &lhs.indices, |_| ());
    let rhs_mesh: CsgrsMesh<()> = tessellation_to_csgrs(&rhs.positions, &rhs.indices, |_| ());

    let result = run_boolean(mode, &lhs_mesh, &rhs_mesh)?;

    // Convert the csgrs result back to triangle-soup Tessellation.
    let (positions, indices, _labels) = csgrs_to_tessellation::<()>(&result, || ())?;
    Tessellation::new(positions, indices).map_err(|e| {
        OpError::InvalidParameter(format!("boolean failed: invalid output tessellation: {e}"))
    })
}
