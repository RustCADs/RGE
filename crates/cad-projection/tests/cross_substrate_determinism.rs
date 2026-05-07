//! Audit-2 A2.6 closure: cross-substrate Boolean × cad-projection × `PieSnapshot`
//! byte-identity gate.
//!
//! Builds a non-trivial `CadGraph` (Cuboid + Transform-translated-Cuboid +
//! `BooleanOp::union`), spawns a `BRepHandle` entity for the union, drives a
//! projection tick, captures both `&cad` and `&projection` participants in a
//! PIE envelope, restores into fresh substrates, drives another projection
//! tick, and asserts the resulting mesh bytes (positions + indices) are
//! byte-identical between the original and restored projections.
//!
//! This is the core determinism gate: any drift between original and
//! post-round-trip mesh bytes indicates either a non-determinism bug in
//! csgrs's BSP triangulation or a missing-state bug in the
//! `SnapshotParticipate` impls (cad-graph or projection).
//!
//! Per dispatch §3 step 9, also re-evaluates the union node directly via
//! `cad.graph().evaluate(...)` (bypassing the projection layer) and asserts
//! byte-identity to the round-tripped mesh — verifies cad-graph's RON
//! serialization preserves every operator parameter exactly. Because the
//! cad-projection's `project()` function copies positions / indices straight
//! out of the underlying `Tessellation`, the projected mesh is bit-for-bit
//! identical to the operator's tessellation output (the projection layer
//! adds provenance metadata but does not transform geometry).

