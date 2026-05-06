//! `audio::AudioPlugin` — fourth real Tier-2 plugin canary per the §10.4
//! dogfood rule.
//!
//! Wraps a single `audio_schedule_step` call and impls
//! [`rge_kernel_plugin_host::Plugin`]. `tick` extracts an owned
//! [`AudioManager<MockBackend>`] and [`AudioFrame`] from the [`PluginContext`],
//! advances the audio engine by exactly one schedule step against the frame's
//! source list, records that one frame advanced, and puts both resources back
//! into the context. Demonstrates that the v1 owned-handoff resource-registry
//! generalises beyond cad-projection's plain-Rust types, gfx's wgpu device
//! handles, and physics's rapier3d arenas to a fourth resource family — kira
//! audio engine state owning a Kira mock-backend manager + spatial sub-track
//! handles + listener handle.
//!
//! # Why this exists
//!
//! Closes the audio-canary follow-up tracked in ADR-114's amendment 2026-05-08
//! (three-substrate validation). The first three Tier-2 canaries
//! ([`rge_cad_projection::CadProjectionPlugin`], [`rge_gfx::GfxPlugin`],
//! [`rge_physics::PhysicsPlugin`]) shipped with the same Tier-1 substrate; this
//! adapter validates that the same `PluginContext` design holds for an
//! entirely different resource family — Kira's audio engine state with its
//! cpal-style backend assumption — without requiring any change to the kernel
//! substrate.
//!
//! # Resource contract
//!
//! On `tick`, the plugin context MUST contain (caller-supplied):
//!
//! * [`AudioManager<MockBackend>`] — owned `&mut` after `take`; mutated by
//!   the schedule step (per-source state reconciliation, listener pose
//!   propagation, command issue to the underlying Kira mixer). The canary
//!   pins the backend type to [`MockBackend`] so all tests are GPU-free /
//!   audio-device-free; the production path
//!   ([`AudioManager<DefaultBackend>`]) is wired separately by the orchestrator
//!   when it stages a real backend.
//! * [`AudioFrame`] — owned `&mut` after `take`; appended to (one
//!   [`FrameRecord`] per scheduled step) so test harnesses can verify the
//!   plugin actually advanced its mixer per call.
//!
//! Missing either resource surfaces as
//! [`PluginError::ContractViolation`] (caller-supplied resource missing —
//! NOT a plugin-side bug; auto-emit downgrades to a warning per audit-2
//! A5.1). Per the cad-projection / gfx / physics precedent, in every error
//! path the resources that WERE supplied are put back into the context
//! before the error propagates (idempotent failure semantics).
//!
//! Inner-work failure ([`ManagerError::UnknownClip`] from a missing clip key)
//! surfaces as [`PluginError::RuntimeFault`] — the plugin code (or the staged
//! frame data) misbehaved; this is distinct from a missing-resource contract
//! violation. Resources are still put back along the runtime-fault path.
//! Unlike physics's no-`RuntimeFault` subcase, audio's `audio_schedule_step`
//! is fallible at the call boundary, so this canary exercises the
//! `RuntimeFault` mapping, mirroring the gfx canary's
//! pipeline-build-failure path.
//!
//! # Send + 'static bound
//!
//! Per the kernel substrate's `Box<dyn Any + Send>` registry: every resource
//! a plugin extracts MUST be `Send + 'static`. Verified empirically by
//! direct `assert_send::<T>()` tests in this module:
//!
//! * [`kira::AudioManager<MockBackend>`] is `Send + 'static`. The
//!   [`crate::manager::AudioManager`] wrapper passes the bound through. The
//!   underlying mock-backend renderer holds no platform handles; the
//!   production [`DefaultBackend`] (cpal) shape is also `Send` because
//!   Kira's wrapper surfaces a `Send`-safe API even though the underlying
//!   `cpal::Stream` is `!Send` on Windows — Kira keeps the platform handle
//!   on a backend-owned thread and routes commands through a `Send` channel.
//!   This is the canonical "audio backend wrapper makes the engine Send"
//!   pattern that ADR-114's amendment 2026-05-08 anticipates.
//! * [`AudioFrame`] is `Send + 'static` by construction — every field is
//!   plain owned data ([`Vec`] / [`u64`] / owned [`String`] / [`f32`]).
//!
//! Both resources satisfy the bound without any `Mutex`, `Arc`, or `unsafe`
//! wrapping. This is the fourth-substrate confirmation for ADR-114 amendment
//! 2026-05-08: the design generalises cleanly from CAD-graph + GPU + physics
//! resource families to audio engine state — Kira's wrapper around the
//! cpal-style backend ensures the public surface is `Send`-safe, so the
//! plugin canary requires no special handling. The amendment's anticipated
//! "cpal-style RAII handles need a wrapper" finding is realised in Kira
//! itself — the wrapper layer is the standard pattern for cpal-backed audio
//! engines, NOT a plugin-host concern.
//!
//! [`MockBackend`]: kira::backend::mock::MockBackend
//! [`DefaultBackend`]: kira::backend::DefaultBackend

