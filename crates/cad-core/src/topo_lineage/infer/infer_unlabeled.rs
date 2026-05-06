//! Plane-equation-matching heuristic for unlabeled-output lineage inference.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage::infer`]; see the parent module's
//! `//!` docs for the design rationale.

use std::collections::{BTreeMap, BTreeSet};

use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topo_lineage::infer::label_by_plane::label_by_plane;
use crate::topo_lineage::plane::QuantizedPlane;
use crate::topo_lineage::types::{LineageEdge, LineageError, LineageGraph, TopologyEvolution};

/// Plane-equation fallback path: output has no labels yet. Build them via
/// [`label_by_plane`] then run the original triangle-count-vs-plane
/// heuristic. Returns the freshly-labeled output.
pub(super) fn infer_lineage_with_unlabeled_output(
    input: &Tessellation,
    input_labels: &[TopologyFaceId],
    output: &Tessellation,
    output_base_id: u64,
) -> Result<(Tessellation, LineageGraph), LineageError> {
    // Group output triangles by plane via label_by_plane. The returned
    // labeled_output's face_labels[i] is the face id of triangle i in
    // output; the iteration order assigns ids in input-traversal order.
    let labeled_output = label_by_plane(output, output_base_id)?;
    let labeled_output_labels = labeled_output
        .face_labels()
        .expect("label_by_plane always returns labeled output");

    // For each output face id, recover its plane and triangle count.
    // Skip triangles tagged DEGENERATE — they're not assigned a real face.
    let mut output_plane_for_face: BTreeMap<TopologyFaceId, QuantizedPlane> = BTreeMap::new();
    let mut output_tri_count_per_face: BTreeMap<TopologyFaceId, usize> = BTreeMap::new();
    for (tri_idx, &face_id) in labeled_output_labels.iter().enumerate() {
        if face_id.is_degenerate() {
            continue;
        }
        *output_tri_count_per_face.entry(face_id).or_insert(0) += 1;
        if let std::collections::btree_map::Entry::Vacant(slot) =
            output_plane_for_face.entry(face_id)
        {
            let i0 = labeled_output.indices[tri_idx * 3] as usize;
            let i1 = labeled_output.indices[tri_idx * 3 + 1] as usize;
            let i2 = labeled_output.indices[tri_idx * 3 + 2] as usize;
            // We just labeled this triangle by plane (label_by_plane only
            // assigns a non-DEGENERATE id when from_triangle succeeded),
            // so re-deriving the plane must succeed. If it doesn't,
            // surface the error instead of silently falling back.
            let plane = QuantizedPlane::from_triangle(
                labeled_output.positions[i0],
                labeled_output.positions[i1],
                labeled_output.positions[i2],
                tri_idx,
            )?;
            slot.insert(plane);
        }
    }

    // Build a plane → output_face_id index for fast input → output lookup.
    let mut output_face_for_plane: BTreeMap<QuantizedPlane, TopologyFaceId> = BTreeMap::new();
    for (&face_id, &plane) in &output_plane_for_face {
        // First-id-wins under the BTreeMap ordering — but each plane has
        // exactly one face_id from label_by_plane, so the entry should be
        // fresh on every insert. Use Entry::or_insert to be robust to a
        // future label_by_plane that allowed two ids per plane.
        output_face_for_plane.entry(plane).or_insert(face_id);
    }

    // For each input face, derive its plane (one triangle is enough: all
    // input triangles with the same face_id are on the same plane by
    // assumption — we walk input.indices once to find the first triangle
    // per face and count triangles per face). Skip DEGENERATE labels.
    //
    // We only count a triangle toward `input_tri_count_per_face` once we've
    // confirmed the triangle is non-degenerate; otherwise the count would
    // be inflated by sliver triangles that happen to share a label with a
    // non-degenerate face.
    let mut input_plane_for_face: BTreeMap<TopologyFaceId, QuantizedPlane> = BTreeMap::new();
    let mut input_tri_count_per_face: BTreeMap<TopologyFaceId, usize> = BTreeMap::new();
    for (tri_idx, &face_id) in input_labels.iter().enumerate() {
        if face_id.is_degenerate() {
            continue;
        }
        let i0 = input.indices[tri_idx * 3] as usize;
        let i1 = input.indices[tri_idx * 3 + 1] as usize;
        let i2 = input.indices[tri_idx * 3 + 2] as usize;
        match QuantizedPlane::from_triangle(
            input.positions[i0],
            input.positions[i1],
            input.positions[i2],
            tri_idx,
        ) {
            Ok(plane) => {
                *input_tri_count_per_face.entry(face_id).or_insert(0) += 1;
                input_plane_for_face.entry(face_id).or_insert(plane);
            }
            Err(LineageError::DegenerateTriangle { .. } | LineageError::NonFiniteNormal { .. }) => {
                // Caller hand-built a labeled tessellation with a
                // non-degenerate label on a degenerate triangle —
                // ambiguous. Skip (the outer `for` loop already advances
                // to the next triangle).
            }
            Err(other) => return Err(other),
        }
    }

    let mut graph = LineageGraph::new();
    let mut output_faces_matched: BTreeSet<TopologyFaceId> = BTreeSet::new();

    // Walk inputs in deterministic (BTreeMap) order.
    for (&input_face_id, &input_plane) in &input_plane_for_face {
        let in_count = *input_tri_count_per_face
            .get(&input_face_id)
            .expect("every input face was counted");
        if let Some(&output_face_id) = output_face_for_plane.get(&input_plane) {
            output_faces_matched.insert(output_face_id);
            let out_count = *output_tri_count_per_face
                .get(&output_face_id)
                .expect("every output face was counted");
            let (evolution, confidence) = match in_count.cmp(&out_count) {
                std::cmp::Ordering::Equal => (TopologyEvolution::Preserved, 1.0_f32),
                std::cmp::Ordering::Greater => (TopologyEvolution::Split, 1.0_f32),
                std::cmp::Ordering::Less => (TopologyEvolution::Merged, 0.5_f32),
            };
            graph.push(LineageEdge {
                from: Some(input_face_id),
                to: Some(output_face_id),
                evolution,
                confidence,
            });
        } else {
            graph.push(LineageEdge {
                from: Some(input_face_id),
                to: None,
                evolution: TopologyEvolution::Deleted,
                confidence: 1.0,
            });
        }
    }

    // Output faces with no input match → Reinterpreted.
    for &output_face_id in output_plane_for_face.keys() {
        if !output_faces_matched.contains(&output_face_id) {
            graph.push(LineageEdge {
                from: None,
                to: Some(output_face_id),
                evolution: TopologyEvolution::Reinterpreted,
                confidence: 1.0,
            });
        }
    }

    Ok((labeled_output, graph))
}
