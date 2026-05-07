//! `gfx::GfxPlugin` — second real Tier-2 plugin canary per the §10.4
//! dogfood rule.
//!
//! Wraps a single-frame headless render workflow and impls
//! [`rge_kernel_plugin_host::Plugin`]. `tick` extracts `&mut GfxContext` and
//! `&mut HeadlessTarget` from the [`PluginContext`], records one render pass
//! that draws the canonical red triangle on the supplied target, submits to
//! the queue, and puts the resources back into the context. Demonstrates
//! that the v1 owned-handoff resource-registry generalizes beyond
//! cad-projection (which deals only in `Send + 'static` plain-Rust types) to
//! GPU resources owning `wgpu::Device` / `wgpu::Texture` handles.
//!
//! # Why this exists
//!
//! Closes the gfx-canary follow-up tracked in ADR-114. The first Tier-2
//! canary ([`rge_cad_projection::CadProjectionPlugin`]) shipped with the
//! 2026-05-07 audit-1 CRITICAL #2 substrate; this adapter validates that the
//! same `PluginContext` design holds for an entirely different resource
//! family (GPU resources vs ECS world / CAD graph / tolerance scalar) without
//! requiring any change to the kernel substrate.
//!
//! The take/insert pattern this module repeats verbatim across all 5 canaries
//! (cad-projection / gfx / physics / audio / editor-ui) is intentional per
//! PLAN §10.4 dogfood rule — see [`rge_cad_projection::plugin_adapter`]'s
//! `# Why this looks duplicated across the five canaries` section for the
//! canonical rationale.
//!
//! # Resource contract
//!
//! On `tick`, the plugin context MUST contain (caller-supplied):
//!
//! * [`GfxContext`] — owned device + queue handle. Borrowed `&` while
//!   recording; not mutated, but transferred ownership-style to satisfy the
//!   `Box<dyn Any + Send>` registry constraint.
//! * [`HeadlessTarget`] — the destination texture for the rendered frame.
//!   Borrowed `&` while recording.
//!
//! Missing either resource surfaces as
//! [`PluginError::ContractViolation`] (caller-supplied resource missing —
//! NOT a plugin-side bug; auto-emit downgrades to a warning per audit-2
//! A5.1). Pipeline build / queue-submission errors surface as
//! [`PluginError::RuntimeFault`] — the plugin code itself misbehaved or the
//! GPU rejected the work. In every error path the resources that WERE
//! supplied are put back into the context before the error propagates
//! (idempotent failure semantics, matching the cad-projection precedent).
//!
//! # Send + 'static bound
//!
//! Per the kernel substrate's `Box<dyn Any + Send>` registry: every
//! resource a plugin extracts MUST be `Send + 'static`. wgpu 29's
//! [`wgpu::Device`] / [`wgpu::Queue`] / [`wgpu::Texture`] / etc. are all
//! `Send + Sync`, so [`GfxContext`] and [`HeadlessTarget`] satisfy the bound
//! without further wrapping. This is a key data point for ADR-114: the
//! design generalizes cleanly to GPU resources without forcing a `Mutex`
//! wrapper or a non-Send compromise.

use rge_kernel_plugin_host::{CanaryPlugin, Plugin, PluginContext, PluginError, PluginId};

use crate::context::GfxContext;
use crate::frame::FrameRecorder;
use crate::pipeline::{PipelineError, TrianglePipeline};
use crate::target::HeadlessTarget;

/// Map a [`PipelineError`] into the [`PluginError::RuntimeFault`] variant the
/// plugin's `tick_inner` surfaces when [`TrianglePipeline::new`] fails.
///
/// Extracted as a `pub(crate)` helper (audit-2 deep-audit-2 round-2 closure)
/// so the unit test can call the SAME mapping the production `tick_inner`
/// uses — without it, the test would synthesize the mapping inline and pass
/// even if `tick_inner` switched to a different `PluginError` variant
/// (tautological-test regression).
///
/// The wire format `"GfxPlugin.tick: pipeline build failed: {err}"` is the
/// documented contract — see this module's `// Resource contract` doc and
/// the `tick_inner` doc-comment.
pub(crate) fn map_pipeline_err(err: &PipelineError) -> PluginError {
    PluginError::runtime_fault(format!("GfxPlugin.tick: pipeline build failed: {err}"))
}

/// Stable [`PluginId`] reported by every [`GfxPlugin`] instance.
pub const GFX_PLUGIN_ID: &str = "rge-gfx.headless-triangle-plugin";

