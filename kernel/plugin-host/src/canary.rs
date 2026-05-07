//! `CanaryPlugin` — the §10.4 dogfood-rule canary protocol.
//!
//! Per [ADR-116](../../../docs/adr/ADR-116-canary-protocol.md). Companion to
//! [`crate::Plugin`]. Extends `Plugin` (super-trait) with a uniform
//! telemetry-accessor method that the four existing §10.4 dogfood-rule
//! canaries (cad-projection / gfx / physics / audio) already expose under
//! divergent inherent names (`ticks_run` / `frames_recorded` / `steps_run` /
//! `frames_advanced`).
//!
//! # The protocol in one paragraph
//!
//! Every Tier-2 plugin canary impl'ing the §10.4 dogfood rule SHOULD impl
//! [`CanaryPlugin`] to expose a uniform telemetry-accessor surface that
//! future tooling (replay diagnostics / observability dashboards / editor
//! sandbox / hot-reload telemetry) can consume through a single
//! `&dyn CanaryPlugin` reference, regardless of which concrete canary type
//! is behind the trait object.
//!
//! # The increment-only-on-success invariant
//!
//! [`CanaryPlugin::successful_ticks`] MUST return a counter that increments
//! exactly when [`Plugin::tick`](crate::Plugin::tick) returns `Ok(_)`.
//! [`PluginError::ContractViolation`](crate::PluginError::ContractViolation)
//! / [`PluginError::RuntimeFault`](crate::PluginError::RuntimeFault) /
//! [`PluginError::Panic`](crate::PluginError::Panic) paths MUST NOT
//! increment. Codified per the 2026-05-10 H5 audit closure
//! (see `change.md` 2026-05-10 05:35 entry); binding for any future canary.
//!
//! Without this invariant a counter that quietly increments on every tick
//! (success and failure alike) silently corrupts replay determinism, distributed
//! orchestration synchronization, and editor-runtime telemetry parity. Codifying
//! it at the trait level pre-emptively closes those failure modes for every
//! current and future canary impl.
//!
//! # Object-safety / dyn-safety
//!
//! [`CanaryPlugin`] is object-safe: `&dyn CanaryPlugin` and
//! `Box<dyn CanaryPlugin>` are both legal. The `Plugin` super-trait is
//! itself object-safe, so super-trait coercion (`&dyn CanaryPlugin`
//! → `&dyn Plugin`) works. Tooling that needs to walk a heterogeneous
//! canary set (`Vec<Box<dyn CanaryPlugin>>`) is therefore expressible.
//!
//! Object-safety is load-bearing for the cross-canary tooling use cases
//! enumerated in ADR-116; see the ADR's Sub-decision 3 for the full
//! rationale.
//!
//! # NOT in this module
//!
//! - Structured `CanaryTelemetry` type (lifecycle markers / replay markers /
//!   health states). Deferred per ADR-116 Sub-decision 1 until canary
//!   diversity surfaces additional shared shapes.
//! - Auto-registration of canaries. Deferred until kernel/types reflection
//!   stabilizes.
//! - Architecture-lint enforcing every canary impls this trait. Doc-comment-
//!   canonical per ADR-104; lint deferred until first violation surfaces.

use crate::plugin::Plugin;