use kira::backend::mock::MockBackend;
use rge_kernel_plugin_host::{Plugin, PluginContext, PluginError, PluginId};

use crate::components::{AudioSource, Entity, Transform};
use crate::manager::AudioManager;
use crate::schedule::{audio_schedule_step, AudioSchedule};

/// Stable [`PluginId`] reported by every [`AudioPlugin`] instance.
pub const AUDIO_PLUGIN_ID: &str = "rge-audio.scheduling-plugin";

/// One scheduled source entry — owned data so it can sit inside an
/// [`AudioFrame`] which travels through the `Box<dyn Any + Send>` registry.
///
/// Mirrors [`AudioSchedule<'a>`] but carries owned [`Transform`] + owned
/// [`AudioSource`] instead of borrowed references; we materialise the
/// borrowed view at the start of [`AudioPlugin::tick`] so the schedule call
/// sees the same shape it would see from a non-plugin caller.
#[derive(Debug, Clone)]
pub struct OwnedAudioSchedule {
    /// Entity owning the [`AudioSource`].
    pub entity: Entity,
    /// World pose for the entity — drives Kira emitter position.
    pub transform: Transform,
    /// Component data — playback intent, volume, pitch, looped, falloff.
    pub source: AudioSource,
}

/// Per-tick audit ledger for the audio canary. Mirrors the shape of
/// [`rge_physics::physics_input_ledger::PhysicsInputLedger`]: an append-only
/// log of frame records plus a running tick counter. Carries the per-frame
/// schedule inputs (sources + optional listener) for the orchestrator-side
/// test harness.
///
/// Send + 'static by construction (every field is plain owned data).
#[derive(Debug, Default, Clone)]
pub struct AudioFrame {
    /// Owned per-source schedule entries the plugin walks at each tick. The
    /// orchestrator sets this before staging the frame; the plugin reads it
    /// inside `tick` to materialise the borrowed-slice view that
    /// [`audio_schedule_step`] expects.
    pub sources: Vec<OwnedAudioSchedule>,
    /// Optional active listener for the frame — plugin reads `(entity,
    /// &transform)` and forwards it to [`audio_schedule_step`] verbatim.
    pub listener: Option<(Entity, Transform)>,
    /// Append-only record of frames advanced. One entry per successful tick.
    pub records: Vec<FrameRecord>,
}

impl AudioFrame {
    /// Construct an empty frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience: number of recorded frame advancements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no frames have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Single-tick record appended to [`AudioFrame::records`] on every successful
/// schedule step. Mirrors the physics canary's
/// [`rge_physics::physics_input_ledger::TickRecord`] shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRecord {
    /// Monotonic tick index assigned by the plugin (zero-based, increments
    /// once per successful tick).
    pub tick: u64,
    /// Number of sources that participated in the tick.
    pub source_count: usize,
    /// `true` if the frame had an active listener configured.
    pub had_listener: bool,
}

/// Tier-2 plugin adapter that drives the audio engine forward by exactly one
/// schedule step per `tick` against a caller-supplied
/// [`AudioManager<MockBackend>`] + [`AudioFrame`].
///
/// Exposes the canary's tick lifecycle through the unified [`Plugin`] trait
/// per PLAN §10.4 dogfood rule. The adapter is a thin shim: real schedule
/// reconciliation is delegated to [`audio_schedule_step`]. The adapter's job
/// is to (1) extract resources from the [`PluginContext`], (2) drive the
/// step, (3) record the advancement, and (4) put the resources back so the
/// orchestrator can retrieve them.
///
/// Mirrors the physics canary pattern: zero state besides the per-tick
/// liveness counter; no GPU / wgpu surface; no lazy resource (the audio
/// engine state lives entirely inside the caller-supplied
/// [`AudioManager`], unlike gfx's `Option<TrianglePipeline>` which needs a
/// `GfxContext` to construct).
#[derive(Debug)]
pub struct AudioPlugin {
    /// Counts the number of successful schedule steps the plugin has driven.
    /// Useful for tests and as a basic liveness signal for the orchestrator.
    /// Increments only on the success path; failed ticks (contract violation
    /// or runtime fault) leave the counter unchanged — matching the
    /// cad-projection / gfx / physics canary precedents.
    frames_advanced: u64,
}

