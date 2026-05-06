//! Audit-2 A2.10 + #4 closure: exercise every `ProjectionError` variant.
//!
//! Pre-dispatch coverage was 1 of 5 variants exercised — only
//! `NodeNotInGraph` (via `validate_handles_detects_orphan_after_partial_restore`
//! in `cad_projection_smoke.rs`). The remaining four (`Eval`, `Tolerance`,
//! `NoBRepHandle`, `EntityCadMap`) had no test coverage. This file closes that
//! gap.
//!
//! Tests:
//!
//! 1. **`projection_error_eval_variant_constructed_via_eval_failure`** —
//!    drives `OperatorGraph::evaluate` into `EvalError::PortMismatch` (a
//!    Transform with no upstream input is arity-violation), then calls
//!    `project()`, and asserts the resulting `ProjectionError::Eval` wraps
//!    that error. Verifies the `From<EvalError>` impl.
//!
//! 2. **`projection_error_tolerance_variant_constructed_via_invalid_tolerance`**
//!    — `Tolerance::new(-1.0)` and `Tolerance::new(NaN)` directly return
//!    `ToleranceError`. The `ProjectionError::Tolerance` variant exists
//!    primarily as a `From<ToleranceError>` conversion sink for callers
//!    that propagate. Test drives the conversion via `.into()`.
//!
//! 3. **`projection_error_no_brep_handle_variant_constructed_when_entity_lacks_brephandle`**
//!    — the variant is declared but no source-side path currently constructs
//!    it (the projection layer doesn't enforce a `BRepHandle` requirement;
//!    `tick`/`project` work on `EntityCadMap` lookups). Test asserts the
//!    variant is constructible (the Display impl works, the `entity` field
//!    is reachable). Documents this as a forward-compat error sink.
//!
//! 4. **`projection_error_entity_cad_map_variant_constructed_via_duplicate_node`**
//!    — drives the projection-layer's `spawn_brep_entity` into an
//!    `EntityCadMapError::DuplicateNode`, which surfaces through the
//!    `From<EntityCadMapError>` conversion as `ProjectionError::EntityCadMap`.
//!
//! 5. **`cad_projection_remap_entity_same_node_is_idempotent_no_op`** — #4
//!    finding (`lib.rs:218-223`). Calling `remap_entity(entity, current_node)`
//!    where the entity is already mapped to that node is a no-op except for
//!    marking the entity dirty so the next tick re-projects.

use rge_cad_core::{
    CadGraph, CuboidOp, OperatorNode, TessellationCache, Tolerance, ToleranceError, TransformOp,
};
use rge_cad_projection::{project, BRepHandle, CadProjection, EntityCadMapError, ProjectionError};
use rge_kernel_ecs::{EntityId, World};
use rge_kernel_graph_foundation::NodeId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Build a `CadGraph` whose root is a `TransformOp` with **no upstream input** —
/// `Transform` is arity 1, so evaluating it fails with `EvalError::PortMismatch`.
fn build_dangling_transform_graph() -> (CadGraph, NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let tx = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp::default()))
        .expect("add transform");
    cad.graph_mut().expect("mut2").set_root(tx).expect("root");
    cad.commit("dangling transform").expect("commit");
    (cad, tx)
}

fn build_cuboid_graph() -> (CadGraph, NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cu = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("add cuboid");
    cad.graph_mut().expect("mut2").set_root(cu).expect("root");
    cad.commit("cuboid").expect("commit");
    (cad, cu)
}

// ---------------------------------------------------------------------------
// 1. ProjectionError::Eval — driven by `EvalError::PortMismatch`
// ---------------------------------------------------------------------------