/// The §10.4 dogfood-rule canary protocol per ADR-116.
///
/// Tier-2 plugin canaries impl this trait to expose the standardized
/// telemetry surface that future tooling (replay diagnostics, observability
/// dashboards, editor sandbox, hot-reload telemetry) can consume uniformly.
///
/// # The increment-only-on-success invariant
///
/// Implementations of [`successful_ticks`](CanaryPlugin::successful_ticks)
/// MUST return a counter that increments exactly when
/// [`Plugin::tick`](crate::Plugin::tick) returns `Ok(_)`.
/// [`PluginError::ContractViolation`](crate::PluginError::ContractViolation)
/// / [`PluginError::RuntimeFault`](crate::PluginError::RuntimeFault) /
/// [`PluginError::Panic`](crate::PluginError::Panic) paths MUST NOT
/// increment. This is the canonical "increment-only-on-success" semantics
/// codified by the 2026-05-10 H5 audit closure and is a binding contract
/// for any future canary impl.
///
/// # Object-safety
///
/// This trait is object-safe. `&dyn CanaryPlugin` and `Box<dyn CanaryPlugin>`
/// are both legal. Super-trait coercion to `&dyn Plugin` is automatic.
///
/// # Example
///
/// Every concrete canary delegates the trait method to its existing inherent
/// telemetry accessor (per ADR-116 Sub-decision 2 — backwards-compat policy):
///
/// ```ignore
/// // crates/cad-projection/src/plugin_adapter.rs
/// use rge_kernel_plugin_host::CanaryPlugin;
///
/// impl CanaryPlugin for CadProjectionPlugin {
///     fn successful_ticks(&self) -> u64 {
///         self.ticks_run()
///     }
/// }
/// ```
pub trait CanaryPlugin: Plugin {
    /// Number of `Plugin::tick()` calls that returned `Ok(_)`.
    ///
    /// MUST increment exactly when `tick()` returns `Ok(_)`.
    /// `ContractViolation` / `RuntimeFault` / `Panic` paths MUST NOT
    /// increment the counter.
    fn successful_ticks(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PluginContext;
    use crate::plugin::{PluginError, PluginId};

    /// Minimal in-module canary used to prove dyn-safety + super-trait
    /// composition WITHOUT pulling in any of the four real Tier-2 canaries
    /// (which would create a Tier-2 → Tier-1 dependency cycle and trip the
    /// `kernel-isolation` architecture lint).
    struct MockCanary {
        ticks: u64,
    }

    impl Plugin for MockCanary {
        fn id(&self) -> PluginId {
            PluginId::new("rge.canary.mock")
        }

        fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            Ok(())
        }
    }

    impl CanaryPlugin for MockCanary {
        fn successful_ticks(&self) -> u64 {
            self.ticks
        }
    }

    /// ADR-116 Sub-decision 3 acceptance: the trait is object-safe.
    ///
    /// If this test compiles, `&dyn CanaryPlugin` and `Box<dyn CanaryPlugin>`
    /// are both legal — load-bearing for future cross-canary tooling that
    /// stores a heterogeneous canary set behind dynamic dispatch.
    #[test]
    fn canary_plugin_is_dyn_safe() {
        let mock = MockCanary { ticks: 7 };
        // Trait-object reference compiles: dyn-safety property holds.
        let dyn_ref: &dyn CanaryPlugin = &mock;
        assert_eq!(dyn_ref.successful_ticks(), 7);

        // Boxed trait object compiles: dyn-safe + the trait inherits the
        // `Send + 'static` bound from `Plugin` so the box satisfies the
        // canonical `Box<dyn Plugin>` storage shape.
        let boxed: Box<dyn CanaryPlugin> = Box::new(MockCanary { ticks: 42 });
        assert_eq!(boxed.successful_ticks(), 42);
    }

    /// ADR-116 Sub-decision 3 acceptance: super-trait composition holds.
    ///
    /// `CanaryPlugin: Plugin` (super-trait) means `&dyn CanaryPlugin` is
    /// implicitly usable as `&dyn Plugin`. Verifies the coercion at runtime
    /// (the assignment is a compile-time check; the call through the
    /// `&dyn Plugin` view is a runtime check that the dyn vtable resolves).
    #[test]
    fn canary_plugin_extends_plugin() {
        let mock = MockCanary { ticks: 0 };
        let canary_ref: &dyn CanaryPlugin = &mock;
        // Super-trait coercion: &dyn CanaryPlugin → &dyn Plugin.
        // If this compiles, the super-trait relationship holds.
        let plugin_ref: &dyn Plugin = canary_ref;
        assert_eq!(plugin_ref.id().as_str(), "rge.canary.mock");
    }
}
