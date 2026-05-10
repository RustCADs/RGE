//! `cad_projection::picking` — CPU-only ray-vs-`ProjectedMesh` face picker.
//!
//! Failure class: snapshot-recoverable (callable primitive; pure query, no
//! mutation, no tick coupling).
//!
//! # Selection rule (LOAD-BEARING)
//!
//! [`CadProjection::pick_face`] returns the **closest *resolvable* face hit**,
//! not the closest geometric triangle hit. The distinction matters: a front
//! Fillet / Boolean / Sweep surface emits unlabeled tessellation (or its
//! source operator is classified [`TopologyChangingOperator`] from the
//! resolver's perspective), so its triangles do **not** produce a
//! [`BRepFaceId`]. Such triangles are **transparent** to the picker — they
//! do NOT mask resolvable faces behind them.
//!
//! Algorithm (per-call, no caching, no acceleration structure):
//!
//! 1. Iterate every entity carrying a [`BRepHandle`] whose `brep_owner` is
//!    `Some`.
//! 2. Möller–Trumbore the ray against every triangle of every candidate's
//!    [`ProjectedMesh`]. Collect all positive-`t` hits as
//!    `(t, entity, triangle_index)`.
//! 3. Sort hits ascending by `t`.
//! 4. For each hit in order, call
//!    [`CadProjection::brep_face_id_for_triangle`]. The first one that
//!    returns `Some(face_id)` wins; build the [`FacePick`] from that hit.
//! 5. If no hit resolves a face (all hits are unlabeled / topology-changing /
//!    no-owner / out-of-bounds), return `None`.
//!
//! The picker does **not** depend on `editor-state`. The returned
//! [`FacePick`] carries `entity` + `owner` + `face_id`; downstream callers
//! (the future `EditorShell::window_event` mouse handler) can compose
//! `FaceSelection { entity, owner, face_id }` themselves.
//!
//! # Substrate posture
//!
//! Picking is the first selection-side substrate consumer of the
//! D-projection-α/β/γ/δ face-ID propagation. For a Cuboid the picker resolves
//! every front-facing triangle; for a Cuboid → Fillet output the picker
//! returns `None` (every triangle's `brep_face_id_for_triangle` is `None`,
//! and the rule says return `None` rather than fabricate identity). The
//! parked [`FILLET_OUTPUT_IDENTITY.md`] design note's gap is now visible
//! through the picker too — `face_picking_smoke.rs::pick_returns_none_for_filleted_only_geometry`
//! references it explicitly. **The design note STAYS PARKED** — the picker
//! demonstrates the gap, it does not close it.
//!
//! [`FILLET_OUTPUT_IDENTITY.md`]: ../../../../docs/architecture/FILLET_OUTPUT_IDENTITY.md
//! [`TopologyChangingOperator`]: rge_cad_core::BRepResolveError::TopologyChangingOperator

use rge_cad_core::{BRepFaceId, BRepOwnerId, OperatorGraph};
use rge_kernel_ecs::{EntityId, World};

use crate::projection_structural::BRepHandle;
use crate::CadProjection;

/// Numerical tolerance for the Möller–Trumbore determinant + the rejection
/// of ray-on-origin (`t <= EPSILON`) hits. Empirically tight enough for the
/// picker's use cases; chosen to match the same scale as the `cad-core`
/// `Tolerance::new(0.001)` baseline used elsewhere in cad-projection tests.
const EPSILON: f32 = 1e-6;

/// World-space ray.
///
/// `direction` need NOT be unit-length; the reported [`FacePick::t`] is in
/// units of `direction.length()`. Callers who want world-distance hits
/// should normalise `direction` before constructing the ray.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// World-space origin point.
    pub origin: [f32; 3],
    /// World-space direction. Need not be unit-length.
    pub direction: [f32; 3],
}

/// Closest resolvable face hit.
///
/// Built by [`CadProjection::pick_face`] from the first hit (in ray-`t`
/// order) whose triangle resolves a [`BRepFaceId`] under
/// [`CadProjection::brep_face_id_for_triangle`]. Callers compose
/// `FaceSelection { entity, owner, face_id }` from these three fields if
/// they want to feed the editor selection pipeline; the picker itself does
/// NOT depend on `editor-state`.
#[derive(Debug, Clone, Copy)]
pub struct FacePick {
    /// The picked entity (the one carrying the [`BRepHandle`]).
    pub entity: EntityId,
    /// The entity's [`BRepHandle::brep_owner`] at pick time. Guaranteed
    /// `Some(...)` — entities with `brep_owner == None` are filtered out
    /// before the picker considers their triangles.
    pub owner: BRepOwnerId,
    /// The stable B-Rep face identity of the picked face.
    pub face_id: BRepFaceId,
    /// Ray parameter at the hit. Guaranteed strictly positive (hits at
    /// `t <= EPSILON` are rejected).
    pub t: f32,
    /// Triangle index in `ProjectedMesh.indices` (the i-th triangle uses
    /// indices `3*i..3*(i+1)`).
    pub triangle_index: usize,
}

