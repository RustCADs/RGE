//! `cad_projection::render_adapter` — game-domain → renderer-tier
//! adapter for the Render-backed face-selection chapter.
//!
//! Failure class: snapshot-recoverable (callable adapter; pure query, no
//! mutation, no tick coupling).
//!
//! Single call: take an entity's `ProjectedMesh` (cad-core
//! `Tessellation`-derived data with `TopologyFaceId` face labels) and
//! produce a [`rge_brep_render::RenderMesh`] (renderer-domain
//! flat-shaded mesh with opaque `u64` face labels). The `u64` is the
//! wire-compatible shape of `TopologyFaceId.0` — re-resolvable later
//! via [`CadProjection::brep_face_id_for_triangle`] for highlight
//! feedback.
//!
//! No caching: each call re-runs `RenderMesh::from_buffers`. Cost is
//! bounded by triangle count; future caching is a separate dispatch
//! if profiling shows it's needed.
//!
//! # Layering — game-domain → renderer-tier (LOAD-BEARING)
//!
//! `cad-projection` is a Tier-2 game-domain crate (per the `rge-cad-`
//! prefix); `brep-render` is in the `RENDERER_CRATES` set under PLAN.md
//! §1.3 rule 6. Rule 6 forbids **renderer-tier → game-domain**;
//! `cad-projection` → `brep-render` is the **opposite** direction
//! (game-domain → renderer-tier) and so does NOT trip rule 6. This
//! adapter therefore lives in `cad-projection` (the natural home for
//! cad-core ↔ renderer-tier bridging — `cad-projection` is already
//! the only Tier-2 crate allowed to import `cad-core` per PLAN
//! §1.5.4.5), and `brep-render` retains its game-domain-clean
//! Cargo.toml.
//!
//! # Two-gate semantics (LOAD-BEARING)
//!
//! [`CadProjection::render_mesh_for`] gates on **two** preconditions:
//!
//! 1. The entity carries a [`BRepHandle`] component — CAD-entity scope
//!    gate. This adapter is for CAD-domain entities, not arbitrary
//!    ECS entities.
//! 2. The entity has a projected mesh in the cache — geometry-validity
//!    gate. There must be projected geometry to convert.
//!
//! It does **NOT** gate on `BRepHandle.brep_owner`. The `brep_owner`
//! field is a *selection-identity* concern (used by the picker to
//! scope `BRepFaceId` resolution under the right owner-seed) and is
//! orthogonal to whether geometry should render. An entity with
//! `BRepHandle` + projected mesh but `brep_owner == None` will still
//! produce a `Some(RenderMesh)` — the geometry renders; only its
//! faces aren't pickable.

use rge_brep_render::RenderMesh;
use rge_kernel_ecs::{EntityId, World};

use crate::projection_structural::BRepHandle;
use crate::CadProjection;

