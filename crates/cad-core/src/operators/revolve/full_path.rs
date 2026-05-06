//! Full-2π revolution algorithm for [`crate::operators::RevolveOp`].
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Sub-module of [`crate::operators::revolve`]; see that module's `//!` docs
//! for the design rationale + winding convention.
//!
//! Emits `n * segments` vertices and `2 * n * segments` triangles via index
//! wrap (no caps). Concave profiles are accepted on this path — the absence of
//! caps means there's no fan-triangulation that would require convexity.

use std::f32::consts::PI;

use crate::operators::OpError;
use crate::tessellation::Tessellation;

/// Full-revolution algorithm (`angle == 2π`). Emits `n * segments`
/// vertices and `2 * n * segments` triangles via index wrap (no caps).
pub(super) fn evaluate_full(
    segments: u32,
    ordered: &[[f32; 2]],
    n_u32: u32,
    segments_usize: usize,
) -> Result<Tessellation, OpError> {
    let n_points = ordered.len();
    // Build vertex buffer: `segments` rings × `n_points` profile points
    // each.
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n_points * segments_usize);
    let two_pi = 2.0 * PI;
    // Cast u32 → f32 for trig parameter. `segments` is bounded in
    // practice to a few thousand (UI knob); precision loss is irrelevant.
    #[allow(
        clippy::cast_precision_loss,
        reason = "segments bounded ≤ ~thousands by UI knob; precision loss in u32→f32 angle math is well below tessellation tolerance"
    )]
    let inv_segments = 1.0 / segments as f32;
    for ring in 0..segments {
        #[allow(
            clippy::cast_precision_loss,
            reason = "segments bounded ≤ ~thousands by UI knob; precision loss in u32→f32 angle math is well below tessellation tolerance"
        )]
        let theta = (ring as f32) * two_pi * inv_segments;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        for [x, y] in ordered {
            positions.push([x * cos_t, *y, x * sin_t]);
        }
    }

    // Build side-wall triangles. For each profile edge `edge_idx` (wrap
    // edge_idx+1 → 0) and each segment ring `ring` (wrap ring+1 → 0),
    // emit a quad split into two CCW-from-outside triangles.
    let triangle_count = 2 * n_points * segments_usize;
    let mut indices: Vec<u32> = Vec::with_capacity(3 * triangle_count);
    for ring in 0..segments {
        let ring_next = (ring + 1) % segments;
        for edge_idx in 0..n_u32 {
            let edge_next = (edge_idx + 1) % n_u32;
            let bottom_left = ring * n_u32 + edge_idx;
            let bottom_right = ring * n_u32 + edge_next;
            let top_right = ring_next * n_u32 + edge_next;
            let top_left = ring_next * n_u32 + edge_idx;

            // Quad split into 2 triangles — CCW from radially-outward
            // viewpoint.
            indices.push(bottom_left);
            indices.push(bottom_right);
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(top_right);
            indices.push(top_left);
        }
    }

    Tessellation::new(positions, indices).map_err(|e| {
        OpError::InvalidParameter(format!("RevolveOp produced invalid tessellation: {e}"))
    })
}
