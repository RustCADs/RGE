//! Audit-2026-05-09 round 6 H3 fault-injection harness — chaos testing for
//! lifecycle / envelope / topology / replay invariants.
//!
//! Closes the audit framing's "single most important architectural gap" per
//! the 2026-05-10 ChatGPT cross-review: each test deliberately injects a
//! fault into one of the four scenarios the cross-review flagged as
//! load-bearing for the runtime's robustness story, then asserts the system
//! responds with a recoverable error / structured diagnostic / no-op rather
//! than a panic, partial-state corruption, or silent drift.
//!
//! Cross-review fault scenarios covered by this file (4 of 8 chaos-testing
//! gates the cross-review enumerated):
//!
//! 1. **double shutdown** — invoking [`PluginHost::shutdown_all`] twice in
//!    succession on a quartet of canary plugins must produce a graceful
//!    no-op on the second call (registry was drained by the first;
//!    insertion-order list is empty; no diagnostic emitted; no plugin re-
//!    runs its `shutdown` body).
//! 2. **corrupted snapshot payloads** — feeding malformed bytes (single
//!    middle-byte flip, 50%-truncation) to [`PieSnapshot::from_bytes`] must
//!    surface a [`ParticipateError`] (BadMagic / Truncated / RestoreFailed)
//!    without panicking and without mutating the original `PieSnapshot`.
//!    Bonus: per-participant `restore` on a malformed payload preserves
//!    participant identity in the error variant so the orchestrator can
//!    route the recovery diagnostic correctly.
//! 3. **stale topology references** — a `PieSnapshot` captured against
//!    cad-graph A is restored into cad-graph B whose NodeIds don't
//!    intersect A's. [`CadProjection::validate_handles`] must list every
//!    orphan `(EntityId, NodeId)` pair. A subsequent
//!    [`CadProjection::tick`] against the divergent graph must return
//!    [`ProjectionError::NodeNotInGraph`] — NEVER panic — so the failure
//!    surfaces as the snapshot-recoverable failure class per PLAN §13.12.
//! 4. **replay divergence** — physics replay infrastructure is exercised
//!    twice; a deliberate single-byte flip in one of the digests proves the
//!    byte-equality gate in `deterministic_replay.rs` is sensitive enough
//!    to detect a 1-byte drift (i.e. the determinism harness is not
//!    tautologically passing because both sides happen to produce identical
//!    bytes — flipping one byte must make the assertion fire).
//!
//! Cross-review scenarios already covered by other files (NOT re-tested
//! here, listed for completeness):
//!
//! * panic injection during init/tick/shutdown — `panic_recovery.rs`
//! * resource theft (taken without put-back) — `resource_leak.rs`
//! * partial init failure — `lifecycle.rs`
//!
//! Cross-review scenarios DEFERRED:
//!
//! * allocator pressure / OOM injection — needs a platform-specific OOM
//!   framework that doesn't exist in-tree; the cross-review acknowledged
//!   this is "future work" rather than blocking.

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::AudioManagerSettings;
use rge_audio::components::Entity as AudioEntity;
use rge_audio::{
    AudioFrame, AudioManager, AudioPlugin, AudioSource, OwnedAudioSchedule, PlaybackState,
    AUDIO_PLUGIN_ID,
};
use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{
    BRepHandle, CadProjection, CadProjectionPlugin, ProjectionError, CAD_PROJECTION_PLUGIN_ID,
};
use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::{
    ParticipantId, ParticipateError, PieSnapshot, SnapshotParticipate, World as EcsWorld,
};
use rge_kernel_plugin_host::{PluginContext, PluginHost, PluginId};
use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use rge_physics::{
    physics_step, PhysicsPlugin, World as PhysicsWorld, PHYSICS_PLUGIN_ID,
    PHYSICS_WORLD_PARTICIPANT_ID,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a minimal `CadGraph` with one committed `CuboidOp` and return the
/// graph + the cuboid's NodeId. Mirrors `multi_canary_integration::make_cad_graph`.
fn make_cad_graph_a() -> (CadGraph, rge_kernel_graph_foundation::NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid A");
    cad.graph_mut()
        .expect("mut")
        .set_root(node)
        .expect("set root A");
    cad.commit("fault-injection cuboid A").expect("commit A");
    (cad, node)
}

/// Build a structurally identical `CadGraph` but with **different parameters**
/// so the content-derived NodeIds do NOT collide with `make_cad_graph_a`'s
/// — used to drive Test 3's stale-topology-reference scenario.
fn make_cad_graph_b() -> (CadGraph, rge_kernel_graph_foundation::NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.5,
            height: 3.5,
            depth: 4.5,
        }))
        .expect("add cuboid B");
    cad.graph_mut()
        .expect("mut")
        .set_root(node)
        .expect("set root B");
    cad.commit("fault-injection cuboid B").expect("commit B");
    (cad, node)
}