impl AudioPlugin {
    /// Build a fresh plugin with zero recorded schedule steps.
    #[must_use]
    pub fn new() -> Self {
        Self { frames_advanced: 0 }
    }

    /// Number of schedule steps successfully driven across all completed
    /// ticks. Increments only on the success path; failed ticks leave the
    /// counter unchanged.
    #[must_use]
    pub fn frames_advanced(&self) -> u64 {
        self.frames_advanced
    }
}

impl Default for AudioPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AudioPlugin {
    fn id(&self) -> PluginId {
        PluginId::new(AUDIO_PLUGIN_ID)
    }

    fn name(&self) -> &'static str {
        "rge-audio scheduling canary"
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Construction already produced the zero-state counter; audio has no
        // GPU / pipeline / lazy-init machinery (the AudioManager is caller-
        // staged, not plugin-built). Mirrors the cad-projection + gfx +
        // physics precedent of an init that does no real work.
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Sequential takes — each `take` releases the borrow on `ctx`
        // immediately so the next `take` / `insert` is unhindered. If a
        // required resource is missing, restore whatever we already took
        // before erroring (idempotent failure semantics — the cad-projection
        // / gfx / physics precedent).
        //
        // Missing-resource cases are CONTRACT violations (caller didn't stage
        // prerequisites) — distinct from RUNTIME faults coming out of the
        // schedule step itself. The host's auto-emit downgrades
        // ContractViolation to a warning per audit-2 A5.1.
        let mut manager = ctx
            .take::<AudioManager<MockBackend>>()
            .ok_or_else(|| PluginError::contract_violation("AudioManager"))?;
        let Some(mut frame) = ctx.take::<AudioFrame>() else {
            // Put AudioManager back before erroring out so the orchestrator
            // can recover its handle — same shape as gfx's HeadlessTarget /
            // physics's PhysicsInputLedger missing-after-first-resource-supplied
            // path.
            let replaced = ctx.insert(manager);
            debug_assert!(replaced.is_none(), "AudioManager slot was empty after take");
            return Err(PluginError::contract_violation("AudioFrame"));
        };

        // Materialise borrowed-slice view from owned schedule entries.
        // `audio_schedule_step` expects `&[AudioSchedule<'_>]` referencing
        // the frame's owned data; build it on the stack so the references
        // live for the duration of the call.
        let scheds: Vec<AudioSchedule<'_>> = frame
            .sources
            .iter()
            .map(|owned| AudioSchedule {
                entity: owned.entity,
                transform: &owned.transform,
                source: &owned.source,
            })
            .collect();
        let listener_ref = frame
            .listener
            .as_ref()
            .map(|(entity, transform)| (*entity, transform));

        // audio_schedule_step is fallible (returns
        // Result<(), ManagerError>): an unknown clip key surfaces as
        // ManagerError::UnknownClip. Map to PluginError::RuntimeFault so the
        // host's auto-emit elevates to Diagnostic::Error.
        let outcome = audio_schedule_step(&mut manager, &scheds, listener_ref);

        // If the step succeeded, append a record to the frame's audit log
        // BEFORE we put it back into the registry. This way the orchestrator
        // sees the new record on the next take.
        let succeeded = outcome.is_ok();
        if succeeded {
            frame.records.push(FrameRecord {
                tick: self.frames_advanced,
                source_count: scheds.len(),
                had_listener: listener_ref.is_some(),
            });
        }

        // Drop the borrowed-slice view explicitly so the subsequent `insert`
        // can move `frame` without aliasing.
        drop(scheds);

        // Always put resources back, even on failure, so the orchestrator
        // can retrieve them. Slots are empty after the takes above, so
        // insert returns None — no resource is dropped on the floor.
        debug_assert!(
            ctx.insert(manager).is_none(),
            "AudioManager slot was empty after tick"
        );
        debug_assert!(
            ctx.insert(frame).is_none(),
            "AudioFrame slot was empty after tick"
        );