/// `project(cad, node, ...)` wraps any `EvalError` from
/// `OperatorGraph::evaluate` as `ProjectionError::Eval`. We force the
/// underlying eval to fail with `PortMismatch` (Transform with no upstream
/// input — arity 1, got 0) and assert the wrapping conversion.
#[test]
fn projection_error_eval_variant_constructed_via_eval_failure() {
    let (cad, tx_node) = build_dangling_transform_graph();
    let mut cache = TessellationCache::new();

    let err = project(&cad, tx_node, &mut cache, tol())
        .expect_err("dangling Transform must fail to evaluate");

    // The wrapped variant must be Eval, and the inner EvalError must be
    // PortMismatch (Transform arity 1, got 0).
    match err {
        ProjectionError::Eval(eval_err) => {
            // Inner error must be PortMismatch with the right shape.
            assert!(
                matches!(
                    &eval_err,
                    rge_cad_core::EvalError::PortMismatch {
                        expected_arity: 1,
                        got: 0,
                        ..
                    }
                ),
                "expected EvalError::PortMismatch (1 vs 0); got {eval_err:?}"
            );
        }
        other => panic!("expected ProjectionError::Eval; got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 2. ProjectionError::Tolerance — driven by `From<ToleranceError>`
// ---------------------------------------------------------------------------

/// `Tolerance::new(-1.0)` and `Tolerance::new(f32::NAN)` return
/// `ToleranceError::Invalid`. The projection layer exposes
/// `ProjectionError::Tolerance(#[from] ToleranceError)` as a forward-compat
/// conversion sink. Verifying:
///
/// 1. `Tolerance::new` rejects negative + NaN tolerances.
/// 2. The conversion `ToleranceError -> ProjectionError::Tolerance` works.
#[test]
fn projection_error_tolerance_variant_constructed_via_invalid_tolerance() {
    // Negative.
    let neg_err = Tolerance::new(-1.0).expect_err("negative tolerance must reject");
    assert!(
        matches!(neg_err, ToleranceError::Invalid { value } if (value - (-1.0)).abs() < f32::EPSILON),
        "expected Invalid {{ value: -1.0 }}; got {neg_err:?}"
    );
    // The From conversion lifts a ToleranceError into ProjectionError::Tolerance.
    let proj_err: ProjectionError = neg_err.into();
    assert!(
        matches!(
            &proj_err,
            ProjectionError::Tolerance(ToleranceError::Invalid { .. })
        ),
        "expected ProjectionError::Tolerance(Invalid); got {proj_err:?}"
    );
    // The Display impl mentions the tolerance.
    let displayed = format!("{proj_err}");
    assert!(
        displayed.contains("invalid tolerance"),
        "Display impl should mention 'invalid tolerance'; got: {displayed}"
    );

    // NaN.
    let nan_err = Tolerance::new(f32::NAN).expect_err("NaN tolerance must reject");
    let proj_err2: ProjectionError = nan_err.into();
    assert!(matches!(
        proj_err2,
        ProjectionError::Tolerance(ToleranceError::Invalid { .. })
    ));

    // Zero.
    let zero_err = Tolerance::new(0.0).expect_err("zero tolerance must reject");
    let proj_err3: ProjectionError = zero_err.into();
    assert!(matches!(
        proj_err3,
        ProjectionError::Tolerance(ToleranceError::Invalid { .. })
    ));
}

// ---------------------------------------------------------------------------
// 3. ProjectionError::NoBRepHandle — direct construction
// ---------------------------------------------------------------------------

/// `ProjectionError::NoBRepHandle { entity }` is declared on the public
/// `ProjectionError` enum but no source-side path currently constructs it.
/// (The projection layer's `tick`/`project`/`spawn_brep_entity` reach for
/// the `EntityCadMap`, not the world's `BRepHandle` component, so a
/// missing-handle case is currently a `None` result rather than a
/// `NoBRepHandle` error.)
///
/// We exercise the variant directly:
///
/// * Construct a `NoBRepHandle { entity: EntityId::new() }` value.
/// * Confirm the `entity` field is reachable + the Display impl emits the
///   expected message format.
///
/// The variant remains a forward-compat error sink for downstream code
/// (e.g. selection-set / picking modules) that may need to require the
/// component as a precondition.
#[test]
fn projection_error_no_brep_handle_variant_constructed_when_entity_lacks_brephandle() {
    let entity = EntityId::new();
    let err = ProjectionError::NoBRepHandle { entity };

    // The entity field round-trips.
    match &err {
        ProjectionError::NoBRepHandle { entity: e } => {
            assert_eq!(*e, entity, "entity field round-trip");
        }
        other => panic!("expected NoBRepHandle; got {other:?}"),
    }

    // Display includes the entity id formatted (the inner Display for
    // EntityId is hex-ulid-like; the wrapper format is "entity {entity} has
    // no BRepHandle").
    let displayed = format!("{err}");
    assert!(
        displayed.contains("has no BRepHandle"),
        "Display impl should mention 'has no BRepHandle'; got: {displayed}"
    );
}

// ---------------------------------------------------------------------------
// 4. ProjectionError::EntityCadMap — driven by `EntityCadMapError::DuplicateNode`
// ---------------------------------------------------------------------------

/// `CadProjection::spawn_brep_entity` uses `EntityCadMap::insert` internally,
/// which fails with `EntityCadMapError::DuplicateNode` when the supplied node
/// is already mapped to another entity. The error converts via
/// `From<EntityCadMapError>` to `ProjectionError::EntityCadMap`.
#[test]
fn projection_error_entity_cad_map_variant_constructed_via_duplicate_node() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let (_cad, node) = build_cuboid_graph();

    // First spawn — must succeed.
    let _entity_a = projection
        .spawn_brep_entity(&mut world, node)
        .expect("first spawn");

    // Second spawn for the SAME node — must fail with EntityCadMap variant.
    let err = projection
        .spawn_brep_entity(&mut world, node)
        .expect_err("duplicate spawn must fail");

    match err {
        ProjectionError::EntityCadMap(inner) => {
            assert!(
                matches!(
                    inner,
                    EntityCadMapError::DuplicateNode { node: n, .. } if n == node
                ),
                "expected DuplicateNode {{ node: {node} }}; got {inner:?}"
            );
        }
        other => panic!("expected ProjectionError::EntityCadMap; got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 5. CadProjection::remap_entity same-node no-op (audit-2 #4 finding;
//    `lib.rs:218-223` — the `existing_entity == entity` early-return path)
// ---------------------------------------------------------------------------

/// Calling `remap_entity(entity, current_node)` where `current_node` is
/// already the entity's binding is a no-op except for marking the entity
/// dirty (so a subsequent `tick` will re-project). This is the
/// "same-old-and-new-node" path documented in `lib.rs:218-223`.
///
/// Specifically asserts:
///
/// * `remap_entity(entity, current_node)` returns `Ok(())`.
/// * The `EntityCadMap` state is unchanged after the call.
/// * The entity ends up in `cache.dirty_entities()` per the documented
///   mark-dirty-on-remap semantics — verified indirectly by ticking and
///   confirming a re-projection occurred.
#[test]
fn cad_projection_remap_entity_same_node_is_idempotent_no_op() {
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let (cad, node) = build_cuboid_graph();
    let entity = projection
        .spawn_brep_entity(&mut world, node)
        .expect("spawn");

    // Run the first tick to populate the cache + clear dirty state.
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);

    // Pre-state: cad-node binding set; cache has a mesh; nothing dirty.
    assert_eq!(projection.node_for(entity), Some(node));
    assert_eq!(projection.entity_for(node), Some(entity));
    assert!(projection.projected_mesh(entity).is_some());

    // Calling remap_entity with the SAME (entity, node) pair is a no-op.
    projection
        .remap_entity(entity, node)
        .expect("same-node remap is a no-op");

    // EntityCadMap state UNCHANGED.
    assert_eq!(
        projection.node_for(entity),
        Some(node),
        "node_for must be unchanged after same-node remap"
    );
    assert_eq!(
        projection.entity_for(node),
        Some(entity),
        "entity_for must be unchanged after same-node remap"
    );

    // Dirty-state side effect: the entity was marked dirty, so a follow-up
    // tick MUST re-project it (cache hit count would be 0 for this entity,
    // and the report would record entities_reprojected >= 1).
    let r2 = projection.tick(&mut world, &cad, tol()).expect("tick2");
    assert_eq!(
        r2.entities_reprojected, 1,
        "same-node remap must mark dirty so the next tick re-projects \
         (got entities_reprojected = {} — non-1 means the dirty-bit \
         contract was not honored)",
        r2.entities_reprojected
    );

    // A third remap-then-tick combo confirms the no-op behavior is stable
    // across repeated calls.
    projection
        .remap_entity(entity, node)
        .expect("repeat same-node remap is also a no-op");
    let r3 = projection.tick(&mut world, &cad, tol()).expect("tick3");
    assert_eq!(r3.entities_reprojected, 1);
}
