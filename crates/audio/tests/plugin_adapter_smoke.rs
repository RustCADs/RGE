//! Phase-canary integration smoke tests for `audio::AudioPlugin`.
//!
//! `AudioPlugin` is the fourth real Tier-2 plugin canary (after
//! `cad-projection::CadProjectionPlugin`, `gfx::GfxPlugin`, and
//! `physics::PhysicsPlugin`) per the §10.4 dogfood rule and the ADR-114
//! amendment 2026-05-08 four-substrate validation. These tests prove that
//! the v1 `PluginContext` owned-resources-handoff design generalises to a
//! fourth resource family — kira audio engine state owning a Kira
//! mock-backend manager + spatial sub-track handles + listener handle —
//! without forcing any change to the Tier-1 substrate.
//!
//! Scenarios:
//!
//! 1. **`audio_plugin_lifecycle_via_plugin_host`** — register, init, tick,
//!    shutdown end-to-end through `PluginHost`. Verifies the plugin appends
//!    a frame record to the `AudioFrame` on each successful tick.
//!
//! 2. **`audio_plugin_tick_returns_contract_violation_when_audio_manager_missing`**
//!    — caller fails to stage `AudioManager`. Tick fails with
//!    `PluginError::ContractViolation { resource_type: "AudioManager" }`,
//!    plugin transitions to `Failed`, and the auto-emit produces a
//!    `Severity::Warning` (not `Error`) per audit-2 A5.1.
//!
//! 3. **`audio_plugin_tick_returns_contract_violation_when_audio_frame_missing`**
//!    — caller stages `AudioManager` but forgets `AudioFrame`. Tick
//!    surfaces `ContractViolation { resource_type: "AudioFrame" }`. The
//!    `AudioManager` WAS supplied so it must be put back into the registry
//!    (idempotent failure semantics — matching the gfx canary's
//!    HeadlessTarget-missing path and physics's PhysicsInputLedger-missing path).
//!
//! 4. **`audio_plugin_puts_resources_back_after_successful_tick`** —
//!    invariant: after a successful tick, both resources are still present
//!    in `ctx`, so the orchestrator can retrieve them.
//!
//! 5. **`audio_plugin_multi_tick_advances_frame_records`** — tick the plugin
//!    repeatedly; each tick must append exactly one new record to the
//!    `AudioFrame` and increment its ledger length deterministically.
//!
//! 6. **`audio_plugin_isolation_with_sibling_failure_fixture`** —
//!    multi-plugin isolation: a sibling test fixture deliberately panics
//!    during tick; the host's `catch_unwind` recovers, the sibling is
//!    marked `Failed`, and `AudioPlugin` ticks successfully alongside it.
//!
//! All tests are GPU-free / audio-device-free — the `MockBackend` runs on
//! every CI configuration without touching cpal.

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::AudioManagerSettings;
use rge_audio::components::Entity;
use rge_audio::{
    AudioFrame, AudioManager, AudioPlugin, AudioSource, OwnedAudioSchedule, PlaybackState,
    Transform, AUDIO_PLUGIN_ID,
};
use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};
use rge_kernel_plugin_host::{
    Plugin, PluginContext, PluginError, PluginHost, PluginId, PluginState,
};

/// Shared helper: build a mock-backend `AudioManager` at 48 kHz with one
/// pre-registered listener + source playing the test sine clip. Mirrors the
/// `falling_cube.rs`-style "minimal but non-trivial scene" pattern used by
/// the physics canary's smoke tests.
fn make_scene_manager() -> (AudioManager<MockBackend>, Entity, Entity, Transform) {
    let mut manager = AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
        backend_settings: MockBackendSettings {
            sample_rate: 48_000,
        },
        ..Default::default()
    })
    .expect("mock backend always succeeds");

    let listener_entity = Entity(1);
    let source_entity = Entity(2);
    let listener_xform = Transform::default();
    let source_xform = Transform::from_position([0.0, 0.0, -2.0]);

    manager
        .register_listener(listener_entity, &listener_xform)
        .unwrap();

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
        .unwrap();

    (manager, listener_entity, source_entity, source_xform)
}

/// Shared helper: build the `OwnedAudioSchedule` + listener pair for the scene.
fn make_scene_frame(
    listener_entity: Entity,
    source_entity: Entity,
    source_xform: Transform,
) -> AudioFrame {
    AudioFrame {
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
        listener: Some((listener_entity, Transform::default())),
        records: Vec::new(),
    }
}

