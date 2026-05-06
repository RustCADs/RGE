//! Boolean operator: union / intersection / difference of two upstream tessellations.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Per [ADR-112](../../../docs/adr/ADR-112-cad-boolean-csg-library.md). Backed
//! by `csgrs` (pure-Rust BSP-tree triangle-mesh CSG). The bridge converts
//! cad-core's triangle-soup [`Tessellation`] into csgrs's `Mesh` representation,
//! runs the boolean via [`CSG`], converts back, and preserves labels when
//! present.
//!
//! # Unified labeled / unlabeled paths (2026-05-08 unified-mesh refactor)
//!
//! [`BooleanOp::evaluate`] handles both unlabeled and labeled inputs in a
//! single signature. Both unlabeled → unlabeled output (legacy bit-identical).
//! Both labeled → labeled output (was `evaluate_labeled`). Mixed → labeled
//! output, unlabeled side synthesizes per-triangle [`TopologyFaceId::DEGENERATE`]
//! labels (downstream lineage classifies as Reinterpreted).
//!
//! # csgrs features
//!
//! csgrs 0.20.1 `default-features = false` + `["f64", "earcut"]`. f64 avoids
//! the rapier3d 0.24/0.32 conflict (workspace pins 0.32 in `crates/physics`).
//! The bridge converts `f32` ↔ `f64` at the boundary. Per ADR-112 §"csgrs
//! feature flags". T-junction handling deferred per csgrs upstream TODO.
//!
//! # Capability surface (ADR-104/112)
//!
//! `boolean_robust_under_tolerance: false`, `healing_strategies: none`,
//! `deterministic_triangulation: true` (gated by 200-iter soak in
//! `cad_boolean_determinism.rs`).
//!
//! # Failure handling
//!
//! csgrs's BSP can panic on degenerate input. Mitigation: pre-filter
//! degenerate triangles in [`tessellation_to_csgrs`], wrap the op in
//! [`std::panic::catch_unwind`] surfacing as [`OpError::InvalidParameter`].
//! Snapshot-recoverable per PLAN §1.13.

use std::fmt::Debug;
use std::panic::AssertUnwindSafe;

use csgrs::mesh::polygon::Polygon as CsgrsPolygon;
use csgrs::mesh::vertex::Vertex as CsgrsVertex;
use csgrs::mesh::Mesh as CsgrsMesh;
use csgrs::traits::CSG;
use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::{Tessellation, TopologyFaceId};

// ---------------------------------------------------------------------------
// BooleanMode + BooleanOp
// ---------------------------------------------------------------------------

/// The boolean operation mode applied by [`BooleanOp`] to its two inputs.
///
/// Per ADR-112's API-shape recommendation. Note that [`BooleanMode::Xor`] is
/// intentionally NOT exposed at this milestone — csgrs supports it but ADR-112
/// did not include it, and we'll add it when a use case appears.
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
    fn discriminant(self) -> u8 {
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
/// The local [`BooleanOp::structural_hash`] depends only on [`BooleanMode`];
/// upstream tessellations contribute via [`crate::OperatorGraph::evaluate`]'s
/// recursive `effective_hash`, which folds in the `lhs` and `rhs` upstream
/// hashes by port index.
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
            self.evaluate_with_labels(lhs, rhs)
        } else {
            self.evaluate_unlabeled(lhs, rhs)
        }
    }
}