        match outcome {
            Ok(()) => {
                self.frames_advanced += 1;
                Ok(())
            }
            Err(err) => Err(PluginError::runtime_fault(format!(
                "audio_schedule_step failed: {err}"
            ))),
        }
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No external resources held; the AudioManager + AudioFrame are
        // caller-owned and remain in the registry for the orchestrator to
        // retrieve. Kira's manager has its own RAII cleanup at drop. Mirrors
        // the cad-projection + gfx + physics precedent of a default Ok(())
        // shutdown.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kira::backend::mock::MockBackendSettings;
    use kira::AudioManagerSettings;
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;
    use crate::source::PlaybackState;
    use crate::waveform::sine_wave;

    fn mock_manager() -> AudioManager<MockBackend> {
        AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
            backend_settings: MockBackendSettings {
                sample_rate: 48_000,
            },
            ..Default::default()
        })
        .expect("mock backend always succeeds")
    }

    /// Send + 'static probe — the central data point for ADR-114's fourth
    /// substrate check. This test is the load-bearing claim documented in
    /// the module header: every resource the plugin extracts MUST be
    /// `Send + 'static` for the `Box<dyn Any + Send>` registry to accept it.
    #[test]
    fn audio_manager_and_audio_frame_are_send_static() {
        fn assert_send_static<T: Send + 'static>() {}
        assert_send_static::<AudioManager<MockBackend>>();
        assert_send_static::<AudioFrame>();
    }

    #[test]
    fn audio_plugin_id_matches_convention() {
        let plugin = AudioPlugin::new();
        assert_eq!(plugin.id(), PluginId::new("rge-audio.scheduling-plugin"));
        assert_eq!(plugin.id().as_str(), AUDIO_PLUGIN_ID);
    }

    #[test]
    fn audio_plugin_name_is_stable_human_readable_string() {
        let plugin = AudioPlugin::new();
        assert_eq!(plugin.name(), "rge-audio scheduling canary");
    }

    #[test]
    fn audio_plugin_frames_advanced_starts_at_zero() {
        let plugin = AudioPlugin::new();
        assert_eq!(plugin.frames_advanced(), 0);
    }

    #[test]
    fn audio_plugin_default_impl_matches_new() {
        let from_default: AudioPlugin = AudioPlugin::default();
        let from_new = AudioPlugin::new();
        assert_eq!(from_default.frames_advanced(), from_new.frames_advanced());
    }

    #[test]
    fn audio_frame_default_is_empty() {
        let frame = AudioFrame::new();
        assert!(frame.is_empty());
        assert_eq!(frame.len(), 0);
        assert!(frame.sources.is_empty());
        assert!(frame.listener.is_none());
    }

    #[test]
    fn audio_plugin_init_succeeds_without_resources() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        // No resources inserted; init should still succeed (it's a no-op —
        // the cad-projection / gfx / physics precedent).
        assert!(plugin.init(&mut ctx).is_ok());
        // Init must not have inserted anything either.
        assert_eq!(ctx.resource_count(), 0);
        // And it must NOT have advanced any state — the counter is unchanged.
        assert_eq!(plugin.frames_advanced(), 0);
    }

    #[test]
    fn audio_plugin_tick_with_no_resources_returns_contract_violation_for_audio_manager() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        let err = plugin.tick(&mut ctx).expect_err("tick must fail");
        match err {
            PluginError::ContractViolation { resource_type } => {
                assert_eq!(resource_type, "AudioManager");
            }
            other => panic!("expected ContractViolation for AudioManager; got {other:?}"),
        }
        // Counter unchanged on failure.
        assert_eq!(plugin.frames_advanced(), 0);
        // No resources were left behind in the registry.
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn audio_plugin_tick_with_manager_only_returns_contract_violation_for_audio_frame() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage AudioManager but NOT AudioFrame. Tick must put AudioManager back.
        assert!(ctx.insert(mock_manager()).is_none());
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

        // Idempotent failure invariant: AudioManager (the one we DID supply)
        // must still be in the registry so the orchestrator can recover it.
        // Mirrors the gfx canary's HeadlessTarget-missing-after-GfxContext
        // and physics's PhysicsInputLedger-missing-after-World precedents.
        assert!(
            ctx.contains::<AudioManager<MockBackend>>(),
            "AudioManager must be put back after a partial-resource contract violation"
        );
        assert_eq!(ctx.resource_count(), 1);
        // Counter unchanged on failure.
        assert_eq!(plugin.frames_advanced(), 0);
    }

    #[test]
    fn audio_plugin_tick_advances_frame_when_both_supplied() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage both required resources with an empty frame (no sources / no
        // listener — the schedule is a no-op but still succeeds).
        assert!(ctx.insert(mock_manager()).is_none());
        assert!(ctx.insert(AudioFrame::new()).is_none());
        assert_eq!(ctx.resource_count(), 2);

        // Tick once.
        plugin.tick(&mut ctx).expect("tick must succeed");
        assert_eq!(plugin.frames_advanced(), 1);

        // Recover the resources from the registry; verify the frame recorded
        // exactly one entry.
        let _manager_back: AudioManager<MockBackend> =
            ctx.take().expect("AudioManager still present");
        let frame_back: AudioFrame = ctx.take().expect("AudioFrame still present");
        assert_eq!(
            frame_back.records.len(),
            1,
            "frame must have recorded exactly one schedule step"
        );
        assert_eq!(frame_back.records[0].tick, 0);
        assert_eq!(frame_back.records[0].source_count, 0);
        assert!(!frame_back.records[0].had_listener);
    }

    #[test]
    fn audio_plugin_tick_records_source_count_and_listener_presence() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage manager with a registered listener + source so the schedule
        // has real work to do.
        let mut manager = mock_manager();
        let listener_entity = Entity(1);
        let source_entity = Entity(2);
        let listener_xform = Transform::default();
        let source_xform = Transform::from_position([0.0, 0.0, -2.0]);
        manager
            .register_listener(listener_entity, &listener_xform)
            .unwrap();
        let samples = sine_wave(440.0, 48_000, 0.05);
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

        let frame = AudioFrame {
            sources: vec![OwnedAudioSchedule {
                entity: source_entity,
                transform: source_xform,
                source: source.clone(),
            }],
            listener: Some((listener_entity, listener_xform)),
            records: Vec::new(),
        };

        ctx.insert(manager);
        ctx.insert(frame);

        plugin.tick(&mut ctx).expect("tick must succeed");

        let frame_back: AudioFrame = ctx.take().expect("AudioFrame still present");
        assert_eq!(frame_back.records.len(), 1);
        assert_eq!(frame_back.records[0].source_count, 1);
        assert!(frame_back.records[0].had_listener);
    }

    #[test]
    fn audio_plugin_tick_with_unknown_clip_surfaces_runtime_fault() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Manager registers a source pointing at "missing" clip but the clip
        // is never registered — schedule step surfaces UnknownClip. The
        // plugin must map this to RuntimeFault.
        let mut manager = mock_manager();
        let source_entity = Entity(7);
        let xform = Transform::default();
        let source = AudioSource {
            clip: "missing".into(),
            desired_state: PlaybackState::Playing,
            ..AudioSource::default()
        };
        manager
            .register_source(source_entity, &xform, &source)
            .unwrap();

        let frame = AudioFrame {
            sources: vec![OwnedAudioSchedule {
                entity: source_entity,
                transform: xform,
                source: source.clone(),
            }],
            listener: None,
            records: Vec::new(),
        };

        ctx.insert(manager);
        ctx.insert(frame);

        let err = plugin.tick(&mut ctx).expect_err("tick must fail");
        match err {
            PluginError::RuntimeFault { reason } => {
                assert!(
                    reason.contains("audio_schedule_step failed") && reason.contains("missing"),
                    "RuntimeFault must mention the schedule failure + clip key; got: {reason}"
                );
            }
            other => panic!("expected RuntimeFault for unknown clip; got {other:?}"),
        }

        // Resources put back even on RuntimeFault; counter unchanged; no
        // record appended (only the success path appends).
        assert!(ctx.contains::<AudioManager<MockBackend>>());
        assert!(ctx.contains::<AudioFrame>());
        assert_eq!(plugin.frames_advanced(), 0);
        let frame_back: AudioFrame = ctx.take().unwrap();
        assert!(
            frame_back.records.is_empty(),
            "RuntimeFault path must not append a frame record"
        );
    }

    #[test]
    fn audio_plugin_shutdown_succeeds_without_resources() {
        let mut plugin = AudioPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        assert!(plugin.shutdown(&mut ctx).is_ok());
    }
}
