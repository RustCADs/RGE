//! Plane-based triangulation labeling for [`crate::Tessellation`].
//!
//! Failure class: snapshot-recoverable (inherited).
//!
//! Sub-module of [`crate::topo_lineage::infer`]; see the parent module's
//! `//!` docs for the design rationale.

use std::collections::HashMap;

use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topo_lineage::plane::QuantizedPlane;
use crate::topo_lineage::types::LineageError;

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
