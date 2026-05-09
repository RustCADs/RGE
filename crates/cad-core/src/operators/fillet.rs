//! `FilletOp` — first real consumer of the [`BRepEdgeId`] substrate.
//!
//! D-Fillet sub-α: Cuboid-only fillet operator that takes a list of
//! [`BRepEdgeId`]s plus a radius, validates each edge against the
//! upstream Cuboid's [`BRepEdgeProvider`], and produces a bounded
//! geometric change per selected edge.
//!
//! Failure class: snapshot-recoverable.
//!
//! # Scope (sub-α)
//!
//! * Upstream operator: [`CuboidOp`] only. Extrude / Revolve / Loft
//!   fillet variants are subsequent sub-dispatches.
//! * Geometry: **chamfer approximation**, NOT round-fillet kernel.
//!   For each filleted edge, the 2 endpoint corners gain an inward-
//!   offset replica vertex and 2 chamfer-cap triangles connect them.
//!   Per filleted edge: +2 vertices, +2 triangles. Linear in
//!   selection count.
//! * Real round-fillet geometry (quarter-cylinder tessellation,
//!   face-strip removal, multi-edge corner blending, curvature
//!   continuity) is OUT OF SCOPE.
//!
//! # NON-GOALS
//!
//! * No `impl BRepProvider for FilletOp` (output-side face identity).
//! * No `impl BRepEdgeProvider for FilletOp` (output-side edge identity).
//! * No general fillet kernel.
//! * No Boolean / Sweep / non-Cuboid input.
//! * No multi-edge corner-sharing geometry. The chamfer is per-edge
//!   independent; if two filleted edges share a corner, the geometry
//!   may be visually weird, but the substrate-validation test does
//!   not exercise that case.
//!
//! # Pattern: BRepEdgeId-as-constructor-parameter
//!
//! This is the first operator to consume [`BRepEdgeId`] in its
//! constructor. The validation pattern (resolve each ID against
//! the upstream's [`BRepEdgeProvider`], reject unknown IDs) is the
//! precedent for future similar operators (Chamfer, Shell, EdgeBlend).
//!
//! Today FilletOp falls into the catch-all in
//! [`crate::topology::resolve::brep_face_ids_for_node`] /
//! [`crate::topology::edge_resolve::brep_edge_ids_for_node`] and
//! returns
//! [`crate::topology::BRepResolveError::TopologyChangingOperator`] —
//! correct, since it changes topology (adds vertices/triangles) and
//! does not provide its own face/edge identity in sub-α.

use serde::{Deserialize, Serialize};

use super::CuboidOp;
use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;
use crate::topology::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, CuboidFaceTag};

// ---------------------------------------------------------------------------
// FilletError
// ---------------------------------------------------------------------------

/// Construction-time errors for [`FilletOp::new`].
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum FilletError {
    /// `radius` must be finite and strictly positive.
    #[error("fillet radius must be finite and > 0; got {radius}")]
    InvalidRadius {
        /// The offending radius value.
        radius: f32,
    },

    /// Caller passed an empty edge selection — degenerate operator.
    #[error("fillet edge list is empty; degenerate operator")]
    EmptyEdgeSelection,

    /// One of the supplied [`BRepEdgeId`]s does not match any edge
    /// emitted by the upstream Cuboid's [`BRepEdgeProvider`].
    #[error("edge id {edge:?} does not appear in upstream Cuboid's BRepEdgeProvider output")]
    EdgeNotInUpstream {
        /// The unknown edge id.
        edge: BRepEdgeId,
    },
}

// ---------------------------------------------------------------------------
// FilletOp
// ---------------------------------------------------------------------------

/// FilletOp sub-α — bounded chamfer along selected Cuboid edges.
///
/// Constructed via [`Self::new`] which validates each edge against
/// the upstream Cuboid's [`BRepEdgeProvider`] and resolves each
/// [`BRepEdgeId`] back to the underlying [`CuboidFaceTag`] pair so
/// evaluation can locate the geometry without holding a graph
/// reference.
///
/// Arity 1 — takes the upstream Cuboid's tessellation as input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilletOp {
    /// Selected edges by stable identity. Mirrors the user-facing
    /// API surface.
    edges: Vec<BRepEdgeId>,
    /// Resolved local face-tag pairs — one per selected edge, in
    /// the same order. Used at evaluation time to locate cuboid
    /// vertices without re-resolving via graph context.
    edge_tags: Vec<(CuboidFaceTag, CuboidFaceTag)>,
    /// Chamfer offset distance, in world units.
    radius: f32,
    /// Owner the substrate-resolved IDs were derived against.
    /// Stored so future-arity sanity (e.g. snapshot round-trip
    /// re-validation) can use it.
    owner: BRepOwnerId,
}

