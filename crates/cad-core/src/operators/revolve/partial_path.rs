//! Partial-revolution algorithm for [`crate::operators::RevolveOp`].
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Sub-module of [`crate::operators::revolve`]; see that module's `//!` docs
//! for the design rationale + winding convention.
//!
//! Emits `n * (segments + 1)` vertices, `2 * n * segments` side-wall triangles,
//! and `2 * (n - 2)` cap triangles (start cap + end cap, both fan-triangulated
//! from profile vertex 0). Convexity is required on this path — the
//! fan-triangulation of caps would produce inverted triangles otherwise.

use crate::operators::OpError;
use crate::tessellation::Tessellation;

/// Partial-revolution algorithm (`angle < 2π`). Emits
/// `n * (segments + 1)` vertices, `2 * n * segments` side-wall triangles,
/// and `2 * (n - 2)` cap triangles (start cap + end cap, both
/// fan-triangulated from profile vertex 0).
pub(super) fn evaluate_partial(
    segments: u32,
    angle: f32,
    ordered: &[[f32; 2]],
    n_u32: u32,
    segments_usize: usize,
) -> Result<Tessellation, OpError> {
    let n_points = ordered.len();
    let rings = segments_usize + 1; // open ends — ring at θ=angle distinct from ring at θ=0

    // Build vertex buffer: `segments + 1` rings × `n_points` profile
    // points each. Step is `angle / segments` (NOT `2π / segments`).
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n_points * rings);
    // Cast u32 → f32 for trig parameter. `segments` is bounded in
    // practice to a few thousand (UI knob); precision loss is irrelevant.
    #[allow(
        clippy::cast_precision_loss,
        reason = "segments bounded ≤ ~thousands by UI knob; precision loss in u32→f32 angle math is well below tessellation tolerance"
    )]
    let step = angle / segments as f32;
    for ring in 0..=segments {
        #[allow(
            clippy::cast_precision_loss,
            reason = "segments bounded ≤ ~thousands by UI knob; precision loss in u32→f32 angle math is well below tessellation tolerance"
        )]
        let theta = (ring as f32) * step;
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        for [x, y] in ordered {
            positions.push([x * cos_t, *y, x * sin_t]);
        }
    }

    // Triangle counts: side walls + 2 caps.
    let side_tris = 2 * n_points * segments_usize;
    let cap_tris = 2 * (n_points - 2);
    let total_tris = side_tris + cap_tris;
    let mut indices: Vec<u32> = Vec::with_capacity(3 * total_tris);

    // Side-wall triangles. NOTE: loop is `0..segments` (NOT
    // `0..=segments`) — ring `s` and ring `s+1` form `n` quads each, so
    // `segments` quad strips × `n` quads = `n*segments` quads × 2 tris.
    // No wrap on the `s+1` axis (open ends).
    for ring in 0..segments {
        let ring_next = ring + 1;
        for edge_idx in 0..n_u32 {
            let edge_next = (edge_idx + 1) % n_u32;
            let bottom_left = ring * n_u32 + edge_idx;
            let bottom_right = ring * n_u32 + edge_next;
            let top_right = ring_next * n_u32 + edge_next;
            let top_left = ring_next * n_u32 + edge_idx;

            indices.push(bottom_left);
            indices.push(bottom_right);
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(top_right);
            indices.push(top_left);
        }
    }

    // Start cap (ring 0, θ=0): profile lies in the XY plane. The swept
    // volume extends into +Z half-space as θ increases from 0, so the
    // outward normal of the start cap is -Z. Fan triangulation from
    // vertex 0 of the ordered (CCW from +Z) profile, listed in 3D as
    // `(0, i+1, i)` to flip handedness from +Z-CCW to -Z-CCW (matches
    // ExtrudeOp's bottom-cap winding convention).
    for i in 1..(n_u32 - 1) {
        indices.push(0);
        indices.push(i + 1);
        indices.push(i);
    }

    // End cap (ring `segments`, θ=angle): profile lies in the plane
    // normal to the swept tangent at the end angle. Outward normal
    // continues in the +tangent direction. Fan triangulation from
    // vertex 0 of the end ring, listed `(end_base, end_base+i,
    // end_base+i+1)` for CCW-from-+tangent winding.
    let segments_u32 = u32::try_from(segments_usize).map_err(|_| {
        OpError::InvalidParameter(format!("revolve segments too large: {segments_usize}"))
    })?;
    let end_base = segments_u32 * n_u32;
    for i in 1..(n_u32 - 1) {
        indices.push(end_base);
        indices.push(end_base + i);
        indices.push(end_base + i + 1);
    }

    Tessellation::new(positions, indices).map_err(|e| {
        OpError::InvalidParameter(format!("RevolveOp produced invalid tessellation: {e}"))
    })
}