impl CadProjection {
    /// Build a renderer-ready [`RenderMesh`] for `entity`'s current
    /// projected mesh, if any.
    ///
    /// # Gates
    ///
    /// Returns `None` when:
    /// * `entity` has no [`BRepHandle`] component (CAD-entity scope
    ///   gate — this method is for CAD-domain entities), OR
    /// * the entity has no projected mesh in the cache (geometry-
    ///   validity gate — there must be projected geometry to convert).
    ///
    /// **Does NOT check `brep_owner`.** The two existing gates above
    /// scope the method to CAD entities with valid geometry; the
    /// `brep_owner` field is a *selection-identity* concern (used by
    /// the picker to scope `BRepFaceId` resolution under the right
    /// owner-seed) and is orthogonal to whether geometry should
    /// render. An entity with `BRepHandle` + projected mesh but
    /// `brep_owner == None` will still produce a `Some(RenderMesh)` —
    /// the geometry renders; only its faces aren't pickable.
    ///
    /// # `face_labels` shape
    ///
    /// `RenderMesh.face_labels` is `Some` iff the underlying
    /// `ProjectedMesh.face_labels` is `Some` — i.e., labeled iff the
    /// source operator emits face labels (Cuboid / Extrude / Revolve
    /// / Loft via D-projection-α/β/γδ; unlabeled for FilletOp /
    /// BooleanOp / SweepOp output, matching the parked
    /// `FILLET_OUTPUT_IDENTITY.md` posture).
    ///
    /// Each `u64` in `face_labels` is the wire-compatible
    /// `TopologyFaceId.0` value; resolution back to `BRepFaceId`
    /// goes through [`Self::brep_face_id_for_triangle`].
    #[must_use]
    pub fn render_mesh_for(&self, entity: EntityId, world: &World) -> Option<RenderMesh> {
        let entity_ref = world.entity(entity)?;
        // Gate 1: BRepHandle presence — CAD-entity scope.
        let _handle = entity_ref.get::<BRepHandle>()?;
        // Gate 2: ProjectedMesh cache presence — geometry validity.
        let mesh = self.projected_mesh(entity)?;
        // Adapter: TopologyFaceId.0 → opaque u64 for renderer-tier consumption.
        let face_labels: Option<Vec<u64>> = mesh
            .face_labels
            .as_ref()
            .map(|v| v.iter().map(|t| t.0).collect());
        Some(RenderMesh::from_buffers(
            &mesh.positions,
            &mesh.indices,
            face_labels.as_deref(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_cad_core::{
        BRepEdgeProvider, BRepOwnerId, CadGraph, CuboidOp, FilletOp, OperatorNode, Tolerance,
        TopologyFaceId,
    };
    use rge_kernel_ecs::World;

    use crate::projection_structural::BRepHandle;
    use crate::CadProjection;

    const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

    fn tol() -> Tolerance {
        Tolerance::new(0.001).expect("tol")
    }

    /// Build `(graph, projection, world, entity)` with a single Cuboid
    /// committed and projected. `BRepHandle.brep_owner` is set to `owner`
    /// post-spawn unless `owner` is `None`.
    fn build_cuboid(
        width: f32,
        height: f32,
        depth: f32,
        owner: Option<BRepOwnerId>,
    ) -> (CadGraph, CadProjection, World, rge_kernel_ecs::EntityId) {
        let mut graph = CadGraph::new();
        graph.begin_operation().expect("begin");
        let cuboid_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width,
                height,
                depth,
            }))
            .expect("add cuboid");
        graph
            .graph_mut()
            .expect("mut2")
            .set_root(cuboid_node)
            .expect("set root");
        graph.commit("cuboid").expect("commit");

        let mut projection = CadProjection::new();
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let entity = projection
            .spawn_brep_entity(&mut world, cuboid_node)
            .expect("spawn");
        if let Some(o) = owner {
            if let Some(mut em) = world.entity_mut(entity) {
                if let Some(mut handle) = em.get_mut::<BRepHandle>() {
                    handle.brep_owner = Some(o);
                }
            }
        }
        projection.tick(&mut world, &graph, tol()).expect("tick");

        (graph, projection, world, entity)
    }

    /// Test 1 — fresh entity (no BRepHandle component) → `None`. Verifies
    /// gate 1 (CAD-entity scope).
    #[test]
    fn render_mesh_for_returns_none_when_entity_has_no_brep_handle() {
        let mut world = World::new();
        // NOTE: do NOT register BRepHandle / spawn via projection — we want
        // an entity with no BRepHandle component at all.
        let entity = world.spawn();
        let projection = CadProjection::new();

        let result = projection.render_mesh_for(entity, &world);
        assert!(
            result.is_none(),
            "entity without BRepHandle must yield None; got {result:?}"
        );
    }

    /// Test 2 — entity has `BRepHandle` but no projection cache entry (no
    /// `tick`) → `None`. Verifies gate 2 (geometry validity).
    #[test]
    fn render_mesh_for_returns_none_when_entity_has_no_projected_mesh() {
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let mut graph = CadGraph::new();
        graph.begin_operation().expect("begin");
        let cuboid_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }))
            .expect("add cuboid");
        graph
            .graph_mut()
            .expect("mut2")
            .set_root(cuboid_node)
            .expect("set root");
        graph.commit("cuboid").expect("commit");

        let entity = projection
            .spawn_brep_entity(&mut world, cuboid_node)
            .expect("spawn");

        // No tick — the entity has a BRepHandle but no projected mesh in
        // the cache yet.
        assert!(
            projection.projected_mesh(entity).is_none(),
            "precondition: no projected mesh before tick",
        );

        let result = projection.render_mesh_for(entity, &world);
        assert!(
            result.is_none(),
            "entity with BRepHandle but no projected mesh must yield None; got {result:?}"
        );
    }

    /// Test 3 — LOAD-BEARING for "doesn't gate on owner" contract.
    ///
    /// Build a Cuboid entity, set `brep_owner = None` post-spawn, run
    /// `tick`, then call `render_mesh_for`. MUST return `Some(RenderMesh)` —
    /// the geometry-vs-selection orthogonality means an entity without an
    /// owner (which is unpickable) still renders.
    #[test]
    fn render_mesh_for_yields_some_when_no_brep_owner_set() {
        let (_graph, projection, world, entity) = build_cuboid(1.0, 1.0, 1.0, None);
        // Sanity: confirm the brep_owner is indeed None.
        let er = world.entity(entity).expect("entity");
        let handle = er.get::<BRepHandle>().expect("handle");
        assert_eq!(
            handle.brep_owner, None,
            "precondition: brep_owner must be None for this test",
        );

        let mesh = projection
            .render_mesh_for(entity, &world)
            .expect("render_mesh_for must return Some(...) even with brep_owner = None");
        // Cuboid → 12 triangles → 36 positions / 36 normals / 36 indices.
        assert_eq!(mesh.positions.len(), 36);
        assert_eq!(mesh.normals.len(), 36);
        assert_eq!(mesh.indices.len(), 36);
        // Cuboid emits face_labels (D-projection-α) → labels propagate
        // through unchanged.
        assert!(
            mesh.face_labels.is_some(),
            "Cuboid is labeled per D-projection-α; face_labels must propagate through render adapter"
        );
        assert_eq!(mesh.face_labels.as_ref().unwrap().len(), 12);
    }

    /// Test 4 — Cuboid entity with `brep_owner = Some(...)` →
    /// `RenderMesh.face_labels.is_some()`, count 12. Verifies labeled
    /// passthrough.
    #[test]
    fn render_mesh_face_labels_some_for_labeled_cuboid_projection() {
        let (_graph, projection, world, entity) = build_cuboid(1.0, 1.0, 1.0, Some(ENTITY_OWNER));

        let mesh = projection
            .render_mesh_for(entity, &world)
            .expect("must render for valid Cuboid entity");
        let labels = mesh
            .face_labels
            .as_ref()
            .expect("Cuboid output is labeled per D-projection-α");
        assert_eq!(labels.len(), 12, "Cuboid has 12 triangles, all labeled");
        // Each label is a `TopologyFaceId.0`; canonical Cuboid face order is
        // 0..6 (NegZ→PosZ→NegY→PosY→NegX→PosX), 2 triangles per face.
        for label in labels {
            assert!(
                *label < 6,
                "Cuboid face labels must be in 0..6; got {label}"
            );
        }
    }

    /// Test 5 — Cuboid -> Fillet entity -> `RenderMesh.face_labels.is_some()`.
    /// Inherited Cuboid labels propagate and chamfer caps are DEGENERATE.
    #[test]
    fn render_mesh_face_labels_some_for_labeled_filleted_output() {
        let mut graph = CadGraph::new();
        graph.begin_operation().expect("begin");
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let cuboid_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid.clone()))
            .expect("cuboid");
        let edge_id = cuboid.brep_edge_ids(ENTITY_OWNER)[0];
        let fillet = FilletOp::new(&cuboid, ENTITY_OWNER, vec![edge_id], 0.1).expect("fillet");
        let fillet_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("fillet node");
        graph
            .graph_mut()
            .expect("mut")
            .connect(cuboid_node, fillet_node, 0)
            .expect("connect");
        graph
            .graph_mut()
            .expect("mut")
            .set_root(fillet_node)
            .expect("set root");
        graph.commit("cuboid -> fillet").expect("commit");

        let mut projection = CadProjection::new();
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let entity = projection
            .spawn_brep_entity(&mut world, fillet_node)
            .expect("spawn");
        if let Some(mut em) = world.entity_mut(entity) {
            if let Some(mut handle) = em.get_mut::<BRepHandle>() {
                handle.brep_owner = Some(ENTITY_OWNER);
            }
        }
        projection.tick(&mut world, &graph, tol()).expect("tick");

        let mesh = projection
            .render_mesh_for(entity, &world)
            .expect("Fillet output still produces a RenderMesh — geometry is valid, only identity is opaque");
        let labels = mesh
            .face_labels
            .as_ref()
            .expect("FilletOp should preserve labels from labeled Cuboid input");
        assert_eq!(
            labels.len(),
            14,
            "FilletOp emits 12 inherited Cuboid triangles + 2 chamfer caps"
        );
        assert!(
            labels
                .iter()
                .any(|label| *label == TopologyFaceId::DEGENERATE.0),
            "chamfer-cap triangles should be marked DEGENERATE"
        );
        // Sanity: positions / indices / normals still populate.
        assert!(!mesh.positions.is_empty());
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.positions.len() % 3, 0);
        assert_eq!(mesh.indices.len(), mesh.positions.len());
    }
}