/// Canonical (CuboidFaceTag, CuboidFaceTag) pair table parallel to
/// the 12-edge order returned by `<CuboidOp as BRepEdgeProvider>::brep_edge_ids`.
///
/// See `cuboid.rs::impl BRepEdgeProvider for CuboidOp` for the canonical
/// adjacency table — this constant mirrors the same `(face_a_tag, face_b_tag)`
/// pairs in the same order:
///
/// ```text
/// 0..3  : NegZ ∩ {NegY, PosY, NegX, PosX}
/// 4..7  : PosZ ∩ {NegY, PosY, NegX, PosX}
/// 8..11 : {NegY, PosY} × {NegX, PosX}
/// ```
const CUBOID_EDGE_TAG_PAIRS: [(CuboidFaceTag, CuboidFaceTag); 12] = [
    // Bottom-face (NegZ) perimeter — 4 edges
    (CuboidFaceTag::NegZ, CuboidFaceTag::NegY),
    (CuboidFaceTag::NegZ, CuboidFaceTag::PosY),
    (CuboidFaceTag::NegZ, CuboidFaceTag::NegX),
    (CuboidFaceTag::NegZ, CuboidFaceTag::PosX),
    // Top-face (PosZ) perimeter — 4 edges
    (CuboidFaceTag::PosZ, CuboidFaceTag::NegY),
    (CuboidFaceTag::PosZ, CuboidFaceTag::PosY),
    (CuboidFaceTag::PosZ, CuboidFaceTag::NegX),
    (CuboidFaceTag::PosZ, CuboidFaceTag::PosX),
    // Vertical edges (Y-axis face × X-axis face) — 4 edges
    (CuboidFaceTag::NegY, CuboidFaceTag::NegX),
    (CuboidFaceTag::NegY, CuboidFaceTag::PosX),
    (CuboidFaceTag::PosY, CuboidFaceTag::NegX),
    (CuboidFaceTag::PosY, CuboidFaceTag::PosX),
];

impl FilletOp {
    /// Construct a FilletOp validated against the upstream Cuboid.
    ///
    /// # Errors
    ///
    /// * [`FilletError::InvalidRadius`] if `radius` is non-finite or
    ///   `<= 0`.
    /// * [`FilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`FilletError::EdgeNotInUpstream`] if any edge ID does not
    ///   appear in `upstream.brep_edge_ids(owner)`.
    pub fn new(
        upstream: &CuboidOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(FilletError::InvalidRadius { radius });
        }
        if edges.is_empty() {
            return Err(FilletError::EmptyEdgeSelection);
        }

        // Resolve each edge ID back to a CuboidFaceTag pair. The
        // upstream's BRepEdgeProvider returns BRepEdgeIds in the
        // canonical 12-edge adjacency order documented above.
        let upstream_edges = upstream.brep_edge_ids(owner);
        let mut edge_tags = Vec::with_capacity(edges.len());
        for edge_id in &edges {
            let position = upstream_edges
                .iter()
                .position(|id| id == edge_id)
                .ok_or(FilletError::EdgeNotInUpstream { edge: *edge_id })?;
            edge_tags.push(CUBOID_EDGE_TAG_PAIRS[position]);
        }

        Ok(Self {
            edges,
            edge_tags,
            radius,
            owner,
        })
    }

    /// Borrow the validated edge selection.
    #[must_use]
    pub fn edges(&self) -> &[BRepEdgeId] {
        &self.edges
    }

    /// Returns the chamfer radius.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Returns the owner the edge IDs were validated against.
    #[must_use]
    pub fn owner(&self) -> BRepOwnerId {
        self.owner
    }
}

