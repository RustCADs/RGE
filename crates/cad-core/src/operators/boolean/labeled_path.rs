//! Labeled-tessellation evaluation path for [`crate::operators::BooleanOp`].
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Sub-module of [`crate::operators::boolean`]; see that module's `//!` docs
//! for the design rationale (ADR-112) + the unified labeled / unlabeled paths
//! overview.
//!
//! This file owns the labeled path: at least one input carries labels; csgrs
//! polygon metadata threads per-triangle [`TopologyFaceId`] through the BSP
//! tree; mixed inputs synthesize [`TopologyFaceId::DEGENERATE`] on the
//! unlabeled side (lineage classifies as Reinterpreted).

use csgrs::mesh::Mesh as CsgrsMesh;

use crate::operators::boolean::csgrs_bridge::{
    csgrs_to_tessellation, run_boolean, tessellation_to_csgrs,
};
use crate::operators::boolean::BooleanMode;
use crate::operators::OpError;
use crate::tessellation::{Tessellation, TopologyFaceId};

/// Labeled path — at least one input carries labels. Per-triangle
/// [`TopologyFaceId`] labels thread through csgrs's polygon metadata.
/// Mixed inputs synthesize [`TopologyFaceId::DEGENERATE`] on the
/// unlabeled side (lineage classifies as Reinterpreted).
///
/// csgrs metadata semantics (per ADR-112 §"Followups"): Union and
/// Intersection preserve polygon metadata through plane splits /
/// `clip_polygons`. **Difference** retags rhs's clipped polygons with
/// `self.metadata` (lhs's `Mesh::metadata` = None) — a known csgrs
/// quirk; those rhs-derived faces fall back to
/// [`TopologyFaceId::DEGENERATE`] via `csgrs_to_tessellation`'s
/// unmetadata sentinel.
pub(super) fn evaluate_with_labels(
    mode: BooleanMode,
    lhs: &Tessellation,
    rhs: &Tessellation,
) -> Result<Tessellation, OpError> {
    let lhs_labels = derive_per_triangle_labels(lhs);
    let rhs_labels = derive_per_triangle_labels(rhs);

    let lhs_mesh: CsgrsMesh<TopologyFaceId> =
        tessellation_to_csgrs(&lhs.positions, &lhs.indices, |tri_idx| lhs_labels[tri_idx]);
    let rhs_mesh: CsgrsMesh<TopologyFaceId> =
        tessellation_to_csgrs(&rhs.positions, &rhs.indices, |tri_idx| rhs_labels[tri_idx]);

    let result = run_boolean(mode, &lhs_mesh, &rhs_mesh)?;

    // Convert back, propagating the per-polygon metadata into per-output-
    // triangle labels. Polygons whose csgrs metadata field is None
    // (rhs-derived under Difference's lhs-retag quirk where
    // `Mesh::metadata` is None) get tagged with TopologyFaceId::DEGENERATE.
    let (positions, indices, labels) =
        csgrs_to_tessellation::<TopologyFaceId>(&result, || TopologyFaceId::DEGENERATE)?;

    Tessellation::with_labels(positions, indices, labels).map_err(|e| {
        OpError::InvalidParameter(format!(
            "boolean failed to build labeled output tessellation: {e}"
        ))
    })
}

/// Pull (or synthesize) per-triangle labels for a [`Tessellation`].
///
/// * Labeled input → clone the existing labels.
/// * Unlabeled input → synthesize a `Vec` of [`TopologyFaceId::DEGENERATE`]
///   one per triangle. Downstream lineage classifies those as Reinterpreted,
///   which is the desired semantics for "this side had no input-face
///   identity to track".
fn derive_per_triangle_labels(tess: &Tessellation) -> Vec<TopologyFaceId> {
    if let Some(labels) = tess.face_labels() {
        labels.to_vec()
    } else {
        vec![TopologyFaceId::DEGENERATE; tess.triangle_count()]
    }
}