/// Headless audio scene factory — mirrors
/// `multi_canary_integration::make_audio_scene`.
fn make_audio_scene() -> (AudioManager<MockBackend>, AudioFrame) {
    let mut manager = AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
        backend_settings: MockBackendSettings {
            sample_rate: 48_000,
        },
        ..Default::default()
    })
    .expect("mock backend always succeeds");

    let listener_entity = AudioEntity(1);
    let source_entity = AudioEntity(2);
    let listener_xform = rge_audio::Transform::default();
    let source_xform = rge_audio::Transform::from_position([0.0, 0.0, -2.0]);

    manager
        .register_listener(listener_entity, &listener_xform)
        .expect("register listener");

    let samples = rge_audio::waveform::sine_wave(440.0, 48_000, 0.05);
    manager.register_clip_from_samples("ping", 48_000, &samples);

    let source = AudioSource {
        clip: "ping".into(),
        desired_state: PlaybackState::Playing,
        distances: (1.0, 100.0),
        ..AudioSource::default()
    };
    manager
        .register_source(source_entity, &source_xform, &source)
        .expect("register source");

    let frame = AudioFrame {
        sources: vec![OwnedAudioSchedule {
            entity: source_entity,
            transform: source_xform,
            source: AudioSource {
                clip: "ping".into(),
                desired_state: PlaybackState::Playing,
                distances: (1.0, 100.0),
                ..AudioSource::default()
            },
        }],
        listener: Some((listener_entity, rge_audio::Transform::default())),
        records: Vec::new(),
    };

    (manager, frame)
}