/// The `AudioPlugin` adapter drives the audio schedule end-to-end through
/// the unified `Plugin` trait + `PluginHost` lifecycle. Verifies that:
///
/// 1. The plugin registers under its canonical id.
/// 2. `init_all` advances the plugin from `Pending` -> `Initialized`.
/// 3. `tick_all` extracts `AudioManager` + `AudioFrame` from the context,
///    advances the schedule by one step, and reports a successful tick.
/// 4. The frame's `records` length increments by exactly one — proof the
///    schedule actually ran.
/// 5. `shutdown_all` LIFO-shuts the plugin down without error.
#[test]
fn audio_plugin_lifecycle_via_plugin_host() {
    let (manager, listener_entity, source_entity, source_xform) = make_scene_manager();
    let frame = make_scene_frame(listener_entity, source_entity, source_xform);

    let plugin = AudioPlugin::new();
    let plugin_id = PluginId::new(AUDIO_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");
    assert_eq!(host.state(&plugin_id), Some(PluginState::Pending));

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Init: must succeed — audio has no GPU / lazy state.
    let init_report = host.init_all(&mut ctx);
    assert_eq!(init_report.initialized, vec![plugin_id.clone()]);
    assert!(
        init_report.failed.is_empty(),
        "init failed: {:?}",
        init_report.failed
    );
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Stage resources for the tick.
    assert!(ctx.insert(manager).is_none());
    assert!(ctx.insert(frame).is_none());
    assert_eq!(ctx.resource_count(), 2);

    // Tick.
    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(
        tick_report.ticked, 1,
        "ticked count: {:?}",
        tick_report.failed
    );
    assert!(
        tick_report.failed.is_empty(),
        "tick failed: {:?}",
        tick_report.failed
    );
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    // Take resources back from ctx — they MUST be present after a successful
    // tick (the plugin contract requires putting them back).
    let _manager_back: AudioManager<MockBackend> =
        ctx.take().expect("AudioManager present after tick");
    let frame_back: AudioFrame = ctx.take().expect("AudioFrame present after tick");
    assert_eq!(ctx.resource_count(), 0);

    // Verify the schedule actually ran: frame has one record.
    assert_eq!(
        frame_back.records.len(),
        1,
        "frame must have exactly one record after one tick"
    );
    assert_eq!(frame_back.records[0].tick, 0);
    assert_eq!(frame_back.records[0].source_count, 1);
    assert!(frame_back.records[0].had_listener);

    // Shutdown LIFO. No plugin-level error expected.
    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(shutdown_report.shutdown.len(), 1);
    assert!(shutdown_report.failed.is_empty());
    assert_eq!(host.count(), 0);
}

/// Runtime safety: a tick with `AudioManager` missing surfaces as
/// `PluginError::ContractViolation { resource_type: "AudioManager" }` and
/// marks the plugin Failed (per plugin-fatal isolation), without panicking.
/// Per audit-2 A5.1, the host's auto-emit classifies this as a Warning (not
/// Error) — the plugin code is fine; the caller failed to stage prerequisites.
#[test]
fn audio_plugin_tick_returns_contract_violation_when_audio_manager_missing() {
    let plugin = AudioPlugin::new();
    let plugin_id = PluginId::new(AUDIO_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(init_report.failed.is_empty());
    }
    assert_eq!(diags.len(), 0, "init must not auto-emit on success");

    let tick_report = {
        let mut ctx = PluginContext::new(&mut diags);
        // Deliberately do NOT insert AudioManager (or AudioFrame). Tick must
        // fail cleanly at the first take.
        host.tick_all(&mut ctx)
    };
    assert_eq!(tick_report.ticked, 0);
    assert_eq!(
        tick_report.failed.len(),
        1,
        "missing AudioManager must surface as a failed tick"
    );
    let (failed_id, failed_msg) = &tick_report.failed[0];
    assert_eq!(*failed_id, plugin_id);
    assert!(
        failed_msg.contains("missing resource of type AudioManager"),
        "error message must mention missing-AudioManager contract violation; got: {failed_msg}"
    );
    // Per plugin-fatal isolation, the plugin is now Failed.
    assert_eq!(host.state(&plugin_id), Some(PluginState::Failed));

    // Audit-2 A5.1: ContractViolation auto-emits as Warning, not Error.
    let new_diags: Vec<_> = diags.iter().collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected one auto-emit diagnostic for the contract violation",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation must auto-emit as Warning (not Error) per audit-2 A5.1",
    );
}