use rge_cad_core::{
    BooleanOp, CadGraph, CuboidOp, OperatorNode, TessellationCache, Tolerance, TransformOp,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{ParticipantId, PieSnapshot, SnapshotParticipate, World};
use rge_kernel_graph_foundation::NodeId;
use rge_physics::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use rge_physics::{World as PhysicsWorld, PHYSICS_WORLD_PARTICIPANT_ID};

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Build `CadGraph`: `Cuboid_a` + Transform(translated `Cuboid_b`) + Boolean(Union)
/// — the union node is the root.
fn build_union_graph() -> (CadGraph, NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("cube_a");
    // Slightly perturbed depth so cube_b's content-derived NodeId differs.
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cube_b");
    let xform = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("xform");
    g.connect(cube_b, xform, 0).expect("c1");

    let union = g
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("union");
    g.connect(cube_a, union, 0).expect("c2");
    g.connect(xform, union, 1).expect("c3");
    g.set_root(union).expect("root");
    cad.commit("Boolean(Union) of two cuboids").expect("commit");
    (cad, union)
}

/// Audit-2 A2.6 closure: full Boolean × cad-projection × `PieSnapshot`
/// round-trip byte-identity check, soaked over **100 iterations** to catch
/// non-determinism that would only surface intermittently. Mirrors the
/// 100-iter pattern in `cad_boolean_determinism.rs:70,104`.
///
/// Per-iter: re-capture from the original substrate, restore into a fresh
/// substrate, tick, and assert the restored mesh is byte-identical to the
/// reference established before the loop. Equality is exact via direct
/// `Vec<[f32; 3]>` / `Vec<u32>` comparison — `f32::PartialEq` matches the
/// raw little-endian bit pattern (NaN excepted, but no operator in this
/// test produces NaN), so we get the same byte-identity guarantee as a
/// BLAKE3 hash without needing to add the `blake3` crate as a test dep.
/// Any drift at any iteration is a major non-determinism finding.
///
/// Setup work (building the union graph, the original substrate's first
/// projection tick) is hoisted outside the soak loop — only the
/// capture / restore / tick / assert path lives inside.
#[test]
fn boolean_through_cad_projection_via_pie_snapshot_round_trip_byte_identity() {
    // ---- One-time setup: original substrate (hoisted outside the soak loop) ----
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();
    let (cad, union_node) = build_union_graph();

    let entity = projection
        .spawn_brep_entity(&mut world, union_node)
        .expect("spawn");
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);

    let mesh_original = projection.projected_mesh(entity).expect("mesh1").clone();
    assert!(
        mesh_original.vertex_count() > 0,
        "original union mesh must be non-empty"
    );
    assert!(
        mesh_original.triangle_count() > 0,
        "original union mesh must have triangles"
    );

    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");

    // ---- 100-iter PIE round-trip soak: capture / restore / tick / assert ----
    for iter in 0..100 {
        let snap = PieSnapshot::capture(
            &world,
            &[
                &cad as &dyn SnapshotParticipate,
                &projection as &dyn SnapshotParticipate,
            ],
        )
        .unwrap_or_else(|e| panic!("pie capture iter {iter}: {e:?}"));
        assert_eq!(snap.participants.len(), 2, "iter {iter}");

        let mut fresh_world = World::new();
        fresh_world.register_snapshot_component::<BRepHandle>();
        let mut fresh_cad = CadGraph::new();
        let mut fresh_projection = CadProjection::new();
        snap.restore(
            &mut fresh_world,
            &mut [
                (&pid_cad, &mut fresh_cad as &mut dyn SnapshotParticipate),
                (
                    &pid_proj,
                    &mut fresh_projection as &mut dyn SnapshotParticipate,
                ),
            ],
        )
        .unwrap_or_else(|e| panic!("pie restore iter {iter}: {e:?}"));

        let r2 = fresh_projection
            .tick(&mut fresh_world, &fresh_cad, tol())
            .unwrap_or_else(|e| panic!("tick post-restore iter {iter}: {e:?}"));
        assert_eq!(r2.entities_reprojected, 1, "iter {iter}");

        let mesh_restored = fresh_projection
            .projected_mesh(entity)
            .unwrap_or_else(|| panic!("mesh restored iter {iter}"))
            .clone();

        // Byte-identity gate per iter: positions / indices / source_node.
        // Detailed assertions localize a future drift to its first
        // diverging dimension (positions vs indices vs source_node).
        assert_eq!(
            mesh_original.positions,
            mesh_restored.positions,
            "iter {iter}: Boolean × cad-projection × PieSnapshot round-trip \
             drifted positions (cad-graph or projection serialization is \
             non-deterministic, OR csgrs BSP is producing different output \
             across runs); vertex_count: ref={} this={}",
            mesh_original.vertex_count(),
            mesh_restored.vertex_count(),
        );
        assert_eq!(
            mesh_original.indices, mesh_restored.indices,
            "iter {iter}: indices drifted",
        );
        // Source node id is content-addressed; restoring the cad-graph
        // yields the same NodeId, so source_node must match too.
        assert_eq!(
            mesh_original.source_node, mesh_restored.source_node,
            "iter {iter}: source_node drifted",
        );

        // Per dispatch step 9, ALSO re-evaluate the union node directly via
        // OperatorGraph::evaluate (bypassing the projection layer) and
        // assert byte-identity to the round-tripped mesh. Verifies cad-
        // graph's RON serialization preserves operator parameters exactly.
        // Run this on iter 0 and a sampling of later iters to keep overall
        // soak runtime bounded without sacrificing coverage of the bypass
        // path's determinism.
        if iter == 0 || iter == 50 || iter == 99 {
            let mut bypass_cache = TessellationCache::new();
            let bypass_tess = fresh_cad
                .graph()
                .evaluate(union_node, &mut bypass_cache, tol())
                .unwrap_or_else(|e| panic!("bypass eval iter {iter}: {e:?}"));
            assert_eq!(
                bypass_tess.positions, mesh_restored.positions,
                "iter {iter}: OperatorGraph::evaluate divergent from projection-layer mesh",
            );
            assert_eq!(
                bypass_tess.indices, mesh_restored.indices,
                "iter {iter}: OperatorGraph::evaluate index divergent",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// H4: 3-way PIE composition test (audit-2026-05-09 round 6).
//
// Adds physics::World as a third SnapshotParticipate participant alongside the
// existing 2-way (cad-graph + cad-projection) round-trip — closes the gap of
// "no test composes all 3 SnapshotParticipate impls in a single PIE
// envelope". This proves that:
//
// 1. RON (cad-core's wire format) + postcard (cad-projection) + postcard
//    (physics) coexist cleanly inside a single PieSnapshot's per-participant
//    payload map (the envelope length-prefixes each payload, so the per-
//    participant format is private to that participant — but the envelope's
//    `to_bytes` / `from_bytes` cycle has to handle whatever bytes each one
//    emits).
// 2. The 3-way envelope round-trips byte-identically through to_bytes →
//    from_bytes → restore, end-to-end (a stronger gate than the 2-way test
//    which only round-trips logical state).
// 3. Ticking the restored 3-way state produces a byte-identical projection
//    mesh AND byte-identical physics state digest, soaked across 50
//    iterations to catch intermittent non-determinism (50 not 100 because
//    a 3-way compose-of-compose is more expensive than a 2-way; the
//    audit-2026-05-09 round-6 budget for compose-of-compose tests is 50
//    iterations).
//
// Determinism digest: physics state is verified via BLAKE3 of
// `World::serialize_state()`, mirroring the precedent in
// `physics::participate.rs::tests::physics_snapshot_participate_round_trip_populated_world`.
// Projected mesh bytes are verified via BLAKE3 of position + index byte
// streams, mirroring physics's same hash precedent (the existing 2-way test
// uses direct Vec equality; for the 3-way variant we use BLAKE3 to localise
// drift to a single 32-byte digest comparison and to reduce per-iter
// assertion overhead at 50 iters).
// ---------------------------------------------------------------------------

/// BLAKE3 digest of the position + index byte streams of a [`ProjectedMesh`]
/// — a content-derived 32-byte fingerprint suitable for byte-identity
/// comparison across PIE round-trips at 50-iter cadence. Mirrors the digest
/// pattern in `physics::participate.rs::tests`.
///
/// We feed the BLAKE3 hasher a postcard encoding of the positions + indices
/// rather than reinterpreting the slices directly: this is the workspace
/// convention (postcard is already a cad-projection regular dep) and avoids
/// pulling `bytemuck` as a new dev-dep. postcard is fully deterministic for
/// `Vec<[f32; 3]>` / `Vec<u32>` (no map iteration, no float text formatting),
/// so byte-equal logical state always produces byte-equal digests.
fn mesh_digest(mesh: &rge_cad_projection::ProjectedMesh) -> blake3::Hash {
    let pos_bytes = postcard::to_allocvec(&mesh.positions).expect("encode positions");
    let idx_bytes = postcard::to_allocvec(&mesh.indices).expect("encode indices");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&pos_bytes);
    hasher.update(&idx_bytes);
    hasher.finalize()
}

/// Build the same scene factory as the H4 spec calls for: a populated
/// physics world (1 ground + 2 dynamic cubes) — non-trivial enough that
/// the digest is meaningfully sensitive to drift, but small enough that
/// 50-iter capture/restore is fast.
fn build_physics_scene() -> PhysicsWorld {
    let mut w = PhysicsWorld::new();
    let _ground = w.insert_body(
        RigidBody {
            kind: BodyKind::Fixed,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Plane,
            ..Collider::default()
        }),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    let _cube_a = w.insert_body(
        RigidBody {
            kind: BodyKind::Dynamic,
            mass: 1.0,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Cuboid {
                hx: 0.5,
                hy: 0.5,
                hz: 0.5,
            },
            ..Collider::default()
        }),
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    let _cube_b = w.insert_body(
        RigidBody {
            kind: BodyKind::Dynamic,
            mass: 2.0,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Cuboid {
                hx: 0.25,
                hy: 0.25,
                hz: 0.25,
            },
            ..Collider::default()
        }),
        [0.5, 4.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    w
}

/// 3-way PIE composition round-trip soak: a single [`PieSnapshot`] carrying
/// `cad-graph` (RON) + `cad-projection` (postcard) + `physics::World`
/// (postcard) participants is captured, written to bytes via
/// [`PieSnapshot::to_bytes`], rehydrated via [`PieSnapshot::from_bytes`],
/// and restored into fresh fixtures. Asserts byte-identity of the
/// projected mesh AND of the physics serialise-state digest, soaked over
/// **50 iterations** (the audit-2026-05-09 round-6 budget for compose-of-
/// compose tests; existing 2-way is 100 because it's cheaper).
///
/// This is the load-bearing cross-format coexistence proof:
///
/// - `cad-graph` participant payload is RON (forced by the internally-
///   tagged `OperatorNode` enum, per `cad-core/checkpoints/participate.rs`).
/// - `cad-projection` participant payload is postcard (compact and stable
///   on a payload that's just a struct-of-vecs).
/// - `physics::World` participant payload is postcard (rapier's
///   `serde-serialize` feature gates `derive(Serialize, Deserialize)` on
///   the persistent rapier types; postcard is the workspace default).
///
/// The PIE envelope length-prefixes each payload (per
/// `kernel/ecs/src/participate.rs::PieSnapshot::to_bytes`), so the three
/// formats coexist without conflict — but a regression in any participant's
/// `capture` / `restore` pair, or in the envelope's length-prefix
/// preservation, surfaces as either a `from_bytes` failure or a per-iter
/// digest divergence on this test.
///
/// Per spec: NOT modifying the existing 100-iter 2-way test —
/// `boolean_through_cad_projection_via_pie_snapshot_round_trip_byte_identity`
/// remains the 2-way determinism gate; this is the additive 3-way variant.
#[test]
fn pie_three_participant_round_trip_50_iter() {
    // ---- Setup: build all three substrates once. ----
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();
    let (cad, union_node) = build_union_graph();

    let entity = projection
        .spawn_brep_entity(&mut world, union_node)
        .expect("spawn");
    let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
    assert_eq!(r1.entities_reprojected, 1);

    let mesh_original = projection.projected_mesh(entity).expect("mesh1").clone();
    assert!(
        mesh_original.vertex_count() > 0,
        "original union mesh must be non-empty",
    );
    let mesh_digest_original = mesh_digest(&mesh_original);
    let mesh_id_original = world
        .entity(entity)
        .expect("entity preserved")
        .get::<BRepHandle>()
        .expect("brep handle present")
        .mesh_id
        .expect("mesh_id populated after first projection tick");

    let physics = build_physics_scene();
    let physics_digest_original = blake3::hash(&physics.serialize_state());
    let physics_body_count_original = physics.body_count();

    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");
    let pid_physics = ParticipantId::new(PHYSICS_WORLD_PARTICIPANT_ID);

    // ---- 50-iter 3-way PIE round-trip soak. ----
    for iter in 0..50 {
        // Capture all three participants into one envelope.
        let snap = PieSnapshot::capture(
            &world,
            &[
                &cad as &dyn SnapshotParticipate,
                &projection as &dyn SnapshotParticipate,
                &physics as &dyn SnapshotParticipate,
            ],
        )
        .unwrap_or_else(|e| panic!("pie capture iter {iter}: {e:?}"));
        assert_eq!(
            snap.participants.len(),
            3,
            "iter {iter}: three participants captured",
        );

        // Cross-format envelope verification: write to bytes, rehydrate,
        // then assert byte-identity of the bytes-of-bytes (envelope is
        // deterministic per `PieSnapshot::to_bytes` doc) so any drift in
        // one participant's wire format would show up here as a non-equal
        // bytes2.
        let bytes1 = snap.to_bytes();
        let snap2 = PieSnapshot::from_bytes(&bytes1)
            .unwrap_or_else(|e| panic!("from_bytes iter {iter}: {e:?}"));
        let bytes2 = snap2.to_bytes();
        assert_eq!(
            bytes1, bytes2,
            "iter {iter}: 3-way envelope bytes must be byte-identical \
             after a to_bytes / from_bytes / to_bytes cycle (RON + \
             postcard + postcard coexist cleanly inside the envelope)",
        );

        // Restore all three participants from the rehydrated envelope.
        let mut fresh_world = World::new();
        fresh_world.register_snapshot_component::<BRepHandle>();
        let mut fresh_cad = CadGraph::new();
        let mut fresh_projection = CadProjection::new();
        let mut fresh_physics = PhysicsWorld::new();
        snap2
            .restore(
                &mut fresh_world,
                &mut [
                    (&pid_cad, &mut fresh_cad as &mut dyn SnapshotParticipate),
                    (
                        &pid_proj,
                        &mut fresh_projection as &mut dyn SnapshotParticipate,
                    ),
                    (
                        &pid_physics,
                        &mut fresh_physics as &mut dyn SnapshotParticipate,
                    ),
                ],
            )
            .unwrap_or_else(|e| panic!("pie restore iter {iter}: {e:?}"));

        // Projection-side: tick the restored cad-graph through the
        // restored projection, then digest the resulting mesh and compare
        // to the original.
        let r2 = fresh_projection
            .tick(&mut fresh_world, &fresh_cad, tol())
            .unwrap_or_else(|e| panic!("post-restore tick iter {iter}: {e:?}"));
        assert_eq!(r2.entities_reprojected, 1, "iter {iter}");

        let mesh_restored = fresh_projection
            .projected_mesh(entity)
            .unwrap_or_else(|| panic!("mesh restored iter {iter}"))
            .clone();
        let mesh_digest_restored = mesh_digest(&mesh_restored);
        assert_eq!(
            mesh_digest_original, mesh_digest_restored,
            "iter {iter}: projected mesh BLAKE3 digest drifted across the \
             3-way PIE round-trip — either cad-graph RON, cad-projection \
             postcard, or physics postcard mutated something it shouldn't",
        );

        // BRepHandle.mesh_id post-restore: the projection re-derives a
        // fresh mesh_id (its serialized state does NOT include
        // next_mesh_id, per cad-projection/src/lib.rs:377), so we cannot
        // compare against the original mesh_id directly — but we CAN
        // assert it is `Some` (proof the post-restore tick re-projected
        // and bound the entity).
        let mesh_id_restored = fresh_world
            .entity(entity)
            .expect("entity preserved through PIE")
            .get::<BRepHandle>()
            .expect("brep handle present")
            .mesh_id
            .expect("mesh_id re-populated by post-restore tick");
        // Both ids exist; they may or may not be ordinally equal because
        // restore re-allocates the cache from zero. The original mesh_id
        // is captured purely as a sanity sentinel (must remain stable
        // across loop iters since we never re-tick the original).
        let _ = mesh_id_original;
        let _ = mesh_id_restored;

        // Physics-side: serialise-state digest must be byte-identical to
        // the original physics world's digest. The body count is a coarse
        // guard for digest equality (catches structural drift before the
        // hash assertion fires).
        assert_eq!(
            fresh_physics.body_count(),
            physics_body_count_original,
            "iter {iter}: physics body count drifted through PIE — restore \
             must preserve every captured rigid body",
        );
        let physics_digest_restored = blake3::hash(&fresh_physics.serialize_state());
        assert_eq!(
            physics_digest_original, physics_digest_restored,
            "iter {iter}: physics serialize_state BLAKE3 drifted through \
             the 3-way PIE round-trip — either rapier serde-serialize is \
             non-deterministic (would also fail physics's solo PIE test) \
             or the envelope's length-prefix corrupted the physics payload \
             when sandwiched between RON cad-graph + postcard cad-projection",
        );
    }
}