impl Operator for FilletOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Fillet
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"fillet:");
        hasher.update(&self.radius.to_le_bytes());
        hasher.update(self.owner.as_bytes());
        hasher.update(
            &u32::try_from(self.edges.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for edge in &self.edges {
            hasher.update(edge.as_bytes());
        }
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];
        let mut positions = upstream.positions.clone();
        let mut indices = upstream.indices.clone();

        // For each filleted edge, locate its 2 endpoint corners in
        // the upstream Cuboid's vertex array and add 2 chamfer-cap
        // triangles. The Cuboid vertex layout is documented in
        // `cuboid.rs::evaluate`:
        //
        //   0: (-x,-y,-z)  1: (+x,-y,-z)  2: (+x,+y,-z)  3: (-x,+y,-z)
        //   4: (-x,-y,+z)  5: (+x,-y,+z)  6: (+x,+y,+z)  7: (-x,+y,+z)
        for &(tag_a, tag_b) in &self.edge_tags {
            let (corner_a_idx, corner_b_idx) = cuboid_edge_corner_indices(tag_a, tag_b);

            // Defensive bounds check — the upstream Cuboid is
            // expected to have at least 8 corners. If the upstream
            // happens to be a non-Cuboid masquerading as one (e.g.
            // someone wired FilletOp downstream of a different
            // operator), surface a structured error rather than
            // panicking.
            let corner_a_usize = corner_a_idx as usize;
            let corner_b_usize = corner_b_idx as usize;
            if corner_a_usize >= positions.len() || corner_b_usize >= positions.len() {
                return Err(OpError::InvalidParameter(format!(
                    "FilletOp upstream tessellation has only {} vertices; \
                     expected at least 8 (Cuboid layout)",
                    positions.len()
                )));
            }

            let corner_a = positions[corner_a_usize];
            let corner_b = positions[corner_b_usize];

            // Compute inward offset direction = average of the two
            // adjacent face normals, negated (we want INWARD offset).
            let normal_a = cuboid_face_normal(tag_a);
            let normal_b = cuboid_face_normal(tag_b);
            let inward = [
                -(normal_a[0] + normal_b[0]) / 2.0,
                -(normal_a[1] + normal_b[1]) / 2.0,
                -(normal_a[2] + normal_b[2]) / 2.0,
            ];

            // Add 2 new vertices: each endpoint corner offset inward
            // by `radius` along the bisector direction.
            let offset_a = [
                corner_a[0] + inward[0] * self.radius,
                corner_a[1] + inward[1] * self.radius,
                corner_a[2] + inward[2] * self.radius,
            ];
            let offset_b = [
                corner_b[0] + inward[0] * self.radius,
                corner_b[1] + inward[1] * self.radius,
                corner_b[2] + inward[2] * self.radius,
            ];

            let offset_a_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(offset_a);
            let offset_b_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(offset_b);

            // Add 2 chamfer-cap triangles connecting the original
            // edge endpoints with the offset replicas. Winding is
            // chosen so the cap faces outward along the bisector
            // direction. (For sub-α, exact winding-correctness for
            // multi-edge configurations is explicitly out of scope.)
            indices.push(corner_a_idx);
            indices.push(corner_b_idx);
            indices.push(offset_a_idx);

            indices.push(corner_b_idx);
            indices.push(offset_b_idx);
            indices.push(offset_a_idx);
        }

        Tessellation::new(positions, indices)
            .map_err(|e| OpError::InvalidParameter(format!("fillet output invalid: {e}")))
    }

    /// `FilletOp::evaluate` calls [`Tessellation::new`] on the
    /// extended positions, which produces an unlabeled output
    /// regardless of whether the upstream input carried
    /// `face_labels`. Mirrors [`super::TransformOp`]'s
    /// label-stripping override so the cache-key prediction matches
    /// reality.
    fn output_is_labeled(&self, _inputs_labeled: &[bool]) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Helper tables — derived from cuboid.rs::evaluate's 8-corner layout.
// ---------------------------------------------------------------------------