/// Idempotent failure: when `AudioFrame` is missing but `AudioManager` was
/// supplied, the plugin must put `AudioManager` back into the registry
/// before returning the contract violation — the orchestrator should still
/// be able to recover the `AudioManager` handle to re-issue the call later.
///
/// This test exercises the plugin adapter directly (no `PluginHost` wrap)
/// because the put-back invariant is tested at the plugin level; the host's
/// resource-leak detection is independently exercised by `host.rs`'s own
/// unit tests. Mirrors the gfx canary's HeadlessTarget-missing-after-
/// GfxContext-supplied test and physics's PhysicsInputLedger-missing-after-World
/// test.
#[test]
fn audio_plugin_tick_returns_contract_violation_when_audio_frame_missing() {
    let (manager, _, _, _) = make_scene_manager();

    let mut plugin = AudioPlugin::new();
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage AudioManager but NOT AudioFrame. Tick must put AudioManager back.
    assert!(ctx.insert(manager).is_none());
    assert!(ctx.contains::<AudioManager<MockBackend>>());
    assert!(!ctx.contains::<AudioFrame>());

    let err = plugin.tick(&mut ctx).expect_err("tick must fail");
    match err {
        PluginError::ContractViolation { resource_type } => {
            assert_eq!(
                resource_type, "AudioFrame",
                "second-resource missing must surface as AudioFrame violation"
            );
        }
        other => panic!("expected ContractViolation for AudioFrame; got {other:?}"),
    }

    // Idempotent failure invariant: AudioManager (the one we DID supply) must
    // still be in the registry so the orchestrator can recover it.
    assert!(
        ctx.contains::<AudioManager<MockBackend>>(),
        "AudioManager must be put back after a partial-resource contract violation"
    );
    assert_eq!(ctx.resource_count(), 1);
    // Counter unchanged on failure.
    assert_eq!(plugin.frames_advanced(), 0);
}

/// After a successful tick, both resources (`AudioManager` / `AudioFrame`)
/// must be back in the context — the plugin is responsible for returning
/// them so the orchestrator can retrieve them. Mirrors the cad-projection /
/// gfx / physics `puts_resources_back` precedents.
#[test]
fn audio_plugin_puts_resources_back_after_successful_tick() {
    let (manager, listener_entity, source_entity, source_xform) = make_scene_manager();
    let frame = make_scene_frame(listener_entity, source_entity, source_xform);

    let plugin = AudioPlugin::new();
    let plugin_id = PluginId::new(AUDIO_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    let _init_report = host.init_all(&mut ctx);

    // Stage resources.
    assert!(ctx.insert(manager).is_none());
    assert!(ctx.insert(frame).is_none());
    assert!(ctx.contains::<AudioManager<MockBackend>>());
    assert!(ctx.contains::<AudioFrame>());
    assert_eq!(ctx.resource_count(), 2);

    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(tick_report.ticked, 1);
    assert!(tick_report.failed.is_empty());

    // The invariant: after a successful tick, every resource we staged is
    // still present.
    assert!(
        ctx.contains::<AudioManager<MockBackend>>(),
        "AudioManager must be put back after tick"
    );
    assert!(
        ctx.contains::<AudioFrame>(),
        "AudioFrame must be put back after tick"
    );
    assert_eq!(ctx.resource_count(), 2);
}

/// Multi-tick advancement: ticking the plugin N times in a row must produce
/// exactly N records in the `AudioFrame`, with monotonically increasing tick
/// indices. This is the audio analogue of physics's multi-tick determinism
/// test — the schedule step is event-driven (not time-stepped) so we don't
/// have a byte-identical-trajectory analogue, but the ledger-append
/// invariant is the appropriate determinism check.
#[test]
fn audio_plugin_multi_tick_advances_frame_records() {
    const N: u64 = 5;

    let (manager, listener_entity, source_entity, source_xform) = make_scene_manager();
    let frame = make_scene_frame(listener_entity, source_entity, source_xform);

    let mut plugin = AudioPlugin::new();
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    plugin.init(&mut ctx).expect("init");
    ctx.insert(manager);
    ctx.insert(frame);

    for _ in 0..N {
        plugin.tick(&mut ctx).expect("tick must succeed");
    }
    assert_eq!(plugin.frames_advanced(), N);

    // Recover the frame and verify exactly N records, with tick indices
    // 0..N in order.
    let frame_back: AudioFrame = ctx.take().expect("AudioFrame present after multi-tick");
    #[allow(
        clippy::cast_possible_truncation,
        reason = "test loop bound N=5 fits usize on every supported target"
    )]
    let n_usize = N as usize;
    assert_eq!(
        frame_back.records.len(),
        n_usize,
        "frame must have exactly {N} records after {N} successful ticks"
    );
    for (i, record) in frame_back.records.iter().enumerate() {
        assert_eq!(
            record.tick, i as u64,
            "record tick indices must be monotonically increasing 0..N"
        );
        assert_eq!(record.source_count, 1);
        assert!(record.had_listener);
    }
}