impl BooleanOp {
    /// Unlabeled fast path — both inputs lack labels, output is unlabeled.
    /// Bit-identical to the pre-refactor `evaluate`.
    fn evaluate_unlabeled(
        &self,
        lhs: &Tessellation,
        rhs: &Tessellation,
    ) -> Result<Tessellation, OpError> {
        // Convert both inputs to csgrs Mesh<()>. () is the no-payload metadata
        // type; this is the unlabeled path that drops any lineage info.
        let lhs_mesh: CsgrsMesh<()> = tessellation_to_csgrs(&lhs.positions, &lhs.indices, |_| ());
        let rhs_mesh: CsgrsMesh<()> = tessellation_to_csgrs(&rhs.positions, &rhs.indices, |_| ());

        let result = run_boolean(self.mode, &lhs_mesh, &rhs_mesh)?;

        // Convert the csgrs result back to triangle-soup Tessellation.
        let (positions, indices, _labels) = csgrs_to_tessellation::<()>(&result, || ())?;
        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("boolean failed: invalid output tessellation: {e}"))
        })
    }

    /// Labeled path — at least one input carries labels. Per-triangle
    /// [`TopologyFaceId`] labels thread through csgrs's polygon metadata so
    /// downstream lineage can recover originating-face identity with high
    /// confidence. Mixed inputs synthesize [`TopologyFaceId::DEGENERATE`]
    /// labels on the unlabeled side; downstream lineage classifies those
    /// as Reinterpreted.
    ///
    /// csgrs metadata semantics (per ADR-112 §"Followups" 30-min spike):
    /// Union and Intersection preserve polygon metadata through plane
    /// splits / `clip_polygons`. **Difference** retags rhs's clipped
    /// polygons with `self.metadata` (lhs's `Mesh::metadata` = None) — a
    /// known csgrs quirk. Those rhs-derived faces fall back to
    /// [`TopologyFaceId::DEGENERATE`] via `csgrs_to_tessellation`'s
    /// unmetadata sentinel, routing them through Reinterpreted.
    fn evaluate_with_labels(
        &self,
        lhs: &Tessellation,
        rhs: &Tessellation,
    ) -> Result<Tessellation, OpError> {
        let lhs_labels = derive_per_triangle_labels(lhs);
        let rhs_labels = derive_per_triangle_labels(rhs);

        let lhs_mesh: CsgrsMesh<TopologyFaceId> =
            tessellation_to_csgrs(&lhs.positions, &lhs.indices, |tri_idx| lhs_labels[tri_idx]);
        let rhs_mesh: CsgrsMesh<TopologyFaceId> =
            tessellation_to_csgrs(&rhs.positions, &rhs.indices, |tri_idx| rhs_labels[tri_idx]);

        let result = run_boolean(self.mode, &lhs_mesh, &rhs_mesh)?;

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

/// Shared implementation of the boolean dispatch + panic catch — used by
/// both the unlabeled and labeled paths.
fn run_boolean<S>(
    mode: BooleanMode,
    lhs_mesh: &CsgrsMesh<S>,
    rhs_mesh: &CsgrsMesh<S>,
) -> Result<CsgrsMesh<S>, OpError>
where
    S: Clone + Send + Sync + Debug + 'static,
{
    // Run the boolean inside catch_unwind. csgrs's BSP can panic on
    // pathological input (e.g. all-coincident vertices that survived our
    // pre-filter, very-near-degenerate triangles). We surface those as
    // InvalidParameter rather than poisoning the caller.
    std::panic::catch_unwind(AssertUnwindSafe(|| match mode {
        BooleanMode::Union => lhs_mesh.union(rhs_mesh),
        BooleanMode::Intersection => lhs_mesh.intersection(rhs_mesh),
        BooleanMode::Difference => lhs_mesh.difference(rhs_mesh),
    }))
    .map_err(|_| {
        OpError::InvalidParameter(
            "boolean failed: csgrs panicked on pathological input".to_string(),
        )
    })
}

// ---------------------------------------------------------------------------
// Conversion bridge
// ---------------------------------------------------------------------------

/// Compute the right-hand-rule outward normal of a triangle defined by three
/// f64 positions in CCW order. Returns a zero-vector for degenerate triangles
/// (the caller filters those before reaching here, but be defensive).
fn triangle_normal_f64(a: Point3<f64>, b: Point3<f64>, c: Point3<f64>) -> Vector3<f64> {
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(&ac);
    let n_sq = n.norm_squared();
    if n_sq > 0.0 {
        n / n_sq.sqrt()
    } else {
        Vector3::zeros()
    }
}

/// Convert a cad-core triangle-soup mesh (positions + indices) to a csgrs
/// [`Mesh<M>`] carrying per-polygon metadata `M`.
///
/// Each input triangle becomes a 3-vertex csgrs [`CsgrsPolygon`]. Per-vertex
/// normals are set to the triangle face normal (right-hand rule from CCW
/// winding); csgrs uses these for BSP plane orientation, and uniform per-face
/// normals are correct for triangle soup.
///
/// The `metadata` closure is invoked once per **input triangle** (in input-
/// order, after degenerate filtering — degenerate triangles do not consume a
/// metadata slot but still advance `triangle_idx`). The returned `M` is
/// stored on the resulting `CsgrsPolygon`'s `metadata` field as `Some(M)`.
/// Pass `|_| ()` for the no-metadata path.
///
/// Degenerate triangles (zero-area or zero-length edges) are filtered out
/// before [`CsgrsPolygon::new`] is called, since csgrs asserts `>= 3 distinct
/// vertices` and panics on degenerate planes.
fn tessellation_to_csgrs<M>(
    positions: &[[f32; 3]],
    indices: &[u32],
    metadata: impl Fn(usize) -> M,
) -> CsgrsMesh<M>
where
    M: Clone + Send + Sync + Debug + 'static,
{
    // Pre-allocate with the upper bound: `triangle_count` polygons (some may
    // be filtered for degeneracy, so the actual count may be lower).
    let triangle_count = indices.len() / 3;
    let mut polygons: Vec<CsgrsPolygon<M>> = Vec::with_capacity(triangle_count);

    for (tri_idx, tri) in indices.chunks_exact(3).enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        // Tessellation::new validated bounds, but be defensive.
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        // Convert f32 → f64 at the boundary.
        let a = Point3::new(f64::from(p0[0]), f64::from(p0[1]), f64::from(p0[2]));
        let b = Point3::new(f64::from(p1[0]), f64::from(p1[1]), f64::from(p1[2]));
        let c = Point3::new(f64::from(p2[0]), f64::from(p2[1]), f64::from(p2[2]));

        // Filter degenerate triangles: any pair coincident OR area near zero.
        let normal = triangle_normal_f64(a, b, c);
        if normal == Vector3::zeros() {
            continue;
        }

        let v0 = CsgrsVertex::new(a, normal);
        let v1 = CsgrsVertex::new(b, normal);
        let v2 = CsgrsVertex::new(c, normal);

        polygons.push(CsgrsPolygon::new(vec![v0, v1, v2], Some(metadata(tri_idx))));
    }

    CsgrsMesh::from_polygons(&polygons, None)
}

