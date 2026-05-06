//! High-confidence label-tracking lineage inference for labeled output.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage::infer`]; see the parent module's
//! `//!` docs for the design rationale.
//!
//! This is the fast path used when a Boolean op (or any operator) has
//! propagated input labels through `csgrs`'s polygon metadata. Per-input-
//! label triangle-count comparison classifies each input face deterministic-
//! ally; confidence is 1.0 throughout (metadata directly tracked identity).

use std::collections::{BTreeMap, BTreeSet};

use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topo_lineage::types::{LineageEdge, LineageGraph, TopologyEvolution};

/// High-confidence label-tracking path: output already carries labels (from
/// e.g. a Boolean op that propagated input labels through `csgrs`'s polygon
/// metadata).
///
/// For each input face id present in `input.face_labels`:
/// * If it appears in `output.face_labels`: classify by triangle count:
///   * `input_count == output_count` → `Preserved` (confidence 1.0).
///   * `input_count != output_count` → `Split` (confidence 1.0). Both
///     directions ("input partially consumed" — `input>output` — and "input
///     retriangulated by csgrs's BSP into more sub-triangles" —
///     `input<output`) classify uniformly as Split. **Merged** in the v0
///     lineage taxonomy means *multiple input faces collapse to one output
///     face* — that requires distinct input labels mapping to a single
///     output label, which the per-input-label scan cannot observe
///     directly. We therefore never emit Merged on the labeled path.
/// * Else (label not in output): `Deleted` (confidence 1.0).
///
/// For each output face id NOT in `input.face_labels`: `Reinterpreted`
/// (confidence 1.0). Triangles tagged with [`TopologyFaceId::DEGENERATE`]
/// are excluded from per-face counts; their presence on the **output** side
/// surfaces as a single `Reinterpreted` edge with `to =
/// Some(DEGENERATE)` (collectively — these are typically rhs-derived faces
/// from csgrs's `Difference` lhs-retag quirk, per ADR-112).
pub(super) fn infer_lineage_with_labeled_output(
    _input: &Tessellation,
    input_labels: &[TopologyFaceId],
    output: &Tessellation,
) -> (Tessellation, LineageGraph) {
    let output_labels = output
        .face_labels()
        .expect("infer_lineage_with_labeled_output called with unlabeled output");

    let mut input_count_per_face: BTreeMap<TopologyFaceId, usize> = BTreeMap::new();
    let mut input_face_set: BTreeSet<TopologyFaceId> = BTreeSet::new();
    for &face_id in input_labels {
        if face_id.is_degenerate() {
            continue;
        }
        *input_count_per_face.entry(face_id).or_insert(0) += 1;
        input_face_set.insert(face_id);
    }

    let mut output_count_per_face: BTreeMap<TopologyFaceId, usize> = BTreeMap::new();
    let mut output_face_set: BTreeSet<TopologyFaceId> = BTreeSet::new();
    let mut output_has_degenerate = false;
    for &face_id in output_labels {
        if face_id.is_degenerate() {
            output_has_degenerate = true;
            continue;
        }
        *output_count_per_face.entry(face_id).or_insert(0) += 1;
        output_face_set.insert(face_id);
    }

    let mut graph = LineageGraph::new();

    // Walk inputs in deterministic (BTreeSet) order.
    for &input_face_id in &input_face_set {
        let in_count = *input_count_per_face
            .get(&input_face_id)
            .expect("every counted input face is in input_count_per_face");
        if let Some(&out_count) = output_count_per_face.get(&input_face_id) {
            // Labeled path: same-label-different-count is always Split,
            // never Merged (Merged requires multiple distinct input labels
            // collapsing to one output label, which this scan cannot see).
            let evolution = if in_count == out_count {
                TopologyEvolution::Preserved
            } else {
                TopologyEvolution::Split
            };
            graph.push(LineageEdge {
                from: Some(input_face_id),
                to: Some(input_face_id),
                evolution,
                // Confidence 1.0 across the board for the labeled path —
                // metadata directly tracked identity, no plane-equation
                // fuzziness.
                confidence: 1.0,
            });
        } else {
            // Input face label not in output → that face was wholly removed.
            graph.push(LineageEdge {
                from: Some(input_face_id),
                to: None,
                evolution: TopologyEvolution::Deleted,
                confidence: 1.0,
            });
        }
    }

    // Output faces with no matching input label → Reinterpreted.
    for &output_face_id in &output_face_set {
        if !input_face_set.contains(&output_face_id) {
            graph.push(LineageEdge {
                from: None,
                to: Some(output_face_id),
                evolution: TopologyEvolution::Reinterpreted,
                confidence: 1.0,
            });
        }
    }

    // Difference's lhs-retag artifacts arrive labeled DEGENERATE (the
    // unmetadata sentinel from the boolean bridge). Surface them as a
    // single Reinterpreted edge (collectively), with `to =
    // Some(DEGENERATE)`.
    if output_has_degenerate {
        graph.push(LineageEdge {
            from: None,
            to: Some(TopologyFaceId::DEGENERATE),
            evolution: TopologyEvolution::Reinterpreted,
            confidence: 1.0,
        });
    }

    (output.clone(), graph)
}