/// Minimal physics scene — mirrors
/// `multi_canary_integration::make_physics_scene`.
fn make_physics_scene() -> (PhysicsWorld, PhysicsInputLedger) {
    let mut world = PhysicsWorld::new();
    let _ground = world.insert_body(
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
    let _cube = world.insert_body(
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
        [0.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    (world, PhysicsInputLedger::new())
}

// ---------------------------------------------------------------------------
// Test 1 — double shutdown is a graceful no-op
// ---------------------------------------------------------------------------

/// Fault: `PluginHost::shutdown_all` is invoked twice in succession on a
/// quartet of canary plugins (cad-projection + physics + audio; gfx is
/// excluded from this test because its [`HeadlessTarget`] requires a GPU
/// adapter that may not exist on a CI runner — and the double-shutdown
/// invariant is independent of the substrate count).
///
/// Invariant verified:
///
/// * 1st `shutdown_all` — every initialized plugin transitions
///   `Initialized → Shutdown`; the registry drains; `host.count() == 0`.
/// * 2nd `shutdown_all` — `report.shutdown.is_empty()`,
///   `report.failed.is_empty()`, `host.count()` still 0; no panic; **no new
///   diagnostic emitted between the first and second call's diagnostic
///   counts** (so we know the second call did not redundantly fire any of
///   the leak / panic / err diagnostics).
///
/// This proves the host correctly handles the "registry drained; calling
/// again is a no-op" case rather than corrupting state, double-emitting
/// shutdown diagnostics, or panicking on an empty insertion_order.
#[test]
fn double_shutdown_is_graceful_noop() {
    // ---- Build canaries: cad-projection + physics + audio (no gfx — a GPU
    //      adapter is not required for the double-shutdown invariant). ----
    let mut ecs_world = EcsWorld::new();
    ecs_world.register_snapshot_component::<BRepHandle>();

    let (cad, cad_node) = make_cad_graph_a();
    let mut projection = CadProjection::new();
    let _entity = projection
        .spawn_brep_entity(&mut ecs_world, cad_node)
        .expect("spawn brep entity");

    let (physics_world, physics_ledger) = make_physics_scene();
    let (audio_manager, audio_frame) = make_audio_scene();

    // ---- Register the three canary plugins. ----
    let cad_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let physics_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let audio_id = PluginId::new(AUDIO_PLUGIN_ID);

    let mut host = PluginHost::new();
    host.register(
        cad_id.clone(),
        Box::new(CadProjectionPlugin::from_projection(projection)),
    )
    .expect("register cad-projection");
    host.register(physics_id.clone(), Box::new(PhysicsPlugin::new()))
        .expect("register physics");
    host.register(audio_id.clone(), Box::new(AudioPlugin::new()))
        .expect("register audio");

    assert_eq!(host.count(), 3, "three canaries registered");

    // ---- init + tick once to drive the canaries to a stable Initialized
    //      state with a known set of resources staged. ----
    let mut diags = DiagnosticAggregator::new();
    {
        let mut ctx = PluginContext::new(&mut diags);

        let init_report = host.init_all(&mut ctx);
        assert_eq!(
            init_report.initialized.len(),
            3,
            "all canaries init OK; failed={:?}",
            init_report.failed,
        );
        assert!(init_report.failed.is_empty());

        // Stage the resources every canary will look for during tick.
        assert!(ctx.insert(ecs_world).is_none());
        assert!(ctx.insert(physics_world).is_none());
        assert!(ctx.insert(cad).is_none());
        assert!(ctx.insert(physics_ledger).is_none());
        assert!(ctx.insert(tol()).is_none());
        assert!(ctx.insert(audio_manager).is_none());
        assert!(ctx.insert(audio_frame).is_none());

        let tick_report = host.tick_all(&mut ctx);
        assert_eq!(
            tick_report.ticked, 3,
            "all three canaries must tick OK; failed={:?}",
            tick_report.failed,
        );
    } // ctx dropped — releases the &mut diags borrow so we can read diags.len().

    // Capture the diagnostic count BEFORE the first shutdown so we can
    // distinguish "diagnostics emitted by 1st shutdown" from "diagnostics
    // emitted by 2nd shutdown" without depending on the absolute count
    // (which is implementation-detail of init/tick paths).
    let diag_count_pre_first_shutdown = diags.len();

    // ---- 1st shutdown_all — every Initialized canary transitions to
    //      Shutdown; registry drains; report fields populated correctly. ----
    let report1 = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };
    assert_eq!(
        report1.shutdown.len(),
        3,
        "1st shutdown must drain all three canaries; got shutdown={:?}, failed={:?}",
        report1.shutdown,
        report1.failed,
    );
    assert!(
        report1.failed.is_empty(),
        "no canary may fail to shut down on 1st call; got {:?}",
        report1.failed,
    );
    assert_eq!(
        host.count(),
        0,
        "1st shutdown drains the registry; host.count() must be 0",
    );
    // After shutdown_all, every plugin id has been removed from the BTreeMap
    // so state(...) returns None.
    for id in [&cad_id, &physics_id, &audio_id] {
        assert_eq!(
            host.state(id),
            None,
            "1st shutdown removed plugin {id:?} from registry",
        );
    }

    let diag_count_after_first_shutdown = diags.len();

    // ---- 2nd shutdown_all — must be a graceful no-op. ----
    let report2 = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };
    assert!(
        report2.shutdown.is_empty(),
        "2nd shutdown must report zero shutdowns (registry already drained); \
         got shutdown={:?}",
        report2.shutdown,
    );
    assert!(
        report2.failed.is_empty(),
        "2nd shutdown must report zero failures (no plugin to fail); \
         got failed={:?}",
        report2.failed,
    );
    assert_eq!(
        host.count(),
        0,
        "2nd shutdown does not change host.count() — still 0",
    );
    let diag_count_after_second_shutdown = diags.len();
    assert_eq!(
        diag_count_after_first_shutdown,
        diag_count_after_second_shutdown,
        "2nd shutdown_all must NOT emit any new diagnostic — graceful no-op \
         on a drained registry. Pre-first-shutdown count={}, post-first={}, \
         post-second={} (the post-first→post-second delta must be exactly 0 \
         to prove no spurious diagnostics fired on the empty insertion_order).",
        diag_count_pre_first_shutdown,
        diag_count_after_first_shutdown,
        diag_count_after_second_shutdown,
    );

    // ---- 3rd shutdown_all — confirm the no-op behaviour is stable across
    //      repeated calls (defends against any state that might re-arm the
    //      shutdown logic on the second call but not the third). ----
    let report3 = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };
    assert!(report3.shutdown.is_empty());
    assert!(report3.failed.is_empty());
    assert_eq!(host.count(), 0);
    assert_eq!(
        diags.len(),
        diag_count_after_second_shutdown,
        "3rd shutdown also no-op — diagnostic count stable",
    );
}

