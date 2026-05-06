//! Plane-based labeling and lineage inference.
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage`]; see that module's `//!` docs for
//! the design rationale + v0 simplifications vs PLAN §1.5.4.3.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topo_lineage::plane::QuantizedPlane;
use crate::topo_lineage::types::{LineageEdge, LineageError, LineageGraph, TopologyEvolution};

// ---------------------------------------------------------------------------
// label_by_plane
// ---------------------------------------------------------------------------

/// Label a [`Tessellation`] by grouping its triangles by plane equation.
/// Returns a new [`Tessellation`] with `face_labels = Some(...)`.
///
/// Each distinct [`QuantizedPlane`] gets a sequential `face_id` starting at
/// `base_id`. Triangles whose planes hash equally (sign-canonicalized at
/// ~1e-4 quantization) share the same `face_id`. Degenerate / non-finite
/// triangles are tagged [`TopologyFaceId::DEGENERATE`] and excluded from
/// face counts; they are not an error condition.
///
/// The `face_id` assignment order is **input-traversal order** (first
/// triangle to touch a given plane gets the lowest id) — deterministic
/// because the outer loop walks `tess.indices` in order.
///
/// `base_id` MUST satisfy `base_id + triangle_count < u64::MAX` so plane
/// face ids cannot collide with [`TopologyFaceId::DEGENERATE`]. In practice
/// a `base_id` of `0`, `100`, or `1_000_000` etc. is safely below the
/// sentinel.
///
/// If the input tessellation already carries labels (`tess.is_labeled() ==
/// true`), they are **discarded and replaced** by plane-derived labels —
/// the caller asked to relabel by plane, so we relabel by plane.
///
/// # Errors
///
/// * [`LineageError::InvalidInput`] if the input tessellation has malformed
///   index buffers (out of bounds; non-multiple-of-3 length). The
///   `Tessellation` constructor already enforces this; the check here is
///   defensive and would only trip if a future API mutated the public
///   fields after construction.
///
/// # Panics
///
/// Panics if the next `face_id` would equal [`TopologyFaceId::DEGENERATE`]
/// (`u64::MAX`). In practice `base_id` is always small (`0`, `100`,
/// `1_000_000`), so the next id stays well below the sentinel; the
/// assertion documents the invariant for future callers that pass huge
/// `base_id` values.
pub fn label_by_plane(tess: &Tessellation, base_id: u64) -> Result<Tessellation, LineageError> {
    if tess.indices.len() % 3 != 0 {
        return Err(LineageError::InvalidInput(format!(
            "indices.len() ({}) must be a multiple of 3",
            tess.indices.len()
        )));
    }
    let positions_len = tess.positions.len();
    for (i, &idx) in tess.indices.iter().enumerate() {
        if (idx as usize) >= positions_len {
            return Err(LineageError::InvalidInput(format!(
                "index {idx} at indices[{i}] out of bounds (positions.len() = {positions_len})"
            )));
        }
    }

    let triangle_count = tess.indices.len() / 3;
    let mut face_labels = Vec::with_capacity(triangle_count);
    // HashMap is fine here: determinism comes from the input traversal order
    // populating face_labels in lock-step with the loop, not from the map's
    // iteration order. (We never iterate the map.)
    let mut plane_to_face: HashMap<QuantizedPlane, TopologyFaceId> =
        HashMap::with_capacity(triangle_count.min(64));
    let mut next_id = base_id;

    for tri_idx in 0..triangle_count {
        let i0 = tess.indices[tri_idx * 3] as usize;
        let i1 = tess.indices[tri_idx * 3 + 1] as usize;
        let i2 = tess.indices[tri_idx * 3 + 2] as usize;
        let plane = match QuantizedPlane::from_triangle(
            tess.positions[i0],
            tess.positions[i1],
            tess.positions[i2],
            tri_idx,
        ) {
            Ok(p) => p,
            Err(LineageError::DegenerateTriangle { .. } | LineageError::NonFiniteNormal { .. }) => {
                // Real-world CSG outputs contain slivers / zero-area
                // artifacts; tag them with the sentinel and continue.
                face_labels.push(TopologyFaceId::DEGENERATE);
                continue;
            }
            Err(other) => return Err(other),
        };
        let face_id = if let Some(existing) = plane_to_face.get(&plane) {
            *existing
        } else {
            // Defensive: if next_id ever reaches u64::MAX (the sentinel) we
            // refuse to issue it as a real face id. In practice base_id is
            // always small (0 / 100 / 1_000_000) so this is unreachable —
            // but the assertion documents the invariant.
            assert!(
                next_id != u64::MAX,
                "label_by_plane exhausted face id space (would collide with DEGENERATE sentinel)"
            );
            let id = TopologyFaceId(next_id);
            next_id = next_id.saturating_add(1);
            plane_to_face.insert(plane, id);
            id
        };
        face_labels.push(face_id);
    }

    Tessellation::with_labels(tess.positions.clone(), tess.indices.clone(), face_labels).map_err(
        |e| {
            LineageError::InvalidInput(format!("label_by_plane produced invalid tessellation: {e}"))
        },
    )
}

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
        Ok(infer_lineage_with_labeled_output(
            input,
            input_labels,
            output,
        ))
    } else {
        infer_lineage_with_unlabeled_output(input, input_labels, output, output_base_id)
    }
}