/// Return the 2 vertex indices in the Cuboid's vertex array that form
/// the endpoints of the edge between `tag_a` and `tag_b`.
///
/// Cuboid corner indexing (per `cuboid.rs::evaluate`):
///
/// ```text
/// 0: (-x,-y,-z)  1: (+x,-y,-z)  2: (+x,+y,-z)  3: (-x,+y,-z)
/// 4: (-x,-y,+z)  5: (+x,-y,+z)  6: (+x,+y,+z)  7: (-x,+y,+z)
/// ```
///
/// Each edge spans 2 corners that differ in exactly ONE axis sign
/// (the axis perpendicular to BOTH faces' normals). Argument order
/// does not matter — the helper sorts the tag pair internally so
/// `f(a, b) == f(b, a)`.
fn cuboid_edge_corner_indices(tag_a: CuboidFaceTag, tag_b: CuboidFaceTag) -> (u32, u32) {
    // Sort by discriminant so the match handles each unordered pair
    // exactly once. The discriminant ordering is frozen at
    // NegZ=0 < PosZ=1 < NegY=2 < PosY=3 < NegX=4 < PosX=5 per
    // face_tag.rs (sub-7.2-α).
    let (lo, hi) = if tag_a.discriminant() <= tag_b.discriminant() {
        (tag_a, tag_b)
    } else {
        (tag_b, tag_a)
    };

    use CuboidFaceTag::{NegX, NegY, NegZ, PosX, PosY, PosZ};
    match (lo, hi) {
        // NegZ (bottom of box, -Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the bottom face.
        (NegZ, NegY) => (0, 1), // -Z ∩ -Y → (-,-,-) and (+,-,-)
        (NegZ, PosY) => (3, 2), // -Z ∩ +Y → (-,+,-) and (+,+,-)
        (NegZ, NegX) => (0, 3), // -Z ∩ -X → (-,-,-) and (-,+,-)
        (NegZ, PosX) => (1, 2), // -Z ∩ +X → (+,-,-) and (+,+,-)

        // PosZ (top of box, +Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the top face.
        (PosZ, NegY) => (4, 5), // +Z ∩ -Y → (-,-,+) and (+,-,+)
        (PosZ, PosY) => (7, 6), // +Z ∩ +Y → (-,+,+) and (+,+,+)
        (PosZ, NegX) => (4, 7), // +Z ∩ -X → (-,-,+) and (-,+,+)
        (PosZ, PosX) => (5, 6), // +Z ∩ +X → (+,-,+) and (+,+,+)

        // The 4 vertical edges (Y-axis face × X-axis face).
        (NegY, NegX) => (0, 4), // -Y ∩ -X → (-,-,-) and (-,-,+)
        (NegY, PosX) => (1, 5), // -Y ∩ +X → (+,-,-) and (+,-,+)
        (PosY, NegX) => (3, 7), // +Y ∩ -X → (-,+,-) and (-,+,+)
        (PosY, PosX) => (2, 6), // +Y ∩ +X → (+,+,-) and (+,+,+)

        // Same axis (e.g. NegZ ∩ NegZ or NegZ ∩ PosZ): not a real
        // cuboid edge. The validation in FilletOp::new should have
        // already rejected these via the BRepEdgeProvider lookup —
        // this arm is a defensive fallback that returns the dummy
        // pair (0, 0) so the caller's geometry produces a degenerate
        // (zero-area) chamfer triangle rather than panicking. In
        // practice `cuboid_edge_corner_indices` is only called for
        // (tag_a, tag_b) pairs we already validated come from
        // `CUBOID_EDGE_TAG_PAIRS`, so this arm is unreachable in
        // production paths.
        _ => (0, 0),
    }
}