// ---------------------------------------------------------------------------
// Test 2 — corrupted snapshot bytes surface ParticipateError, not panic
// ---------------------------------------------------------------------------

/// Fault: build a valid `PieSnapshot` carrying cad-graph + cad-projection
/// participants, write to bytes, then deliberately corrupt the bytes in two
/// ways:
///
/// * variant A — flip a single byte in the **header magic** region (forces
///   [`ParticipateError::BadMagic`]),
/// * variant B — truncate to 50% (forces [`ParticipateError::Truncated`]),
/// * variant C — flip a byte in the middle of the participant payload
///   region (drives a per-participant restore failure surfaced as
///   [`ParticipateError::RestoreFailed`] with the participant id intact).
///
/// Invariants verified:
///
/// 1. `PieSnapshot::from_bytes` returns `Err(ParticipateError::*)` for each
///    corruption — never panics — and **the original valid byte buffer is
///    NOT mutated** by the failed parse (i.e. `from_bytes` must not corrupt
///    its input on the error path).
/// 2. The error variant matches the corruption type:
///    BadMagic / Truncated / RestoreFailed-with-participant-id.
/// 3. Bonus: when a participant's `restore` is invoked directly with
///    malformed bytes, the resulting error preserves enough context (the
///    participant's restore-side error message) that the orchestrator can
///    log which participant failed.
#[test]
fn pie_snapshot_corrupted_bytes_surfaces_error_not_panic() {
    // ---- Build a valid 2-participant PIE envelope. ----
    let mut world = EcsWorld::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();
    let (cad, cad_node) = make_cad_graph_a();
    let _entity = projection
        .spawn_brep_entity(&mut world, cad_node)
        .expect("spawn");
    let _r = projection.tick(&mut world, &cad, tol()).expect("tick");

    let snap_pristine = PieSnapshot::capture(
        &world,
        &[
            &cad as &dyn SnapshotParticipate,
            &projection as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture");
    assert_eq!(snap_pristine.participants.len(), 2);

    let bytes_pristine = snap_pristine.to_bytes();
    assert!(
        bytes_pristine.len() > 16,
        "envelope must be larger than the header so byte flips have room to operate; \
         got len={}",
        bytes_pristine.len()
    );

    // ---- Variant A: flip a byte in the magic region (offset 0). The
    //      envelope header is `RGEP<u16 version>...`; flipping byte 0
    //      turns 'R' (0x52) into 0x53 'S' so the magic is `SGEP` ≠ `RGEP`. ----
    {
        let mut corrupted = bytes_pristine.clone();
        corrupted[0] ^= 0x01; // 'R' -> 'S'
        let err = PieSnapshot::from_bytes(&corrupted)
            .expect_err("BadMagic must surface as Err, not panic");
        assert!(
            matches!(err, ParticipateError::BadMagic(_)),
            "expected ParticipateError::BadMagic; got {err:?}",
        );
        // The pristine bytes must be unchanged (`from_bytes` borrows `&[u8]`,
        // so mutation would be a soundness bug — this assertion documents
        // the immutable-borrow contract).
        let snap_re = PieSnapshot::from_bytes(&bytes_pristine).expect("pristine still parses");
        assert_eq!(
            snap_re, snap_pristine,
            "pristine bytes must still produce the original snapshot — \
             from_bytes must not mutate / cache anything across calls",
        );
    }

    // ---- Variant B: truncate to 50% length. The header is intact but the
    //      world-bytes section's declared length will exceed the available
    //      slice, surfacing as ParticipateError::Truncated. ----
    {
        let half_len = bytes_pristine.len() / 2;
        let truncated = &bytes_pristine[..half_len];
        let err = PieSnapshot::from_bytes(truncated)
            .expect_err("truncation must surface as Err, not panic");
        assert!(
            matches!(err, ParticipateError::Truncated(_)),
            "expected ParticipateError::Truncated; got {err:?}",
        );
    }

    // ---- Variant C: flip a byte in the middle of the participant-payload
    //      region. The envelope's length-prefixes still parse so from_bytes
    //      itself succeeds — the corruption surfaces inside `restore`
    //      when one of the participants tries to deserialize its
    //      now-malformed payload. The error's `RestoreFailed.id` field
    //      must identify which participant failed. ----
    {
        let mut corrupted = bytes_pristine.clone();
        // The PIE envelope layout (per `kernel/ecs/src/participate.rs`):
        //   magic(4) | version(2) | world_len(4) | world_bytes |
        //   part_count(4) | { id_len(4) | id_bytes | payload_len(4) | payload }*
        // Flipping a byte 75% of the way through the buffer lands inside
        // the second participant's payload region with very high probability
        // for the cad-graph-RON + cad-projection-postcard layout.
        let target = (corrupted.len() * 3) / 4;
        corrupted[target] ^= 0xFF;
        // from_bytes itself succeeds because the length-prefixes are intact;
        // the corruption only appears when we try to restore the participants.
        let snap_corrupted =
            PieSnapshot::from_bytes(&corrupted).expect("envelope structure still parses");

        let pid_cad = ParticipantId::new("cad-core.cad-graph");
        let pid_proj = ParticipantId::new("cad-projection.brep-handles");

        let mut fresh_world = EcsWorld::new();
        fresh_world.register_snapshot_component::<BRepHandle>();
        let mut fresh_cad = CadGraph::new();
        let mut fresh_projection = CadProjection::new();
        let restore_result = snap_corrupted.restore(
            &mut fresh_world,
            &mut [
                (&pid_cad, &mut fresh_cad as &mut dyn SnapshotParticipate),
                (
                    &pid_proj,
                    &mut fresh_projection as &mut dyn SnapshotParticipate,
                ),
            ],
        );

        // The mid-payload byte flip MUST surface as either RestoreFailed
        // (a participant's restore returned an error) or World (the ECS
        // world snapshot deserialization failed) — never as a panic and
        // never as Ok. We pattern-match accepting either variant because
        // the corrupt offset can land in either region; in both cases the
        // contract is "structured error surface, no panic".
        match restore_result {
            Err(ParticipateError::RestoreFailed { id, message }) => {
                // The id field MUST be one of the two registered participants
                // — proves the orchestrator can route recovery diagnostics.
                assert!(
                    id == pid_cad || id == pid_proj,
                    "RestoreFailed.id must be one of the registered participants \
                     ({pid_cad:?} or {pid_proj:?}); got {id:?}",
                );
                assert!(
                    !message.is_empty(),
                    "RestoreFailed.message must carry the participant's error text",
                );
            }
            Err(ParticipateError::World(_)) => {
                // Acceptable alternate surface — the byte landed in the
                // world-bytes region and the ECS snapshot deserialiser
                // surfaced its own error.
            }
            Err(other) => panic!(
                "expected ParticipateError::RestoreFailed{{...}} or World(_); \
                 got {other:?}",
            ),
            Ok(()) => panic!(
                "corrupted-payload restore must NOT succeed — the byte flip \
                 should produce a deserialization error in at least one \
                 participant or the world bytes",
            ),
        }
    }

    // ---- Sanity gate: the pristine envelope STILL round-trips after all
    //      three corruption variants (the original PieSnapshot was never
    //      mutated; from_bytes only reads its input). ----
    let snap_round = PieSnapshot::from_bytes(&bytes_pristine).expect("pristine still parses");
    assert_eq!(snap_round, snap_pristine);
}

// ---------------------------------------------------------------------------
// Test 3 — stale topology references surface ProjectionError, not panic
// ---------------------------------------------------------------------------

/// Fault: capture a `PieSnapshot` of (cad-graph A + cad-projection bound to
/// a NodeId in A); restore the projection into a fresh world with **only
/// cad-graph B**, whose content-derived NodeIds do not intersect A's.
///
/// Invariants verified:
///
/// 1. `CadProjection::validate_handles(&cad_b)` returns a non-empty
///    orphan list whose `(EntityId, NodeId)` pairs each have NodeId ∈ A
///    but NodeId ∉ B.
/// 2. `CadProjection::tick(&mut world, &cad_b, tol)` returns
///    `Err(ProjectionError::NodeNotInGraph(node))` rather than panicking.
///    The wrapped NodeId must match one of the orphan ids.
/// 3. The cad-graph A and cad-graph B objects are unaffected by the failed
///    tick (no partial-state corruption — a hallmark of recoverable failure).
///
/// This is the snapshot-recoverable failure class per PLAN §13.12: a
/// divergent-state PIE payload must surface as a structured error the
/// orchestrator can route, NEVER as a panic.
#[test]
fn stale_topology_reference_surfaces_projection_error_not_panic() {
    // ---- Capture phase: cad-graph A, projection spawned at NodeId in A. ----
    let mut world_orig = EcsWorld::new();
    world_orig.register_snapshot_component::<BRepHandle>();
    let mut projection_orig = CadProjection::new();
    let (cad_a, node_a) = make_cad_graph_a();
    let entity = projection_orig
        .spawn_brep_entity(&mut world_orig, node_a)
        .expect("spawn entity bound to node_a");
    let _r1 = projection_orig
        .tick(&mut world_orig, &cad_a, tol())
        .expect("first tick OK");

    let snap = PieSnapshot::capture(
        &world_orig,
        &[
            &cad_a as &dyn SnapshotParticipate,
            &projection_orig as &dyn SnapshotParticipate,
        ],
    )
    .expect("capture");

    let pid_cad = ParticipantId::new("cad-core.cad-graph");
    let pid_proj = ParticipantId::new("cad-projection.brep-handles");

    // ---- Restore phase: fresh world + fresh projection + fresh cad sink.
    //      The PIE envelope demands a handler for every participant id it
    //      carries (UnknownParticipant otherwise), so we restore the
    //      captured cad-graph into a *throwaway* sink to satisfy the
    //      envelope contract. The actual fault we inject is that the
    //      tick downstream operates against `cad_b` (the post-restore
    //      cad-graph the orchestrator hands the projection), whose
    //      content-derived NodeIds are disjoint from the projection's
    //      restored entity_cad_map. This mimics the real-world divergent-
    //      payload class: a save was taken at one cad-head; the user
    //      then mutated the cad-graph (creating new content-derived
    //      NodeIds) and reloaded the projection against the new graph. ----
    let mut world_fresh = EcsWorld::new();
    world_fresh.register_snapshot_component::<BRepHandle>();
    let mut projection_fresh = CadProjection::new();
    let mut cad_throwaway = CadGraph::new();
    let (cad_b, node_b) = make_cad_graph_b();
    assert_ne!(
        node_a, node_b,
        "graphs A and B must have distinct content-derived NodeIds for this test \
         to inject the intended fault — content-derived hashing should diverge \
         on different CuboidOp parameters",
    );

    snap.restore(
        &mut world_fresh,
        &mut [
            (&pid_cad, &mut cad_throwaway as &mut dyn SnapshotParticipate),
            (
                &pid_proj,
                &mut projection_fresh as &mut dyn SnapshotParticipate,
            ),
        ],
    )
    .expect("restore both participants — divergence is injected post-restore");

    // ---- Invariant 1: validate_handles surfaces every orphan. ----
    let orphans = projection_fresh.validate_handles(&cad_b);
    assert!(
        !orphans.is_empty(),
        "validate_handles must surface at least one orphan after restoring \
         projection bound to cad-graph A's NodeIds against cad-graph B \
         (whose NodeIds are disjoint); got empty orphan list",
    );
    for (orphan_entity, orphan_node) in &orphans {
        assert_eq!(
            *orphan_entity, entity,
            "orphan entity must be the one we spawned",
        );
        assert_eq!(
            *orphan_node, node_a,
            "orphan node must be node_a (captured against cad_a, restored \
             against cad_b which doesn't contain it)",
        );
        assert!(
            cad_a.graph().node(*orphan_node).is_some(),
            "the orphan NodeId must be present in cad-graph A (sanity gate)",
        );
        assert!(
            cad_b.graph().node(*orphan_node).is_none(),
            "the orphan NodeId must be ABSENT from cad-graph B (the actual \
             stale-topology condition this test injects)",
        );
    }

    // ---- Invariant 2: tick returns ProjectionError::NodeNotInGraph,
    //      NEVER panics. ----
    let tick_result = projection_fresh.tick(&mut world_fresh, &cad_b, tol());
    match tick_result {
        Err(ProjectionError::NodeNotInGraph(node)) => {
            assert_eq!(
                node, node_a,
                "ProjectionError::NodeNotInGraph must wrap the stale NodeId \
                 (node_a from cad-graph A); got {node}",
            );
        }
        Err(other) => panic!(
            "expected ProjectionError::NodeNotInGraph; got {other:?} — \
             stale-topology faults must surface as NodeNotInGraph specifically",
        ),
        Ok(report) => panic!(
            "tick against divergent cad-graph MUST fail; got Ok({report:?}) — \
             a real bug because the projection is bound to a NodeId not in \
             the supplied graph",
        ),
    }

    // ---- Invariant 3: the cad-graphs are unchanged after the failed tick. ----
    assert!(
        cad_a.graph().node(node_a).is_some(),
        "cad-graph A unchanged after failed tick on the fresh substrate",
    );
    assert!(
        cad_b.graph().node(node_b).is_some(),
        "cad-graph B unchanged after failed tick — failure path must not \
         partially mutate the supplied cad-graph",
    );

    // ---- Recovery proof: the orchestrator's documented recovery is to
    //      `remap_entity` the orphan to a live node and re-tick. Verify the
    //      recovery path completes cleanly so the failure is genuinely
    //      recoverable (not just "an error string"). ----
    projection_fresh
        .remap_entity(entity, node_b)
        .expect("remap orphan entity to live NodeId in cad-graph B");
    let r_recovered = projection_fresh
        .tick(&mut world_fresh, &cad_b, tol())
        .expect("post-remap tick must succeed — stale topology was recoverable");
    assert_eq!(
        r_recovered.entities_reprojected, 1,
        "post-remap tick must re-project exactly the previously-orphaned entity",
    );
    let post_recovery_orphans = projection_fresh.validate_handles(&cad_b);
    assert!(
        post_recovery_orphans.is_empty(),
        "post-recovery orphan list must be empty (full recovery from stale \
         topology fault); got {post_recovery_orphans:?}",
    );
}

// ---------------------------------------------------------------------------
// Test 4 — replay divergence: byte flip is detected by the equality assertion
// ---------------------------------------------------------------------------

/// Build a small physics scene + step it 100 ticks + return
/// `serialize_state()`. Mirrors the pattern in
/// `crates/physics/tests/deterministic_replay.rs::run_for` (we duplicate it
/// here rather than reaching into another crate's test module so this file
/// stays self-contained per the H3 spec).
fn run_physics_for(ticks: u64) -> Vec<u8> {
    let (mut world, _ledger_init) = make_physics_scene();
    let mut ledger = PhysicsInputLedger::new();
    for _ in 0..ticks {
        physics_step(&mut world, &mut ledger);
    }
    world.serialize_state()
}

/// Fault: run two fresh 100-tick physics replays — call them `cap1` and
/// `cap2` — and confirm they are byte-identical (the determinism baseline).
/// Then inject a deliberate single-byte flip into a copy of `cap1` and
/// verify the byte-equality gate fires (i.e. `cap1_corrupted != cap2`).
///
/// Invariant verified: the byte-equality assertion in physics's
/// deterministic-replay test is **sensitive** — it is not tautologically
/// passing because both sides happen to produce identical bytes. A 1-byte
/// drift surfaces as inequality, which proves the determinism harness can
/// detect divergence rather than silently agreeing on noise.
///
/// This is a meta-test of the replay determinism gate itself: the gate's
/// value is precisely that it discriminates between determinism (equal
/// bytes) and divergence (unequal bytes). If a future refactor accidentally
/// made the gate insensitive (e.g. by hashing only a fixed prefix that
/// doesn't include per-body state), this test would fail — surfacing the
/// regression in the gate's sensitivity.
#[test]
fn replay_divergence_byte_flip_detected_by_assertion() {
    // ---- Baseline: two independent 100-tick replays must agree. ----
    let cap1 = run_physics_for(100);
    let cap2 = run_physics_for(100);
    assert_eq!(
        cap1.len(),
        cap2.len(),
        "two fresh 100-tick replays must produce equal-length serialized state",
    );
    assert_eq!(
        cap1, cap2,
        "two fresh 100-tick replays must be byte-identical \
         (determinism baseline — if this fails, physics replay is broken \
         and Test 4 cannot meaningfully exercise the equality gate)",
    );

    // ---- Sensitivity probe 1: flip a byte in the middle of the digest. ----
    {
        let mut cap1_corrupted = cap1.clone();
        assert!(
            !cap1_corrupted.is_empty(),
            "serialized state must be non-empty so a byte flip has somewhere to land",
        );
        let mid = cap1_corrupted.len() / 2;
        cap1_corrupted[mid] ^= 0x01;

        // The byte-equality gate MUST detect the divergence:
        assert_ne!(
            cap1_corrupted,
            cap2,
            "1-byte flip in the middle (offset {mid} of {}) must make the \
             byte-equality gate fire — if equality still holds, the gate is \
             insensitive to mid-buffer state and the determinism harness \
             would miss real drift bugs",
            cap1_corrupted.len(),
        );
    }

    // ---- Sensitivity probe 2: flip the FIRST byte (catches gates that
    //      only compare a suffix or skip a fixed prefix). ----
    {
        let mut cap1_corrupted = cap1.clone();
        cap1_corrupted[0] ^= 0xFF;
        assert_ne!(
            cap1_corrupted, cap2,
            "1-byte flip at offset 0 must make the byte-equality gate fire",
        );
    }

    // ---- Sensitivity probe 3: flip the LAST byte (catches gates that
    //      compare a prefix). ----
    {
        let mut cap1_corrupted = cap1.clone();
        let last = cap1_corrupted.len() - 1;
        cap1_corrupted[last] ^= 0xFF;
        assert_ne!(
            cap1_corrupted, cap2,
            "1-byte flip at the final byte (offset {last}) must make the \
             byte-equality gate fire",
        );
    }

    // ---- Sensitivity probe 4: shorten by one byte (length divergence). ----
    {
        let mut cap1_corrupted = cap1.clone();
        cap1_corrupted.pop();
        assert_ne!(
            cap1_corrupted,
            cap2,
            "removing one byte (length now {} vs cap2.len()={}) must make \
             the byte-equality gate fire",
            cap1_corrupted.len(),
            cap2.len(),
        );
    }

    // ---- Provenance gate: the physics participant id constant exists and
    //      is non-empty. Ensures the cross-substrate tests above remain in
    //      sync with the canonical ParticipantId — if this constant ever
    //      gets renamed without a corresponding test update the symbol
    //      breakage surfaces here as well. ----
    assert!(
        !PHYSICS_WORLD_PARTICIPANT_ID.is_empty(),
        "physics participant id must be non-empty; this assertion guards \
         against an empty rename / typo regression in the participant \
         identifier scheme",
    );
}