/// Tier-2 plugin adapter that records one canonical headless triangle frame
/// per `tick` against a caller-supplied [`HeadlessTarget`].
///
/// The wrapped [`TrianglePipeline`] is built lazily on the first `tick`
/// because pipeline construction needs a live [`GfxContext`] (not available
/// at plugin construction time). Lazy-init means a fresh `GfxPlugin` can be
/// constructed without GPU presence; the GPU is touched only when the
/// orchestrator stages the resources for the first tick.
///
/// Exposes the canary's tick lifecycle through the unified [`Plugin`] trait
/// per PLAN §10.4 dogfood rule. The adapter is a thin shim: real GPU work
/// is delegated to [`FrameRecorder`] + [`TrianglePipeline`].
pub struct GfxPlugin {
    /// Clear colour applied at the start of each rendered frame.
    clear: wgpu::Color,
    /// Lazy-built render pipeline. `None` until the first `tick` triggers
    /// pipeline construction with the supplied target's format.
    pipeline: Option<TrianglePipeline>,
    /// Counts the number of successful frame submissions; useful for tests
    /// and as a basic liveness signal for the orchestrator.
    frames_recorded: u64,
}

impl std::fmt::Debug for GfxPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GfxPlugin")
            .field("clear", &self.clear)
            .field("pipeline_built", &self.pipeline.is_some())
            .field("frames_recorded", &self.frames_recorded)
            .finish()
    }
}

impl GfxPlugin {
    /// Build a plugin that clears to opaque black before drawing each frame.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clear: wgpu::Color::BLACK,
            pipeline: None,
            frames_recorded: 0,
        }
    }

    /// Build a plugin with a custom clear colour.
    #[must_use]
    pub fn with_clear(clear: wgpu::Color) -> Self {
        Self {
            clear,
            pipeline: None,
            frames_recorded: 0,
        }
    }

    /// The configured per-frame clear colour.
    #[must_use]
    pub fn clear_color(&self) -> wgpu::Color {
        self.clear
    }

    /// Number of frames the plugin has successfully submitted across all
    /// completed `tick`s. Increments only on the success path; failed ticks
    /// (contract violation, runtime fault) leave the counter unchanged.
    #[must_use]
    pub fn frames_recorded(&self) -> u64 {
        self.frames_recorded
    }

    /// `true` once the lazy [`TrianglePipeline`] has been built. Useful for
    /// tests asserting that the pipeline was constructed at the right
    /// lifecycle point.
    #[must_use]
    pub fn pipeline_built(&self) -> bool {
        self.pipeline.is_some()
    }
}

impl Default for GfxPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for GfxPlugin {
    fn id(&self) -> PluginId {
        PluginId::new(GFX_PLUGIN_ID)
    }

    fn name(&self) -> &'static str {
        "rge-gfx headless triangle canary"
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Construction already produced the empty initial state; the pipeline
        // is built lazily on first tick because it needs a real GfxContext
        // (which the orchestrator may stage AFTER init). Mirrors the
        // cad-projection precedent of an init that does no real work.
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Sequential takes — each `take` releases the borrow on `ctx`
        // immediately so the next `take` / `insert` is unhindered. If a
        // required resource is missing, restore whatever we already took
        // before erroring (idempotent failure semantics — the cad-projection
        // precedent).
        //
        // Missing-resource cases are CONTRACT violations (caller didn't
        // stage prerequisites) — distinct from RUNTIME faults coming out of
        // the render submission itself. The host's auto-emit downgrades
        // ContractViolation to a warning per audit-2 A5.1.
        let gfx_ctx = ctx
            .take::<GfxContext>()
            .ok_or_else(|| PluginError::contract_violation("GfxContext"))?;
        let Some(target) = ctx.take::<HeadlessTarget>() else {
            // Put GfxContext back before erroring out so the orchestrator
            // can recover its handle.
            let replaced = ctx.insert(gfx_ctx);
            debug_assert!(replaced.is_none(), "GfxContext slot was empty after take");
            return Err(PluginError::contract_violation("HeadlessTarget"));
        };

        // Lazy pipeline build. Pipeline format must match the target's
        // format, so we can't pre-build at plugin construction time without
        // knowing the target. If the build fails (it shouldn't with the
        // embedded WGSL — but the API is fallible for callers swapping in
        // custom shaders) surface as RuntimeFault and put resources back.
        let result = self.tick_inner(&gfx_ctx, &target);

        // Always put resources back, even on failure, so the orchestrator
        // can retrieve them. The plugin is responsible for not leaving the
        // ctx in a dirty state. Slots are empty after the takes above, so
        // insert returns None — no resource is dropped on the floor.
        debug_assert!(
            ctx.insert(gfx_ctx).is_none(),
            "GfxContext slot was empty after tick"
        );
        debug_assert!(
            ctx.insert(target).is_none(),
            "HeadlessTarget slot was empty after tick"
        );