/// Outward-pointing unit normal for the given Cuboid face tag.
fn cuboid_face_normal(tag: CuboidFaceTag) -> [f32; 3] {
    match tag {
        CuboidFaceTag::NegX => [-1.0, 0.0, 0.0],
        CuboidFaceTag::PosX => [1.0, 0.0, 0.0],
        CuboidFaceTag::NegY => [0.0, -1.0, 0.0],
        CuboidFaceTag::PosY => [0.0, 1.0, 0.0],
        CuboidFaceTag::NegZ => [0.0, 0.0, -1.0],
        CuboidFaceTag::PosZ => [0.0, 0.0, 1.0],
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_cube() -> CuboidOp {
        CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    #[test]
    fn new_rejects_zero_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = FilletOp::new(&cube, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_rejects_negative_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = FilletOp::new(&cube, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_rejects_non_finite_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err_nan = FilletOp::new(&cube, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, FilletError::InvalidRadius { .. }));
        let err_inf = FilletOp::new(&cube, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, FilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_rejects_empty_edge_list() {
        let cube = unit_cube();
        let err = FilletOp::new(&cube, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, FilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_rejects_unknown_edge_id() {
        let cube = unit_cube();
        // Synthesize an edge ID with bytes that don't match any
        // valid Cuboid edge under this owner.
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = FilletOp::new(&cube, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, FilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    #[test]
    fn new_accepts_valid_single_edge() {
        let cube = unit_cube();
        let first_edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![first_edge], 0.1).expect("valid");
        assert_eq!(op.edges(), &[first_edge]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_accepts_all_12_edges() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        let op = FilletOp::new(&cube, owner(), all_edges.clone(), 0.05).expect("12 edges");
        assert_eq!(op.edges().len(), 12);
        assert_eq!(op.edges(), &all_edges[..]);
    }

    #[test]
    fn op_kind_is_fillet() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.op_kind(), OpKind::Fillet);
    }

    #[test]
    fn arity_is_one() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.arity(), 1);
    }

    #[test]
    fn structural_hash_changes_with_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edge], 0.2).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_changes_with_edge_selection() {
        let cube = unit_cube();
        let edges = cube.brep_edge_ids(owner());
        let a = FilletOp::new(&cube, owner(), vec![edges[0]], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edges[0], edges[1]], 0.1).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_includes_owner() {
        let owner_a = BRepOwnerId::from_bytes([0x11; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22; 16]);
        let cube = unit_cube();
        // Use the FIRST edge from each owner — same canonical
        // position (NegZ ∩ NegY), but different owner means different
        // BRepEdgeId bytes (face IDs include owner in their derivation).
        let edge_a = cube.brep_edge_ids(owner_a)[0];
        let edge_b = cube.brep_edge_ids(owner_b)[0];
        let a = FilletOp::new(&cube, owner_a, vec![edge_a], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner_b, vec![edge_b], 0.1).expect("b");
        assert_ne!(
            a.structural_hash(),
            b.structural_hash(),
            "different owners should produce different structural hashes"
        );
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn evaluate_rejects_wrong_arity_zero_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn evaluate_rejects_wrong_arity_two_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let err = op.evaluate(&[&upstream, &upstream]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 2
            }
        ));
    }

    #[test]
    fn evaluate_one_edge_adds_2_vertices_and_2_triangles() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // Cuboid: 8 verts + 36 indices (12 triangles).
        // After 1 fillet: +2 verts + 6 indices (2 triangles).
        assert_eq!(out.vertex_count(), 10);
        assert_eq!(out.indices.len(), 42);
        assert_eq!(out.triangle_count(), 14);
    }

    #[test]
    fn evaluate_three_edges_adds_6_vertices_and_6_triangles() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        // Use 3 non-adjacent edges (the 3 edges of corner 0 would
        // share corner 0; the spec says the substrate-validation
        // tests don't exercise that case — pick edges that don't all
        // meet at a single corner).
        let op = FilletOp::new(
            &cube,
            owner(),
            vec![all_edges[0], all_edges[5], all_edges[11]],
            0.1,
        )
        .expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // 8 + 6 = 14 verts; 36 + 18 = 54 indices.
        assert_eq!(out.vertex_count(), 14);
        assert_eq!(out.indices.len(), 54);
        assert_eq!(out.triangle_count(), 18);
    }

    #[test]
    fn cuboid_edge_corner_indices_arg_order_independent() {
        // f(a, b) == f(b, a) for every valid pair.
        for &(a, b) in &CUBOID_EDGE_TAG_PAIRS {
            let (i_ab, j_ab) = cuboid_edge_corner_indices(a, b);
            let (i_ba, j_ba) = cuboid_edge_corner_indices(b, a);
            assert_eq!(
                (i_ab, j_ab),
                (i_ba, j_ba),
                "corner-indices helper must be order-independent for ({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn cuboid_edge_corner_indices_all_pairs_in_bounds() {
        // All 12 canonical pairs return corner indices in [0, 8).
        for &(a, b) in &CUBOID_EDGE_TAG_PAIRS {
            let (i, j) = cuboid_edge_corner_indices(a, b);
            assert!(
                i < 8 && j < 8,
                "corner indices ({i}, {j}) for ({a:?}, {b:?}) out of cuboid 8-corner bounds"
            );
            assert_ne!(i, j, "edge endpoints must differ for ({a:?}, {b:?})");
        }
    }

    /// `FilletOp::evaluate` strips labels (calls `Tessellation::new`
    /// which always produces an unlabeled mesh) — so
    /// `output_is_labeled` must return `false` regardless of input
    /// label state. Mirrors `TransformOp::transform_output_is_labeled_strips`.
    #[test]
    fn output_is_labeled_strips() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert!(!op.output_is_labeled(&[false]));
        assert!(!op.output_is_labeled(&[true]));
    }
}