// ---------------------------------------------------------------------------
// Möller–Trumbore — raw `[f32; 3]` math (no glam dep).
// ---------------------------------------------------------------------------

/// Subtract two `[f32; 3]` vectors component-wise.
#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Cross-product of two `[f32; 3]` vectors.
#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Dot-product of two `[f32; 3]` vectors.
#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Möller–Trumbore ray/triangle intersection.
///
/// Returns `Some(t)` when the ray hits the triangle at parameter `t > EPSILON`
/// (i.e. strictly in front of the ray origin), `None` otherwise. Two-sided
/// (NO back-face culling): the picker should be able to select either side
/// of a face.
///
/// Standard implementation; see e.g. <https://en.wikipedia.org/wiki/M%C3%B6ller%E2%80%93Trumbore_intersection_algorithm>.
fn ray_triangle_intersect(ray: &Ray, v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> Option<f32> {
    let edge1 = sub(v1, v0);
    let edge2 = sub(v2, v0);
    let h = cross(ray.direction, edge2);
    let a = dot(edge1, h);
    // Parallel-to-triangle (or near-parallel) — |a| close to 0.
    if a.abs() < EPSILON {
        return None;
    }
    let f = 1.0 / a;
    let s = sub(ray.origin, v0);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = f * dot(ray.direction, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * dot(edge2, q);
    if t > EPSILON {
        Some(t)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// CadProjection::pick_face
// ---------------------------------------------------------------------------

impl CadProjection {
    /// Find the closest *resolvable* face hit for `ray` across all entities
    /// in `world`.
    ///
    /// See [the module-level docs][crate::picking] for the selection rule.
    /// In short: closest **resolvable** hit (whose triangle yields a stable
    /// [`BRepFaceId`] via [`Self::brep_face_id_for_triangle`]), not closest
    /// geometric hit. Unlabeled / topology-changing / no-owner triangles
    /// are transparent — they do NOT mask resolvable faces behind them.
    ///
    /// Returns `None` when:
    ///
    /// * No entity in `world` carries a [`BRepHandle`] with
    ///   `brep_owner == Some(_)`, OR
    /// * No candidate triangle is hit by the ray, OR
    /// * Every triangle that IS hit fails to resolve a [`BRepFaceId`] (e.g.
    ///   filleted / boolean / sweep output everywhere along the ray).
    ///
    /// Cost: `O(total_triangles)` for the Möller–Trumbore pass plus
    /// `O(hits * triangles_per_resolved_entity)` for the resolve loop in
    /// the worst case. No acceleration structure; the picker is intentionally
    /// straightforward — Phase 7 / future-dispatch territory.
    ///
    /// # Substrate posture
    ///
    /// `pick_face` is a callable primitive. It is NOT wired into any UI
    /// surface yet; the future `EditorShell::window_event` mouse handler
    /// composes `FaceSelection { entity: pick.entity, owner: pick.owner,
    /// face_id: pick.face_id }` from the returned [`FacePick`] when the
    /// rendering wave lands.
    #[must_use]
    pub fn pick_face(&self, ray: &Ray, world: &World, graph: &OperatorGraph) -> Option<FacePick> {
        // Phase 1 — collect ALL hits across every candidate entity.
        // (entity, triangle_index, t) tuples.
        let mut hits: Vec<(EntityId, usize, f32)> = Vec::new();
        for (entity, handle) in world.query::<BRepHandle>() {
            // Filter: entities without a brep_owner have no resolvable
            // identity space; they are transparent to the picker. (The
            // resolver would also short-circuit to None on owner == None,
            // but skipping here saves the geometry work.)
            if handle.brep_owner.is_none() {
                continue;
            }
            let Some(mesh) = self.projected_mesh(entity) else {
                continue;
            };
            let positions = &mesh.positions;
            let indices = &mesh.indices;
            let triangle_count = indices.len() / 3;
            for tri_idx in 0..triangle_count {
                let i0 = indices[tri_idx * 3] as usize;
                let i1 = indices[tri_idx * 3 + 1] as usize;
                let i2 = indices[tri_idx * 3 + 2] as usize;
                let Some(v0) = positions.get(i0).copied() else {
                    continue;
                };
                let Some(v1) = positions.get(i1).copied() else {
                    continue;
                };
                let Some(v2) = positions.get(i2).copied() else {
                    continue;
                };
                if let Some(t) = ray_triangle_intersect(ray, v0, v1, v2) {
                    hits.push((entity, tri_idx, t));
                }
            }
        }

        // Phase 2 — sort hits ascending by t.
        // f32 has no Ord; partial_cmp + unwrap_or(Equal) is fine since the
        // intersection routine rejects NaN/inf cases (parallel-to-triangle
        // and t<=EPSILON branches both return None).
        hits.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // Phase 3 — iterate in-order, returning the first hit that resolves.
        for (entity, tri_idx, t) in hits {
            let Some(face_id) = self.brep_face_id_for_triangle(entity, tri_idx, world, graph)
            else {
                continue;
            };
            // The owner is guaranteed Some at this point: we filtered on
            // brep_owner.is_some() upstream, AND brep_face_id_for_triangle
            // returned Some (which itself short-circuits on owner == None).
            // Re-read it explicitly so the FacePick carries it through.
            let owner = world
                .entity(entity)
                .and_then(|e| e.get::<BRepHandle>().and_then(|h| h.brep_owner))?;
            return Some(FacePick {
                entity,
                owner,
                face_id,
                t,
                triangle_index: tri_idx,
            });
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Möller–Trumbore unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical XY-plane triangle: v0 at origin, v1 along +X, v2 along +Y.
    /// Spans `(0,0,0)`, `(1,0,0)`, `(0,1,0)`. Outward normal is `+Z`.
    fn xy_triangle() -> ([f32; 3], [f32; 3], [f32; 3]) {
        ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    }

    /// Ray pointing at the triangle's geometric centroid `(1/3, 1/3, 0)` from
    /// `(1/3, 1/3, +5)` along `-Z`. Should hit at `t == 5.0` on the centroid
    /// — well inside `(u, v)` bounds.
    #[test]
    fn ray_hits_triangle_dead_center() {
        let (v0, v1, v2) = xy_triangle();
        let ray = Ray {
            origin: [1.0 / 3.0, 1.0 / 3.0, 5.0],
            direction: [0.0, 0.0, -1.0],
        };
        let t = ray_triangle_intersect(&ray, v0, v1, v2).expect("must hit");
        assert!((t - 5.0).abs() < 1e-5, "expected t≈5.0, got {t}");
    }

    /// Ray pointing straight down at `(2, 2, +5)` — far outside the unit
    /// triangle. Must NOT hit.
    #[test]
    fn ray_misses_triangle_outside() {
        let (v0, v1, v2) = xy_triangle();
        let ray = Ray {
            origin: [2.0, 2.0, 5.0],
            direction: [0.0, 0.0, -1.0],
        };
        let result = ray_triangle_intersect(&ray, v0, v1, v2);
        assert!(result.is_none(), "ray well outside triangle must miss");
    }

    /// Ray parallel to the triangle (direction in the XY plane, +X) does
    /// NOT hit — the determinant goes to zero and the routine bails before
    /// dividing by it.
    #[test]
    fn ray_parallel_to_triangle_misses() {
        let (v0, v1, v2) = xy_triangle();
        let ray = Ray {
            origin: [-1.0, 0.25, 0.0],
            direction: [1.0, 0.0, 0.0],
        };
        let result = ray_triangle_intersect(&ray, v0, v1, v2);
        assert!(
            result.is_none(),
            "ray parallel to triangle plane must not produce a hit"
        );
    }

    /// Ray origin sits on the triangle's near side — the only intersection
    /// is at `t < 0` (behind the ray origin). Must NOT hit (negative `t`
    /// rejected).
    #[test]
    fn ray_behind_origin_misses() {
        let (v0, v1, v2) = xy_triangle();
        let ray = Ray {
            origin: [0.25, 0.25, -5.0],
            direction: [0.0, 0.0, -1.0],
        };
        // The triangle is at z=0, ray origin is at z=-5 pointing -Z; the
        // triangle is BEHIND the origin along the ray direction. t would
        // be negative; we reject.
        let result = ray_triangle_intersect(&ray, v0, v1, v2);
        assert!(
            result.is_none(),
            "hits at negative t (behind ray origin) must be rejected"
        );
    }

    /// Ray pointing at a point right at the v0-v1 edge of the triangle.
    /// The Möller–Trumbore u/v bounds use `0..=1` and `u+v <= 1`, so an
    /// edge sample is inclusive — should hit.
    #[test]
    fn ray_glancing_edge_hits() {
        let (v0, v1, v2) = xy_triangle();
        // Aim slightly inside the v0-v1 edge (the X axis from x=0 to x=1, y=0).
        let ray = Ray {
            origin: [0.5, 0.0, 5.0],
            direction: [0.0, 0.0, -1.0],
        };
        let t = ray_triangle_intersect(&ray, v0, v1, v2).expect("edge sample must hit");
        assert!((t - 5.0).abs() < 1e-5);
    }

    /// Ray pointing at the centroid from BELOW the triangle (z=-5 along +Z).
    /// The triangle is wound CCW from +Z (outward normal +Z), so this is a
    /// back-face hit. The picker rule says NO back-face culling — must hit.
    #[test]
    fn ray_back_face_hits() {
        let (v0, v1, v2) = xy_triangle();
        let ray = Ray {
            origin: [1.0 / 3.0, 1.0 / 3.0, -5.0],
            direction: [0.0, 0.0, 1.0],
        };
        let t = ray_triangle_intersect(&ray, v0, v1, v2)
            .expect("back-face hit must succeed (no culling)");
        assert!((t - 5.0).abs() < 1e-5);
    }
}