/// Plane-equation fallback path: output has no labels yet. Build them via
/// [`label_by_plane`] then run the original triangle-count-vs-plane
/// heuristic. Returns the freshly-labeled output.
fn infer_lineage_with_unlabeled_output(
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
fn infer_lineage_with_labeled_output(
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{CuboidOp, ExtrudeOp, Operator, Polygon2D};

    /// Helper for the labeled-path tests: build a hand-rolled labeled
    /// [`Tessellation`] from positions / indices / labels.
    fn labeled_mesh(
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        labels: Vec<TopologyFaceId>,
    ) -> Tessellation {
        Tessellation::with_labels(positions, indices, labels).expect("test labeled mesh ctor")
    }

    // --- label_by_plane ---------------------------------------------------

    #[test]
    fn label_by_plane_unit_cube_yields_6_face_groups() {
        // CuboidOp::default() is 1×1×1 origin-centered → 12 triangles in 6
        // plane groups (the cube's 6 axis-aligned faces).
        let cube = CuboidOp::default();
        let tess = cube.evaluate(&[]).expect("cube tess");
        assert_eq!(tess.triangle_count(), 12);
        let labeled = label_by_plane(&tess, 0).expect("label cube");
        assert!(labeled.is_labeled(), "label_by_plane returns labeled tess");
        assert_eq!(
            labeled.face_count(),
            Some(6),
            "cube should have 6 plane groups"
        );
        assert_eq!(labeled.triangle_count(), 12);
    }

    #[test]
    fn label_by_plane_extrude_triangle_yields_5_face_groups() {
        // Triangle profile extruded by 1.0 → 1 bottom cap + 1 top cap + 3
        // side walls = 5 plane groups. Triangle profile produces 1
        // triangle/cap + 2 triangles/side wall (2 per quad) = 1 + 1 + 2*3 =
        // 8 triangles total.
        let triangle =
            Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle profile");
        let extrude = ExtrudeOp::new(triangle, 1.0).expect("extrude op");
        let tess = extrude.evaluate(&[]).expect("extrude tess");
        assert_eq!(tess.triangle_count(), 8, "triangle prism = 8 triangles");
        let labeled = label_by_plane(&tess, 0).expect("label extrude");
        assert_eq!(
            labeled.face_count(),
            Some(5),
            "triangle prism should have 5 plane groups (2 caps + 3 walls)"
        );
    }

    // --- infer_lineage error path -----------------------------------------

    #[test]
    fn infer_lineage_with_unlabeled_input_returns_invalid_input_error() {
        // Caller passes unlabeled input → must return InvalidInput.
        let cube = CuboidOp::default();
        let tess = cube.evaluate(&[]).expect("cube tess");
        // tess is unlabeled (default).
        assert!(!tess.is_labeled());
        let err = infer_lineage(&tess, &tess, 100).unwrap_err();
        match err {
            LineageError::InvalidInput(msg) => {
                assert!(
                    msg.contains("requires labeled input"),
                    "expected 'requires labeled input' message; got: {msg}"
                );
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    // --- infer_lineage with unlabeled output (plane-heuristic path) -------

    #[test]
    fn infer_lineage_with_labeled_input_unlabeled_output_uses_plane_heuristic() {
        // input == output (same cube) → identity preserves all 6 plane
        // groups. Output is unlabeled, so the plane heuristic kicks in.
        let cube = CuboidOp::default();
        let tess = cube.evaluate(&[]).expect("cube tess");
        let labeled_input = label_by_plane(&tess, 0).expect("label cube");
        // tess is unlabeled.
        assert!(!tess.is_labeled());

        let (labeled_output, lineage) =
            infer_lineage(&labeled_input, &tess, 100).expect("identity lineage");
        assert!(
            labeled_output.is_labeled(),
            "infer_lineage returns labeled output"
        );
        assert_eq!(
            labeled_output.face_count(),
            Some(6),
            "output has same 6 face groups"
        );
        // 6 input faces ⇒ 6 Preserved edges; 0 Reinterpreted (no new
        // planes).
        let preserved_count = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(
            preserved_count, 6,
            "expected 6 Preserved edges, got {preserved_count}"
        );
        let reint_count = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .count();
        assert_eq!(
            reint_count, 0,
            "expected 0 Reinterpreted edges for identity, got {reint_count}"
        );
        // All Preserved edges should have confidence 1.0.
        for edge in lineage.edges_by_evolution(TopologyEvolution::Preserved) {
            assert!(
                (edge.confidence - 1.0).abs() < 1e-6,
                "Preserved confidence should be 1.0, got {}",
                edge.confidence
            );
        }
    }

    #[test]
    fn infer_lineage_deletion_records_deleted_edge() {
        // Input: cube → 6 plane groups.
        // Output: synthesize a "smaller" mesh with one input plane removed
        // (drop the +Z face's two triangles).
        let cube = CuboidOp::default();
        let tess = cube.evaluate(&[]).expect("cube tess");
        let labeled_input = label_by_plane(&tess, 0).expect("label cube");

        // Find the +Z plane's face_id by inspecting input.face_labels.
        // We'll drop all triangles whose plane has +Z normal (z == +0.5
        // offset) — the easiest way is to just check vertex z coords on
        // each triangle: if all three z-coords are +0.5 we're on the +Z
        // face. Then build a mesh from the remaining triangles only.
        let mut shrunk_indices = Vec::new();
        for tri_idx in 0..tess.triangle_count() {
            let i0 = tess.indices[tri_idx * 3] as usize;
            let i1 = tess.indices[tri_idx * 3 + 1] as usize;
            let i2 = tess.indices[tri_idx * 3 + 2] as usize;
            let z0 = tess.positions[i0][2];
            let z1 = tess.positions[i1][2];
            let z2 = tess.positions[i2][2];
            let on_plus_z =
                (z0 - 0.5).abs() < 1e-5 && (z1 - 0.5).abs() < 1e-5 && (z2 - 0.5).abs() < 1e-5;
            if !on_plus_z {
                shrunk_indices.push(tess.indices[tri_idx * 3]);
                shrunk_indices.push(tess.indices[tri_idx * 3 + 1]);
                shrunk_indices.push(tess.indices[tri_idx * 3 + 2]);
            }
        }
        let shrunk =
            Tessellation::new(tess.positions.clone(), shrunk_indices).expect("shrunk tess");

        let (_labeled_output, lineage) =
            infer_lineage(&labeled_input, &shrunk, 100).expect("lineage");
        let deleted_count = lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .count();
        assert_eq!(
            deleted_count, 1,
            "expected exactly 1 Deleted edge (the +Z face), got {deleted_count}"
        );
        // The Deleted edge should have to=None and confidence=1.0.
        let deleted_edge = lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .next()
            .unwrap();
        assert!(deleted_edge.from.is_some());
        assert!(deleted_edge.to.is_none());
        assert!((deleted_edge.confidence - 1.0).abs() < 1e-6);
    }

    #[test]
    fn infer_lineage_reinterpretation_records_new_face() {
        // Input: cube → 6 plane groups.
        // Output: cube + an extra triangle on a NEW plane (e.g. the y=0
        // plane diagonal — a plane that does not match any cube face).
        let cube = CuboidOp::default();
        let tess = cube.evaluate(&[]).expect("cube tess");
        let labeled_input = label_by_plane(&tess, 0).expect("label cube");

        // Build an output that's the cube + a single extra triangle on a
        // tilted plane (no axis-aligned, so it cannot match any of the
        // cube's 6 faces).
        let mut positions = tess.positions.clone();
        let v_a = u32::try_from(positions.len()).expect("position count fits u32");
        positions.push([0.0, 0.0, 1.0]);
        positions.push([1.0, 0.0, 1.5]);
        positions.push([0.0, 1.0, 1.7]);
        let mut indices = tess.indices.clone();
        indices.push(v_a);
        indices.push(v_a + 1);
        indices.push(v_a + 2);
        let augmented = Tessellation::new(positions, indices).expect("augmented");

        let (_labeled_output, lineage) =
            infer_lineage(&labeled_input, &augmented, 100).expect("lineage");
        let reint_count = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .count();
        assert_eq!(
            reint_count, 1,
            "expected exactly 1 Reinterpreted edge (the new tilted plane), got {reint_count}"
        );
        let reint_edge = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .next()
            .unwrap();
        assert!(reint_edge.from.is_none());
        assert!(reint_edge.to.is_some());
        assert!((reint_edge.confidence - 1.0).abs() < 1e-6);
        // All 6 cube faces should still be Preserved.
        let preserved_count = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(
            preserved_count, 6,
            "expected 6 Preserved edges (cube unchanged), got {preserved_count}"
        );
    }

    #[test]
    fn infer_lineage_split_edge_when_input_has_more_triangles_on_plane() {
        // Build a minimal scenario where the input has 2 triangles on a
        // plane and the output has 1 triangle on the same plane — the
        // detector should fire `Split` (input tri count > output tri
        // count).
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        // Input: 2 triangles forming a quad on z=0 plane.
        let in_tess =
            Tessellation::new(positions.clone(), vec![0_u32, 1, 2, 0, 2, 3]).expect("input");
        let labeled_in = label_by_plane(&in_tess, 0).expect("label input");
        // Output: 1 triangle on the same z=0 plane.
        let out_tess = Tessellation::new(positions, vec![0_u32, 1, 2]).expect("output");
        let (_labeled_out, lineage) = infer_lineage(&labeled_in, &out_tess, 100).expect("lineage");
        let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
        assert_eq!(
            split_count, 1,
            "expected 1 Split edge (input had more triangles), got {split_count}"
        );
    }

    // --- infer_lineage with labeled output (high-confidence path) ---------

    #[test]
    fn infer_lineage_with_labeled_input_labeled_output_uses_label_tracking() {
        // input == output (same labels everywhere) → all Preserved.
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let indices = vec![0_u32, 1, 2, 0, 2, 3];
        let labels = vec![TopologyFaceId(7), TopologyFaceId(7)];
        let mesh = labeled_mesh(positions, indices, labels);
        assert!(mesh.is_labeled());

        let (_out, lineage) = infer_lineage(&mesh, &mesh, 100).expect("identity labeled");
        let preserved = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(preserved, 1, "expected 1 Preserved edge for face 7");
        // No other classifications.
        for ev in [
            TopologyEvolution::Split,
            TopologyEvolution::Merged,
            TopologyEvolution::Deleted,
            TopologyEvolution::Reinterpreted,
        ] {
            assert_eq!(
                lineage.edges_by_evolution(ev).count(),
                0,
                "expected 0 {ev:?} edges for identity labeled mesh"
            );
        }
        // Confidence on the labeled path is 1.0 across the board.
        for edge in &lineage.edges {
            assert!((edge.confidence - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn infer_lineage_labeled_difference_classifies_as_split_not_merged() {
        // Hand-construct an input + output where one input face has FEWER
        // output triangles than input — the labeled path must classify
        // this as Split (not Merged, which is the v0 plane-only false-
        // positive class). This is the **central correctness validation**
        // of the metadata-passthrough integration.
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        // Input: 4 triangles on the same plane, all sharing label 0.
        let input = labeled_mesh(
            positions.clone(),
            vec![0, 1, 2, 0, 2, 3, 0, 1, 3, 1, 2, 3],
            vec![
                TopologyFaceId(0),
                TopologyFaceId(0),
                TopologyFaceId(0),
                TopologyFaceId(0),
            ],
        );
        // Output: only 2 triangles survived with label 0 (others consumed
        // by Difference).
        let output = labeled_mesh(
            positions,
            vec![0, 1, 2, 0, 2, 3],
            vec![TopologyFaceId(0), TopologyFaceId(0)],
        );

        let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled diff lineage");
        let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
        let merged_count = lineage
            .edges_by_evolution(TopologyEvolution::Merged)
            .count();
        assert_eq!(
            split_count, 1,
            "labeled path must classify input>output triangle count as Split, got {split_count}"
        );
        assert_eq!(
            merged_count, 0,
            "labeled path must NOT classify input>output as Merged (v0 plane-only false positive); got {merged_count}"
        );
    }

    #[test]
    fn infer_lineage_labeled_deletion_records_deleted() {
        // Input has labels {0, 1}; output has only {0}. Label 1 should
        // surface as a single Deleted edge.
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let input = labeled_mesh(
            positions.clone(),
            vec![0, 1, 2, 0, 2, 3],
            vec![TopologyFaceId(0), TopologyFaceId(1)],
        );
        let output = labeled_mesh(positions, vec![0, 1, 2], vec![TopologyFaceId(0)]);

        let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled del lineage");
        let deleted = lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .count();
        assert_eq!(
            deleted, 1,
            "expected exactly 1 Deleted edge for missing label 1; got {deleted}"
        );
        let deleted_edge = lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .next()
            .unwrap();
        assert_eq!(deleted_edge.from, Some(TopologyFaceId(1)));
        assert!(deleted_edge.to.is_none());
        assert!((deleted_edge.confidence - 1.0).abs() < 1e-6);
        // Label 0 is Preserved (1 input tri, 1 output tri).
        let preserved = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(preserved, 1);
    }

    #[test]
    fn infer_lineage_labeled_reinterpretation_records_new_face() {
        // Output has a face label not in input → Reinterpreted edge.
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let input = labeled_mesh(positions.clone(), vec![0, 1, 2], vec![TopologyFaceId(0)]);
        let output = labeled_mesh(
            positions,
            vec![0, 1, 2, 0, 1, 2],
            vec![TopologyFaceId(0), TopologyFaceId(99)],
        );

        let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled reint lineage");
        let reint = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .count();
        assert_eq!(
            reint, 1,
            "expected exactly 1 Reinterpreted edge for new label 99; got {reint}"
        );
        let edge = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .next()
            .unwrap();
        assert!(edge.from.is_none());
        assert_eq!(edge.to, Some(TopologyFaceId(99)));
        // Label 0 is Preserved (1 input tri, 1 output tri matching label 0).
        let preserved = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(preserved, 1);
    }

    #[test]
    fn infer_lineage_labeled_distinguishes_lhs_rhs_labels() {
        // Input has labels {0,1,2} (lhs face range) ∪ {10,11,12} (rhs
        // face range), simulating a Boolean op where lhs labels were 0..3
        // and rhs labels were 10..13. The output drops face 1 and face 11
        // but keeps the rest. Verify both sides surface independently in
        // the lineage:
        //  * lhs: 0, 2 → Preserved; 1 → Deleted
        //  * rhs: 10, 12 → Preserved; 11 → Deleted
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let lhs_indices = vec![
            0, 1, 2, // tri 0 → label 0
            0, 1, 2, // tri 1 → label 1
            0, 1, 2, // tri 2 → label 2
            0, 1, 2, // tri 3 → label 10
            0, 1, 2, // tri 4 → label 11
            0, 1, 2, // tri 5 → label 12
        ];
        let lhs_labels = vec![
            TopologyFaceId(0),
            TopologyFaceId(1),
            TopologyFaceId(2),
            TopologyFaceId(10),
            TopologyFaceId(11),
            TopologyFaceId(12),
        ];
        let input = labeled_mesh(positions.clone(), lhs_indices, lhs_labels);

        // Output: keep 0, 2, 10, 12 (drop 1 and 11).
        let out_indices = vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
        let out_labels = vec![
            TopologyFaceId(0),
            TopologyFaceId(2),
            TopologyFaceId(10),
            TopologyFaceId(12),
        ];
        let output = labeled_mesh(positions, out_indices, out_labels);

        let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled lhs/rhs lineage");
        // 4 Preserved (0, 2, 10, 12); 2 Deleted (1, 11); 0 Reinterpreted.
        assert_eq!(
            lineage
                .edges_by_evolution(TopologyEvolution::Preserved)
                .count(),
            4,
            "expected 4 Preserved edges across lhs+rhs"
        );
        assert_eq!(
            lineage
                .edges_by_evolution(TopologyEvolution::Deleted)
                .count(),
            2
        );
        assert_eq!(
            lineage
                .edges_by_evolution(TopologyEvolution::Reinterpreted)
                .count(),
            0
        );

        // Verify edges from the lhs range (0..3) and rhs range (10..13)
        // both exist with deterministic order.
        let preserved_from: Vec<TopologyFaceId> = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .filter_map(|e| e.from)
            .collect();
        // BTreeSet iteration → ascending: 0, 2, 10, 12.
        assert_eq!(
            preserved_from,
            vec![
                TopologyFaceId(0),
                TopologyFaceId(2),
                TopologyFaceId(10),
                TopologyFaceId(12),
            ]
        );

        let deleted_from: Vec<TopologyFaceId> = lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .filter_map(|e| e.from)
            .collect();
        assert_eq!(deleted_from, vec![TopologyFaceId(1), TopologyFaceId(11)]);
    }

    #[test]
    fn infer_lineage_labeled_difference_degenerate_metadata_surfaces_as_reinterpreted() {
        // Simulates Boolean::Difference's lhs-retag csgrs quirk: rhs-
        // derived faces arrive at the output labeled DEGENERATE (the
        // unmetadata sentinel from the boolean bridge — see
        // csgrs_to_tessellation in operators::boolean). Verify the
        // labeled-inference treats those collectively as a single
        // Reinterpreted edge.
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let input = labeled_mesh(positions.clone(), vec![0, 1, 2], vec![TopologyFaceId(0)]);
        let output = labeled_mesh(
            positions,
            vec![0, 1, 2, 0, 1, 2, 0, 1, 2],
            vec![
                TopologyFaceId(0),
                TopologyFaceId::DEGENERATE,
                TopologyFaceId::DEGENERATE,
            ],
        );

        let (_out, lineage) =
            infer_lineage(&input, &output, 100).expect("labeled degenerate lineage");
        // Label 0: Preserved (1 input, 1 output).
        let preserved = lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count();
        assert_eq!(preserved, 1);
        // DEGENERATE on output: collectively 1 Reinterpreted edge.
        let reint = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .count();
        assert_eq!(
            reint, 1,
            "DEGENERATE-labeled rhs faces should surface as 1 Reinterpreted edge; got {reint}"
        );
        let edge = lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .next()
            .unwrap();
        assert_eq!(edge.to, Some(TopologyFaceId::DEGENERATE));
        assert!(edge.from.is_none());
    }
}