/// Triangle-soup output of [`csgrs_to_tessellation`]: positions, indices,
/// per-output-triangle metadata. Aliased so the function signature isn't a
/// clippy `type_complexity` violation.
type TriangleSoupWithLabels<M> = (Vec<[f32; 3]>, Vec<u32>, Vec<M>);

/// Convert a csgrs [`Mesh<M>`] back to triangle-soup buffers + a per-output-
/// triangle metadata vector. Polygons with `N > 3` vertices are
/// fan-triangulated from `vertex[0]` (csgrs's polygons are coplanar by
/// construction so fan-triangulation is valid). Each output triangle clones
/// its source polygon's metadata.
///
/// Vertex dedup uses exact f32 bit equality after the f64 → f32 conversion
/// (12-byte LE-byte key). Required for BLAKE3-determinism downstream —
/// tolerance comparisons would yield non-deterministic indexings.
///
/// Polygons with csgrs `metadata = None` (rhs-derived under Difference's
/// lhs-retag quirk) yield `unmetadata_label()`. Unlabeled path: `()`.
/// Labeled path: [`TopologyFaceId::DEGENERATE`] (routes through Reinterpreted).
///
/// # Errors
///
/// * [`OpError::InvalidParameter`] on non-finite output position or
///   `u32::MAX` vertex count overflow.
fn csgrs_to_tessellation<M>(
    mesh: &CsgrsMesh<M>,
    unmetadata_label: impl Fn() -> M,
) -> Result<TriangleSoupWithLabels<M>, OpError>
where
    M: Clone + Send + Sync + Debug + 'static,
{
    use std::collections::BTreeMap;

    // Vertex de-dup map: keyed on the 12-byte f32 little-endian bit pattern of
    // (x, y, z) so determinism is exact across iterations. BTreeMap ensures
    // deterministic iteration order; the actual key bits are derived from the
    // *order in which we encounter* a vertex, not from the BTreeMap's sort, so
    // sort order doesn't affect output.
    let mut dedup: BTreeMap<[u8; 12], u32> = BTreeMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut labels: Vec<M> = Vec::new();

    let mut intern = |pos_f32: [f32; 3]| -> Option<u32> {
        let mut key = [0u8; 12];
        key[0..4].copy_from_slice(&pos_f32[0].to_le_bytes());
        key[4..8].copy_from_slice(&pos_f32[1].to_le_bytes());
        key[8..12].copy_from_slice(&pos_f32[2].to_le_bytes());

        if let Some(&existing) = dedup.get(&key) {
            return Some(existing);
        }
        // u32::MAX is used as "no more slots"; we never expect to hit that
        // for realistic boolean output but guard against overflow.
        let new_index = u32::try_from(positions.len()).ok()?;
        positions.push(pos_f32);
        dedup.insert(key, new_index);
        Some(new_index)
    };

    for poly in &mesh.polygons {
        let n = poly.vertices.len();
        if n < 3 {
            // csgrs shouldn't emit < 3 vertex polygons but be defensive.
            continue;
        }

        // Pre-intern all vertex indices for this polygon.
        let mut vertex_indices: Vec<u32> = Vec::with_capacity(n);
        let mut had_overflow = false;
        for v in &poly.vertices {
            // f64 → f32 conversion at the boundary.
            #[allow(clippy::cast_possible_truncation)]
            let pos_f32 = [v.pos.x as f32, v.pos.y as f32, v.pos.z as f32];
            // Reject NaN / infinite outputs from csgrs (snap to error).
            if !pos_f32[0].is_finite() || !pos_f32[1].is_finite() || !pos_f32[2].is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "boolean failed: csgrs produced non-finite vertex {pos_f32:?}"
                )));
            }
            if let Some(idx) = intern(pos_f32) {
                vertex_indices.push(idx);
            } else {
                had_overflow = true;
                break;
            }
        }
        if had_overflow {
            return Err(OpError::InvalidParameter(
                "boolean failed: vertex count exceeds u32::MAX".to_string(),
            ));
        }

        // Pull the polygon's metadata once; clone per emitted fan triangle.
        // csgrs's Polygon::metadata is Option<M>; if None (which Difference
        // can produce for rhs-derived faces per the lhs-retag quirk), fall
        // back to the caller-supplied unmetadata sentinel. For the labeled
        // path that's TopologyFaceId::DEGENERATE — a sentinel that's
        // distinct from every real input face id and is treated as
        // "Reinterpreted" by the downstream inference.
        let poly_meta: M = poly.metadata.clone().unwrap_or_else(&unmetadata_label);

        // Fan-triangulate from vertex 0: (0, i, i+1) for i in 1..n-1.
        for i in 1..n - 1 {
            let i0 = vertex_indices[0];
            let i1 = vertex_indices[i];
            let i2 = vertex_indices[i + 1];
            // Skip fan triangles that collapse to coincident indices (can
            // happen if two polygon vertices ended up bit-identical after
            // f32 conversion).
            if i0 == i1 || i1 == i2 || i0 == i2 {
                continue;
            }
            indices.push(i0);
            indices.push(i1);
            indices.push(i2);
            labels.push(poly_meta.clone());
        }
    }

    Ok((positions, indices, labels))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{CuboidOp, Operator};
    use crate::topo_lineage::label_by_plane;

    /// Helper: an axis-aligned cube of `size` centered at `(cx, cy, cz)`.
    fn cube_at(cx: f32, cy: f32, cz: f32, size: f32) -> Tessellation {
        let h = size * 0.5;
        let positions = vec![
            [cx - h, cy - h, cz - h],
            [cx + h, cy - h, cz - h],
            [cx + h, cy + h, cz - h],
            [cx - h, cy + h, cz - h],
            [cx - h, cy - h, cz + h],
            [cx + h, cy - h, cz + h],
            [cx + h, cy + h, cz + h],
            [cx - h, cy + h, cz + h],
        ];
        // Right-handed CCW outward winding (matches CuboidOp).
        #[rustfmt::skip]
        let indices = vec![
            0, 3, 2,  0, 2, 1,  // -Z
            4, 5, 6,  4, 6, 7,  // +Z
            0, 1, 5,  0, 5, 4,  // -Y
            3, 7, 6,  3, 6, 2,  // +Y
            0, 4, 7,  0, 7, 3,  // -X
            1, 2, 6,  1, 6, 5,  // +X
        ];
        Tessellation::new(positions, indices).expect("valid cube")
    }

    fn unit_cube_origin() -> Tessellation {
        let op = CuboidOp::default();
        op.evaluate(&[]).expect("default cube")
    }

    /// Helper: build a labeled [`Tessellation`] from a Tessellation by
    /// labeling each triangle with the supplied `face_id`.
    fn labeled_with_id(tess: &Tessellation, face_id: TopologyFaceId) -> Tessellation {
        let labels = vec![face_id; tess.triangle_count()];
        Tessellation::with_labels(tess.positions.clone(), tess.indices.clone(), labels)
            .expect("test labeled mesh ctor")
    }

    /// Test 1 — arity must be 2.
    #[test]
    fn boolean_op_arity_is_2() {
        for mode in [
            BooleanMode::Union,
            BooleanMode::Intersection,
            BooleanMode::Difference,
        ] {
            let op = BooleanOp::new(mode);
            assert_eq!(op.arity(), 2);
        }
    }

    /// Test 2 — different modes produce different local structural hashes.
    #[test]
    fn boolean_op_structural_hash_differs_per_mode() {
        let u = BooleanOp::union().structural_hash();
        let i = BooleanOp::intersection().structural_hash();
        let d = BooleanOp::difference().structural_hash();
        assert_ne!(u, i);
        assert_ne!(u, d);
        assert_ne!(i, d);
    }

    /// Test 3 — same mode → same hash, deterministic across constructions.
    #[test]
    fn boolean_op_structural_hash_deterministic() {
        let a = BooleanOp::union();
        let b = BooleanOp::new(BooleanMode::Union);
        assert_eq!(a.structural_hash(), b.structural_hash());

        let c = BooleanOp::intersection();
        let d = BooleanOp::new(BooleanMode::Intersection);
        assert_eq!(c.structural_hash(), d.structural_hash());
    }

    /// Test 4 — wrong arity input is rejected before reaching csgrs.
    #[test]
    fn boolean_evaluate_rejects_wrong_arity() {
        let op = BooleanOp::union();
        let cube = unit_cube_origin();

        // 0 inputs.
        let err0 = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err0,
            OpError::WrongArity {
                expected: 2,
                got: 0
            }
        ));

        // 1 input.
        let err1 = op.evaluate(&[&cube]).unwrap_err();
        assert!(matches!(
            err1,
            OpError::WrongArity {
                expected: 2,
                got: 1
            }
        ));
    }

    /// Test 5 — union of two disjoint unit cubes far apart preserves both.
    /// Disjoint inputs shouldn't merge, so the union's bounding box must
    /// include both cubes' extents.
    #[test]
    fn union_two_disjoint_unit_cubes_yields_combined_mesh() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs = cube_at(10.0, 0.0, 0.0, 1.0); // far apart on +X
        let op = BooleanOp::union();
        let result = op.evaluate(&[&lhs, &rhs]).expect("union eval");

        // The union must contain vertices spanning both cubes.
        let xs: Vec<f32> = result.positions.iter().map(|[x, _, _]| *x).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        assert!(min_x <= -0.49, "union lost the lhs cube: min_x = {min_x}");
        assert!(max_x >= 10.49, "union lost the rhs cube: max_x = {max_x}");
        // Output must be non-empty.
        assert!(result.vertex_count() > 0);
        assert!(result.triangle_count() > 0);
        // Both inputs were unlabeled → output is unlabeled.
        assert!(
            !result.is_labeled(),
            "two unlabeled inputs should yield unlabeled output"
        );
    }

    /// Test 6 — union of overlapping cubes has FEWER vertices than naive
    /// concat. The two cubes overlap in the corner-cube `[0,0.5]³`, so the
    /// boolean must clip / merge faces.
    #[test]
    fn union_two_overlapping_unit_cubes_is_smaller_than_naive_concat() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // offset so they overlap
        let op = BooleanOp::union();
        let result = op.evaluate(&[&lhs, &rhs]).expect("union eval");

        // The boolean output must be non-empty.
        assert!(result.vertex_count() > 0);
        assert!(result.triangle_count() > 0);

        // Output bounding box must span the union (not be cut off).
        let xs: Vec<f32> = result.positions.iter().map(|[x, _, _]| *x).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(min_x <= -0.49, "lost lhs extents: min_x = {min_x}");
        assert!(max_x >= 0.99, "lost rhs extents: max_x = {max_x}");
    }

    /// Test 7 — intersection of disjoint cubes is empty (or returns an
    /// `EmptyResult` / `InvalidParameter`). The dispatch ADR §"Implementation
    /// guidance" says assert which behavior occurs and lock it in.
    #[test]
    fn intersection_two_disjoint_cubes_is_empty_or_diagnostic() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs = cube_at(10.0, 0.0, 0.0, 1.0); // disjoint
        let op = BooleanOp::intersection();
        let result = op.evaluate(&[&lhs, &rhs]);

        match result {
            Ok(t) => {
                // csgrs returns an empty Mesh for disjoint intersection;
                // the bridge converts that to a 0-vertex Tessellation.
                assert_eq!(t.vertex_count(), 0, "empty intersection expected");
                assert_eq!(t.triangle_count(), 0);
            }
            Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {
                // Acceptable alternative: csgrs may surface its own
                // diagnostic. Either branch is locked in.
            }
            Err(other) => {
                panic!("unexpected error from disjoint-intersection: {other:?}");
            }
        }
    }

    /// Test 8 — intersection of overlapping cubes yields a smaller box.
    /// Two unit cubes shifted by `(0.5, 0.5, 0.5)` overlap in a `0.5³` cube.
    #[test]
    fn intersection_overlapping_cubes_yields_central_box() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs = cube_at(0.5, 0.5, 0.5, 1.0);
        let op = BooleanOp::intersection();
        let result = op.evaluate(&[&lhs, &rhs]).expect("intersection eval");

        assert!(result.vertex_count() > 0, "intersection must be non-empty");
        assert!(result.triangle_count() > 0);

        // The intersection volume is `[0,0.5] × [0,0.5] × [0,0.5]` plus a
        // small numeric tolerance.
        for [x, y, z] in &result.positions {
            assert!(
                *x >= -0.001 && *x <= 0.501,
                "intersection vertex x = {x} outside [0, 0.5]"
            );
            assert!(
                *y >= -0.001 && *y <= 0.501,
                "intersection vertex y = {y} outside [0, 0.5]"
            );
            assert!(
                *z >= -0.001 && *z <= 0.501,
                "intersection vertex z = {z} outside [0, 0.5]"
            );
        }
    }

    /// Test 9 — difference `cube_a` − `cube_b` (offset overlap) yields a dented
    /// version of `cube_a` with strictly more vertices than the original 8-vert
    /// cube (since the dent introduces additional clip vertices).
    #[test]
    fn difference_a_minus_b_with_overlap_yields_dented_a() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0); // 8 verts
        let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping corner
        let op = BooleanOp::difference();
        let result = op.evaluate(&[&lhs, &rhs]).expect("difference eval");

        assert!(result.vertex_count() > 0);
        // The dent must add vertices vs. the 8-vert original (typically the
        // dent creates 7+ additional clip vertices on the corner cube).
        assert!(
            result.vertex_count() > 8,
            "expected > 8 vertices after dent; got {}",
            result.vertex_count()
        );

        // Difference must NOT extend into the cube_b corner: vertices in the
        // ([0.5,1] × [0.5,1] × [0.5,1]) sub-cube interior are removed.
        // Allow boundary vertices (x == 0.5, etc.) since the boolean cuts
        // along those planes.
        let lhs_extent = 0.5 + 0.001;
        let mut found_inside_dent = false;
        for [x, y, z] in &result.positions {
            // We classify "inside the dent" as strictly above 0.5 in all
            // three axes (i.e. interior of the carved-out corner). Boundary
            // vertices on the cut plane are fine.
            if *x > 0.5 + 0.01 && *y > 0.5 + 0.01 && *z > 0.5 + 0.01 {
                found_inside_dent = true;
                break;
            }
            // Also assert no vertex extends past the lhs's outer boundary.
            assert!(
                *x <= lhs_extent && *y <= lhs_extent && *z <= lhs_extent,
                "vertex {} outside lhs extents",
                format_args!("({x},{y},{z})")
            );
        }
        assert!(
            !found_inside_dent,
            "difference output has vertices inside the cut-out region"
        );
    }

    /// Test 10 — `a - b ≠ b - a` (difference is non-commutative).
    #[test]
    fn difference_is_non_commutative() {
        let a = cube_at(0.0, 0.0, 0.0, 1.0);
        let b = cube_at(0.5, 0.5, 0.5, 1.0);
        let op = BooleanOp::difference();

        let a_minus_b = op.evaluate(&[&a, &b]).expect("a - b");
        let b_minus_a = op.evaluate(&[&b, &a]).expect("b - a");

        // Centroids differ: a−b is centered around (≈-0.1, -0.1, -0.1) (with
        // the dent removing the +x/+y/+z corner), b−a is centered around
        // (≈+0.6, +0.6, +0.6) (with the dent removing the -x/-y/-z corner).
        let centroid = |t: &Tessellation| -> [f32; 3] {
            #[allow(clippy::cast_precision_loss)]
            let n = t.positions.len() as f32;
            let sum = t.positions.iter().fold([0.0_f32, 0.0, 0.0], |acc, p| {
                [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
            });
            [sum[0] / n, sum[1] / n, sum[2] / n]
        };

        let ca = centroid(&a_minus_b);
        let cb = centroid(&b_minus_a);
        assert!(
            (ca[0] - cb[0]).abs() > 0.05
                || (ca[1] - cb[1]).abs() > 0.05
                || (ca[2] - cb[2]).abs() > 0.05,
            "a-b and b-a centroids too close: ca={ca:?}, cb={cb:?}"
        );
    }

    /// Test 11 — near-degenerate input is handled gracefully (no panic).
    /// Tiny cube intersected with a normal cube: the result is either
    /// correct-tiny or an `InvalidParameter` — but never a panic.
    #[test]
    fn near_degenerate_input_handled_gracefully() {
        let tiny = cube_at(0.0, 0.0, 0.0, 1e-6); // 1µm cube
        let big = cube_at(0.0, 0.0, 0.0, 1.0);
        let op = BooleanOp::intersection();
        let result = op.evaluate(&[&tiny, &big]);

        match result {
            Ok(_) | Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {
                // Either ok or clean diagnostic — both acceptable.
            }
            Err(other) => panic!("unexpected error class on tiny input: {other:?}"),
        }
    }

    /// Test 12 — pathological input yields a diagnostic, not a panic.
    /// All three positions of every triangle are at the origin → all
    /// triangles are degenerate (zero area). Our bridge filters them, so
    /// csgrs sees an empty mesh and the boolean is well-defined (empty).
    #[test]
    fn boolean_returns_diagnostic_not_panic_on_pathological_input() {
        let positions = vec![[0.0_f32, 0.0, 0.0]];
        let indices = vec![0, 0, 0]; // a single degenerate triangle
        let pathological = Tessellation::new(positions, indices).expect("ctor");
        let cube = cube_at(0.0, 0.0, 0.0, 1.0);
        let op = BooleanOp::union();

        // Should not panic. Either Ok(some-output) or a clean error.
        let result = op.evaluate(&[&pathological, &cube]);
        match result {
            Ok(_) | Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {}
            Err(other) => panic!("unexpected error class on pathological input: {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // Labeled-input dispatch (csgrs metadata-passthrough path)
    // -----------------------------------------------------------------

    /// Both unlabeled inputs → unlabeled output. Bit-identical legacy path.
    #[test]
    fn boolean_evaluate_with_both_unlabeled_inputs_returns_unlabeled_output() {
        let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
        let op = BooleanOp::union();
        let out = op.evaluate(&[&lhs, &rhs]).expect("union eval");
        assert!(
            !out.is_labeled(),
            "two unlabeled inputs must produce unlabeled output"
        );
    }

    /// Both labeled inputs → labeled output. Was `evaluate_labeled`.
    #[test]
    fn boolean_evaluate_with_both_labeled_inputs_returns_labeled_output() {
        let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
        let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
        let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));
        let op = BooleanOp::union();
        let out = op.evaluate(&[&lhs, &rhs]).expect("union labeled");
        assert!(
            out.is_labeled(),
            "two labeled inputs must produce labeled output"
        );

        // Output has triangles. Labels must include 0 (from lhs) AND 1
        // (from rhs) — csgrs preserves polygon metadata for Union.
        assert!(out.triangle_count() > 0);
        let labels = out.face_labels().expect("labeled");
        let mut has_0 = false;
        let mut has_1 = false;
        for &lbl in labels {
            if lbl == TopologyFaceId(0) {
                has_0 = true;
            }
            if lbl == TopologyFaceId(1) {
                has_1 = true;
            }
        }
        assert!(has_0, "union must carry lhs label 0 through");
        assert!(has_1, "union must carry rhs label 1 through");
    }

    /// Mixed: lhs labeled, rhs unlabeled → labeled output (rhs synthesizes
    /// DEGENERATE labels, which surface as Reinterpreted in lineage).
    #[test]
    fn boolean_evaluate_with_one_labeled_one_unlabeled_input() {
        let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0);
        let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(42));
        assert!(!rhs_tess.is_labeled());

        let op = BooleanOp::union();
        let out = op.evaluate(&[&lhs, &rhs_tess]).expect("mixed union");
        assert!(out.is_labeled(), "labeled lhs must propagate to output");
        assert!(out.triangle_count() > 0);
        let labels = out.face_labels().expect("labeled output");
        assert!(
            labels.contains(&TopologyFaceId(42)),
            "lhs label 42 must propagate through union"
        );
    }

    /// Reverse mixed: lhs unlabeled, rhs labeled → labeled output.
    #[test]
    fn boolean_evaluate_with_unlabeled_lhs_labeled_rhs() {
        let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0); // unlabeled
        let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
        let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(7));
        assert!(!lhs_tess.is_labeled());

        let op = BooleanOp::union();
        let out = op.evaluate(&[&lhs_tess, &rhs]).expect("mixed union");
        assert!(out.is_labeled());
        assert!(out.triangle_count() > 0);
        // rhs label 7 should propagate through union.
        let labels = out.face_labels().expect("labeled");
        assert!(
            labels.contains(&TopologyFaceId(7)),
            "rhs label 7 must propagate through union"
        );
    }

    /// Test 14 — Intersection carries through both lhs and rhs labels on
    /// the overlap region (csgrs preserves polygon metadata as-is from
    /// clipping).
    #[test]
    fn boolean_evaluate_carries_lhs_label_through_intersection() {
        let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
        let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
        let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));

        let op = BooleanOp::intersection();
        let out = op.evaluate(&[&lhs, &rhs]).expect("intersection labeled");

        assert!(out.is_labeled());
        assert!(out.triangle_count() > 0);
        let labels: std::collections::BTreeSet<TopologyFaceId> = out
            .face_labels()
            .expect("labeled")
            .iter()
            .copied()
            .collect();
        assert!(
            labels.contains(&TopologyFaceId(0)) || labels.contains(&TopologyFaceId(1)),
            "intersection must carry at least one of the input labels through; got {labels:?}"
        );
    }

    /// Difference's csgrs lhs-retag quirk: rhs's clipped polygons are retagged
    /// with `Mesh::metadata` (we pass None) so they arrive as DEGENERATE per
    /// ADR-112. lhs-derived survivors keep their metadata.
    #[test]
    fn boolean_evaluate_difference_retags_rhs_as_lhs_per_csgrs_quirk() {
        let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
        let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
        let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
        let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));

        let op = BooleanOp::difference();
        let out = op.evaluate(&[&lhs, &rhs]).expect("difference labeled");

        assert!(out.is_labeled());
        assert!(out.triangle_count() > 0);
        let labels: std::collections::BTreeSet<TopologyFaceId> = out
            .face_labels()
            .expect("labeled")
            .iter()
            .copied()
            .collect();

        // lhs label (0) should still appear on at least one surviving
        // output triangle (lhs's faces that were not clipped retain
        // their metadata).
        assert!(
            labels.contains(&TopologyFaceId(0)),
            "Difference output must still carry the lhs label 0 on lhs-survivor faces; got {labels:?}"
        );
        assert!(
            !labels.contains(&TopologyFaceId(1)),
            "Difference output unexpectedly preserved rhs label 1; csgrs retagged rhs polygons \
             with lhs's Mesh::metadata (None), so rhs-derived faces should arrive as DEGENERATE \
             or be absent. Got labels: {labels:?}"
        );
    }

    /// Round-trip: a cube labeled by plane (6 distinct labels for the 6
    /// axis-aligned faces) survives a no-op self-union evaluate.
    #[test]
    fn boolean_evaluate_label_by_plane_round_trip_through_self_union() {
        let cube_tess = unit_cube_origin();
        let labeled = label_by_plane(&cube_tess, 0).expect("label cube");
        // 6 distinct face ids on a unit cube (one per axis-aligned face).
        assert_eq!(labeled.face_count(), Some(6));

        let op = BooleanOp::union();
        let out = op
            .evaluate(&[&labeled, &labeled])
            .expect("self-union labeled");

        assert!(out.is_labeled());
        let in_labels: std::collections::BTreeSet<TopologyFaceId> = labeled
            .face_labels()
            .expect("labeled")
            .iter()
            .copied()
            .collect();
        let out_labels: std::collections::BTreeSet<TopologyFaceId> = out
            .face_labels()
            .expect("labeled")
            .iter()
            .copied()
            .collect();
        for &id in in_labels.iter().filter(|id| !id.is_degenerate()) {
            assert!(
                out_labels.contains(&id),
                "label {id} from input is missing on self-union output; out_labels = {out_labels:?}"
            );
        }
    }
}