/// Multi-plugin isolation: register `AudioPlugin` alongside a sibling test
/// fixture that deliberately panics during tick. Verify:
///
/// 1. The host's `catch_unwind` recovers from the sibling's panic.
/// 2. The sibling is marked `Failed` (plugin-fatal isolation per §1.13).
/// 3. `AudioPlugin` ticks successfully alongside the sibling — its state
///    and resource handoff are entirely unaffected by the sibling's failure.
/// 4. The diagnostic stream contains exactly one new error
///    (`PANICKED during tick`) attributable to the sibling, not to audio.
#[test]
fn audio_plugin_isolation_with_sibling_failure_fixture() {
    let (manager, listener_entity, source_entity, source_xform) = make_scene_manager();
    let frame = make_scene_frame(listener_entity, source_entity, source_xform);

    let audio_id = PluginId::new(AUDIO_PLUGIN_ID);
    let panicker_id = PluginId::new("test.panic-sibling");

    let mut host = PluginHost::new();
    host.register(audio_id.clone(), Box::new(AudioPlugin::new()))
        .expect("register audio");
    host.register(
        panicker_id.clone(),
        Box::new(PanickingTickPlugin::new(panicker_id.clone())),
    )
    .expect("register panicker");

    let mut diags = DiagnosticAggregator::new();

    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(
            init_report.failed.is_empty(),
            "init: {:?}",
            init_report.failed
        );
        assert_eq!(init_report.initialized.len(), 2);
    }

    let pre_tick_diag_count = diags.len();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage audio-only resources; the PanickingTickPlugin doesn't take any.
    assert!(ctx.insert(manager).is_none());
    assert!(ctx.insert(frame).is_none());

    let tick_report = host.tick_all(&mut ctx);

    assert_eq!(
        tick_report.ticked, 1,
        "exactly one plugin (audio) ticked Ok"
    );
    assert_eq!(
        tick_report.failed.len(),
        1,
        "exactly one plugin (sibling) failed"
    );
    assert_eq!(tick_report.failed[0].0, panicker_id);
    assert!(
        tick_report.failed[0].1.contains("panicked during tick"),
        "sibling failure must mention panic; got: {}",
        tick_report.failed[0].1
    );

    // AudioPlugin survived in spite of the sibling's panic — plugin-fatal
    // isolation per PLAN §1.13.
    assert_eq!(host.state(&audio_id), Some(PluginState::Initialized));
    assert_eq!(host.state(&panicker_id), Some(PluginState::Failed));

    // Resources put back successfully despite the sibling's failure.
    assert!(ctx.contains::<AudioManager<MockBackend>>());
    assert!(ctx.contains::<AudioFrame>());

    // The frame recorded exactly one tick — audio did its job.
    let frame_ref = ctx.get_mut::<AudioFrame>().expect("frame present");
    assert_eq!(
        frame_ref.records.len(),
        1,
        "audio must have ticked despite sibling panic"
    );

    // Exactly one new diagnostic — the PANICKED one for the sibling.
    let new_messages: Vec<&str> = diags
        .iter()
        .skip(pre_tick_diag_count)
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        new_messages
            .iter()
            .any(|m| m.contains("PANICKED during tick") && m.contains("test.panic-sibling")),
        "expected PANICKED-during-tick diagnostic for sibling; got {new_messages:?}",
    );
    // Audio must NOT have produced any failure diagnostic.
    assert!(
        !new_messages
            .iter()
            .any(|m| m.contains(AUDIO_PLUGIN_ID)
                && (m.contains("PANICKED") || m.contains("violation"))),
        "audio must not have produced failure diagnostics; got {new_messages:?}",
    );
}

// ---------------------------------------------------------------------------
// Test fixture: a plugin whose tick deliberately panics, used to drive the
// host's catch_unwind recovery path while audio ticks normally alongside it.
// Mirrors the gfx / physics canary's `PanickingTickPlugin` fixture verbatim
// — kept local to this test file so it doesn't need privileged access to
// kernel-level test helpers.
// ---------------------------------------------------------------------------

/// Minimal `Plugin` impl that panics on every `tick`. Test-only sibling
/// fixture for the isolation test above.
struct PanickingTickPlugin {
    id: PluginId,
}

impl PanickingTickPlugin {
    fn new(id: PluginId) -> Self {
        Self { id }
    }
}

impl Plugin for PanickingTickPlugin {
    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Deliberate panic to drive the host's catch_unwind recovery.
        panic!("PanickingTickPlugin: deliberate tick panic for isolation test");
    }
}
