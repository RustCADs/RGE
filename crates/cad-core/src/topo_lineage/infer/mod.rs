//! Plane-based labeling and lineage inference.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage`]; see that module's `//!` docs for
//! the design rationale + v0 simplifications vs PLAN §1.5.4.3.
//!
//! # Module layout
//!
//! * `label_by_plane` — group input triangles by plane equation, assign each
//!   distinct plane a sequential `face_id` starting at `base_id`.
//! * `infer_unlabeled` — plane-equation-matching heuristic for unlabeled
//!   output (the original `infer_lineage` path).
//! * `infer_labeled` — high-confidence label-tracking path for labeled output
//!   (csgrs metadata-passthrough fast path).

mod infer_labeled;
mod infer_unlabeled;
mod label_by_plane;
#[cfg(test)]
mod tests;

pub use label_by_plane::label_by_plane;

use crate::tessellation::Tessellation;
use crate::topo_lineage::types::{LineageError, LineageGraph};

// ---------------------------------------------------------------------------
// infer_lineage (unified)
// ---------------------------------------------------------------------------

/// Reconstruct lineage between a labeled input [`Tessellation`] and an
/// output [`Tessellation`] (which may or may not carry labels).
///
/// **Input must be labeled.** If `input.is_labeled() == false` this returns
/// [`LineageError::InvalidInput`]. Callers starting from primitive output
/// should derive labels via [`label_by_plane`] first.
///
/// # Two paths
///
/// The function dispatches on `output.is_labeled()`:
///
/// * **Labeled output** (high-confidence path, was `infer_lineage_labeled`)
///   — both sides carry per-triangle labels (typically because a Boolean op
///   propagated input labels through `csgrs`'s polygon metadata). Per-input-
///   label triangle-count comparison classifies each input face as
///   `Preserved` (in == out) or `Split` (in != out). Output labels not
///   present on the input become `Reinterpreted`. Confidence is 1.0
///   throughout (metadata directly tracked identity).
///
/// * **Unlabeled output** (plane-equation heuristic, was the original
///   `infer_lineage`) — labels the output via [`label_by_plane`] internally,
///   then matches input vs output planes. Same plane + same triangle count
///   → `Preserved` (1.0); same plane + fewer outputs → `Split` (1.0); same
///   plane + more outputs → `Merged` (0.5); no plane match → `Deleted`
///   (1.0); output planes with no input match → `Reinterpreted` (1.0).
///
/// Both paths return `(labeled_output, lineage_graph)` where the labeled
/// output is the input's `output` upgraded to labeled form (cloned through
/// when already labeled, or relabeled by plane when not).
///
/// # Errors
///
/// * [`LineageError::InvalidInput`] if the input is unlabeled.
/// * [`LineageError::InvalidInput`] if either mesh has malformed buffers.
///
/// # Panics
///
/// Panics if internal book-keeping diverges (every counted face id should
/// be present in its accompanying count map). Internal invariant — the
/// `expect`s document the guarantee.
pub fn infer_lineage(
    input: &Tessellation,
    output: &Tessellation,
    output_base_id: u64,
) -> Result<(Tessellation, LineageGraph), LineageError> {
    let input_labels = input.face_labels().ok_or_else(|| {
        LineageError::InvalidInput(
            "infer_lineage requires labeled input (call label_by_plane first)".to_string(),
        )
    })?;

    if output.is_labeled() {
        Ok(infer_labeled::infer_lineage_with_labeled_output(
            input,
            input_labels,
            output,
        ))
    } else {
        infer_unlabeled::infer_lineage_with_unlabeled_output(
            input,
            input_labels,
            output,
            output_base_id,
        )
    }
}