        if result.is_ok() {
            self.frames_recorded += 1;
        }
        result
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No external resources held; pipeline + counter are dropped with
        // the plugin. wgpu RAII cleans up the GPU resources at drop. Mirrors
        // the cad-projection precedent of a default Ok(()) shutdown.
        Ok(())
    }
}

/// ADR-116 §10.4 dogfood-rule canary protocol impl. Delegates to the
/// inherent `frames_recorded` accessor; backwards-compat per ADR-116
/// Sub-decision 2.
impl CanaryPlugin for GfxPlugin {
    fn successful_ticks(&self) -> u64 {
        self.frames_recorded()
    }
}

impl GfxPlugin {
    /// Inner tick body — built out as a separate method so the resource
    /// put-back path (above) is straight-line.
    ///
    /// Lazy-builds the pipeline if needed, then records + submits one frame.
    /// Any pipeline-build failure surfaces as a [`PluginError::RuntimeFault`]
    /// — the plugin itself misbehaved (rather than the caller failing to
    /// stage a resource).
    fn tick_inner(
        &mut self,
        gfx_ctx: &GfxContext,
        target: &HeadlessTarget,
    ) -> Result<(), PluginError> {
        if self.pipeline.is_none() {
            let pipeline = TrianglePipeline::new(gfx_ctx, target.format())
                .map_err(|e| map_pipeline_err(&e))?;
            self.pipeline = Some(pipeline);
        }

        // Pipeline is now Some; the unwrap is safe.
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("pipeline was just inserted above");

        let mut frame = FrameRecorder::new(gfx_ctx);
        frame.render_triangle(target, pipeline, self.clear);
        frame.submit();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;

    #[test]
    fn gfx_plugin_id_matches_convention() {
        let plugin = GfxPlugin::new();
        assert_eq!(
            plugin.id(),
            PluginId::new("rge-gfx.headless-triangle-plugin")
        );
        assert_eq!(plugin.id().as_str(), GFX_PLUGIN_ID);
    }

    #[test]
    fn gfx_plugin_name_is_stable_human_readable_string() {
        let plugin = GfxPlugin::new();
        assert_eq!(plugin.name(), "rge-gfx headless triangle canary");
    }

    #[test]
    fn gfx_plugin_default_clear_is_opaque_black() {
        let plugin = GfxPlugin::new();
        let c = plugin.clear_color();
        assert!((c.r - 0.0).abs() < f64::EPSILON);
        assert!((c.g - 0.0).abs() < f64::EPSILON);
        assert!((c.b - 0.0).abs() < f64::EPSILON);
        assert!((c.a - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gfx_plugin_with_clear_overrides_default() {
        let custom = wgpu::Color {
            r: 0.25,
            g: 0.5,
            b: 0.75,
            a: 1.0,
        };
        let plugin = GfxPlugin::with_clear(custom);
        let got = plugin.clear_color();
        assert!((got.r - custom.r).abs() < f64::EPSILON);
        assert!((got.g - custom.g).abs() < f64::EPSILON);
        assert!((got.b - custom.b).abs() < f64::EPSILON);
        assert!((got.a - custom.a).abs() < f64::EPSILON);
    }

    #[test]
    fn gfx_plugin_default_impl_matches_new() {
        let from_default: GfxPlugin = GfxPlugin::default();
        // Both produce zero-state pipelines + zero frames + black clear.
        let from_new = GfxPlugin::new();
        assert_eq!(from_default.frames_recorded(), from_new.frames_recorded());
        assert_eq!(from_default.pipeline_built(), from_new.pipeline_built());
        assert!((from_default.clear_color().r - from_new.clear_color().r).abs() < f64::EPSILON);
    }

    #[test]
    fn gfx_plugin_init_succeeds_without_resources() {
        let mut plugin = GfxPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        // No resources inserted; init should still succeed (it's a no-op
        // because the pipeline is built lazily on first tick — the
        // cad-projection precedent).
        assert!(plugin.init(&mut ctx).is_ok());
        // Init must not have inserted anything either.
        assert_eq!(ctx.resource_count(), 0);
        // And it must NOT have built the pipeline (that requires GfxContext
        // staging from the orchestrator on the first tick).
        assert!(!plugin.pipeline_built());
    }

    #[test]
    fn gfx_plugin_tick_with_no_resources_returns_contract_violation_for_gfx_context() {
        let mut plugin = GfxPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        let err = plugin.tick(&mut ctx).expect_err("tick must fail");
        match err {
            PluginError::ContractViolation { resource_type } => {
                assert_eq!(resource_type, "GfxContext");
            }
            other => panic!("expected ContractViolation for GfxContext; got {other:?}"),
        }
        // Counter unchanged on failure.
        assert_eq!(plugin.frames_recorded(), 0);
        // And the pipeline was never reached for build.
        assert!(!plugin.pipeline_built());
    }

    #[test]
    fn gfx_plugin_shutdown_succeeds_without_resources() {
        let mut plugin = GfxPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        assert!(plugin.shutdown(&mut ctx).is_ok());
    }

    #[test]
    fn gfx_plugin_frames_recorded_starts_at_zero() {
        let plugin = GfxPlugin::new();
        assert_eq!(plugin.frames_recorded(), 0);
    }

    /// Audit-2 closure (deep audit 2026-05-09 round 2): `tick_inner`'s
    /// doc-comment (see the `tick_inner` function in this file) promises
    /// "any pipeline-build failure surfaces as [`PluginError::RuntimeFault`]".
    /// Today, the only fallible call site in `tick_inner` is
    /// [`crate::pipeline::TrianglePipeline::new`], which is
    /// `#[allow(clippy::unnecessary_wraps)]` because the embedded WGSL never
    /// fails to compile. The fallible API surface is preserved for future
    /// callers who substitute custom WGSL — see `pipeline.rs:73`.
    ///
    /// **Approach (a)** test-quality fix: end-to-end firing of the
    /// [`PluginError::RuntimeFault`] path is out of scope without
    /// fault-injection plumbing, so we extract the mapping logic into a
    /// `pub(crate) fn map_pipeline_err` that BOTH the production `tick_inner`
    /// AND this test call. If a future refactor changes `tick_inner`'s
    /// `map_err(|e| map_pipeline_err(&e))` to a different variant
    /// (e.g. `PluginError::contract_violation(...)`), it must change
    /// `map_pipeline_err` too — and THIS test fails. The prior version of
    /// this test was tautological: it constructed a `PluginError::runtime_fault`
    /// directly using the same string formula it asserted, so it would still
    /// pass even if `tick_inner` switched variants.
    ///
    /// When a future dispatch lands the fault-injection path, this test
    /// should be supplemented with an end-to-end variant that actually
    /// drives `tick_all` against a plugin that fails pipeline build.
    #[test]
    fn gfx_plugin_runtime_fault_on_pipeline_build_failure() {
        use crate::pipeline::PipelineError;

        // Synthesize a PipelineError::Wgsl as if pipeline build had failed
        // (e.g. a future custom-WGSL caller passed invalid shader text).
        let synthetic_pipeline_err = PipelineError::Wgsl(
            "synthetic shader compile failure: invalid attribute @synthetic".to_string(),
        );
        let synthetic_msg = synthetic_pipeline_err.to_string();

        // Call the SAME `map_pipeline_err` helper `tick_inner` uses (no
        // longer reconstructs the formula inline — that was the
        // tautological-test bug audit-2 round-2 flagged at this site).
        let plugin_err = map_pipeline_err(&synthetic_pipeline_err);

        // The mapping must produce a RuntimeFault variant (NOT
        // ContractViolation, NOT InitFailed) — RuntimeFault auto-emits as
        // Severity::Error, the right severity for "the plugin code itself
        // misbehaved", per the doc-comment at `plugin_adapter.rs:34-39`.
        match &plugin_err {
            PluginError::RuntimeFault { reason } => {
                assert!(
                    reason.starts_with("GfxPlugin.tick: pipeline build failed:"),
                    "mapping must use the documented prefix; got: {reason}",
                );
                assert!(
                    reason.contains(&synthetic_msg),
                    "mapping must propagate the underlying PipelineError text; got: {reason}",
                );
            }
            other => panic!(
                "pipeline-build failures must map to RuntimeFault, got {other:?} \
                 — if this fails, `tick_inner`'s `.map_err(...)` (or the shared \
                 `map_pipeline_err` helper) regressed",
            ),
        }

        // Defensive cross-check: the auto-emit Display impl matches the
        // expected "plugin runtime fault: ..." wire format (used by the
        // host's auto-emit + by the structured-diagnostic stream).
        let display_str = plugin_err.to_string();
        assert!(
            display_str.starts_with("plugin runtime fault:"),
            "RuntimeFault Display must use 'plugin runtime fault:' prefix; got: {display_str}",
        );
    }

    /// ADR-116 acceptance: `GfxPlugin` impls the `CanaryPlugin` protocol.
    /// Trait method delegates to the existing inherent `frames_recorded`
    /// accessor; calling through `&dyn CanaryPlugin` exercises the
    /// dynamic-dispatch path future cross-canary tooling will use.
    #[test]
    fn gfx_plugin_impls_canary_protocol() {
        let plugin = GfxPlugin::new();
        let canary: &dyn CanaryPlugin = &plugin;
        assert_eq!(canary.successful_ticks(), 0);
        assert_eq!(canary.successful_ticks(), plugin.frames_recorded());
    }
}
