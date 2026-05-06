//! Plugin trait + identity + error type.
//!
//! Per PLAN.md §10.4 (dogfood rule): "Tier 2 uses the same `Plugin` trait as
//! Tier 3." This module defines that trait. Tier-2 subsystems (gfx, physics,
//! editor-ui, cad-projection) implement [`Plugin`] exactly like Tier-3
//! sandboxed WASM plugins; Tier-3 sandboxing (capability gating, WASM
//! isolation) is layered on top by future `runtime-wasmtime` × `plugin-host`
//! integration and is NOT this trait's concern.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::context::PluginContext;

/// Stable identifier for a plugin.
///
/// String-based for cross-version identity stability — same convention as
/// [`rge_kernel_ecs::participate::ParticipantId`]. Convention:
/// `"<vendor>.<name>"` (Tier-3) or `"<crate-name>"` (Tier-2). Must be unique
/// within a [`crate::PluginHost`]'s plugin set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PluginId(pub String);

impl PluginId {
    /// Construct a [`PluginId`] from any `Into<String>`.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for PluginId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for PluginId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Which lifecycle phase a panic occurred in.
///
/// Carried inside [`PluginError::Panic`] so the host can attribute the panic
/// to the right call site. Display impl matches the lower-case method name
/// (`init`, `tick`, `shutdown`) so auto-emit messages read naturally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginPhase {
    /// Panic happened inside [`Plugin::init`].
    Init,
    /// Panic happened inside [`Plugin::tick`].
    Tick,
    /// Panic happened inside [`Plugin::shutdown`].
    Shutdown,
}

impl std::fmt::Display for PluginPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginPhase::Init => f.write_str("init"),
            PluginPhase::Tick => f.write_str("tick"),
            PluginPhase::Shutdown => f.write_str("shutdown"),
        }
    }
}

/// Errors produced by [`Plugin`] implementations during lifecycle calls.
///
/// Each variant maps to a lifecycle-stage failure that the host treats per
/// PLAN.md §1.13 plugin-fatal isolation: the plugin is marked failed, but
/// other plugins continue and the kernel keeps running.
///
/// # Taxonomy (post-2026-05-08 audit-2 / Pairing 3 / N1 finding A5.1)
///
/// The orchestrator distinguishes:
///
/// * [`PluginError::InitFailed`] / [`PluginError::ShutdownFailed`] /
///   [`PluginError::RuntimeFault`] — the plugin returned `Err` from inside
///   its own logic; classified as **plugin-fatal errors**.
/// * [`PluginError::ContractViolation`] — a required resource was absent
///   from [`PluginContext`] when the plugin needed it; classified as a
///   **caller-side warning** (not a plugin bug). Auto-emit downgrades to
///   [`rge_kernel_diagnostics::Severity::Warning`].
/// * [`PluginError::Panic`] — the plugin panicked; the host caught it via
///   `catch_unwind` and recovered. Resources held by the panicking plugin
///   are unrecoverable; the host detects + reports any leak separately.
///   Constructed only by the host, never via a public constructor.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Plugin's [`Plugin::init`] could not complete (resource unavailable,
    /// dependency missing, validation failed, etc.).
    #[error("plugin init failed: {reason}")]
    InitFailed {
        /// Human-readable reason for the init failure.
        reason: String,
    },

    /// Plugin's [`Plugin::shutdown`] could not complete cleanly.
    ///
    /// The host treats the plugin as shut down anyway (plugin-fatal isolation
    /// per PLAN.md §1.13), but surfaces this so callers can route a diagnostic.
    #[error("plugin shutdown failed: {reason}")]
    ShutdownFailed {
        /// Human-readable reason for the shutdown failure.
        reason: String,
    },

    /// Plugin signaled a generic runtime fault (typically from
    /// [`Plugin::tick`]). Distinct from contract violations and panics.
    #[error("plugin runtime fault: {reason}")]
    RuntimeFault {
        /// Human-readable reason for the runtime fault.
        reason: String,
    },

    /// A required resource was absent from [`PluginContext`] when the plugin
    /// needed it. This is a CONTRACT issue between plugin and orchestrator
    /// (caller responsibility), distinct from a runtime fault.
    ///
    /// Auto-emit classifies these as warnings, not errors: the plugin code
    /// itself is fine; the caller failed to stage the prerequisites.
    #[error("plugin contract violation: missing resource of type {resource_type}")]
    ContractViolation {
        /// Static name of the missing resource type (e.g. `"World"`,
        /// `"CadGraph"`). Static so the variant doesn't need allocation
        /// and can be matched on at zero cost.
        resource_type: &'static str,
    },

    /// The plugin panicked during a lifecycle method; the orchestrator
    /// caught the panic via `catch_unwind` and recovered. The host emits a
    /// [`rge_kernel_diagnostics::Diagnostic::error`] with the PANIC payload
    /// and marks the plugin [`crate::PluginState::Failed`]. Resources held
    /// by the panicking plugin are unrecoverable; the orchestrator detects
    /// + reports any resource leak separately.
    ///
    /// Constructed by the host only — there is no public constructor for
    /// this variant because plugins should not synthesize "I panicked"
    /// errors from inside their own code (a real panic produces this; a
    /// soft fault should use [`PluginError::RuntimeFault`]).
    #[error("plugin panicked during {phase}: {payload}")]
    Panic {
        /// Which lifecycle phase the panic occurred in.
        phase: PluginPhase,
        /// Best-effort string from the panic payload (extracted by the host
        /// via `Any::downcast_ref::<String>` / `&'static str`).
        payload: String,
    },
}

impl PluginError {
    /// Construct a [`PluginError::InitFailed`] from any `Into<String>`.
    #[must_use]
    pub fn init(reason: impl Into<String>) -> Self {
        Self::InitFailed {
            reason: reason.into(),
        }
    }

    /// Construct a [`PluginError::ShutdownFailed`] from any `Into<String>`.
    #[must_use]
    pub fn shutdown(reason: impl Into<String>) -> Self {
        Self::ShutdownFailed {
            reason: reason.into(),
        }
    }

    /// Construct a [`PluginError::RuntimeFault`] from any `Into<String>`.
    #[must_use]
    pub fn runtime_fault(reason: impl Into<String>) -> Self {
        Self::RuntimeFault {
            reason: reason.into(),
        }
    }

    /// Construct a [`PluginError::ContractViolation`] for a missing resource
    /// of the given static type name.
    #[must_use]
    pub fn contract_violation(resource_type: &'static str) -> Self {
        Self::ContractViolation { resource_type }
    }
}

/// The `Plugin` trait — the contract every Tier-2 / Tier-3 plugin implements.
///
/// Per PLAN.md §10.4 dogfood rule: Tier-2 subsystems and Tier-3 sandboxed
/// WASM plugins implement this same trait. The host calls lifecycle methods
/// in a fixed order: registration → [`init`](Plugin::init) (once) → zero or
/// more [`tick`](Plugin::tick) calls → [`shutdown`](Plugin::shutdown) (once).
///
/// Implementations must be `Send + 'static` so the host can store them as
/// `Box<dyn Plugin>`.
pub trait Plugin: Send + 'static {
    /// Stable identifier for this plugin instance.
    ///
    /// Must match the [`PluginId`] under which it is registered with
    /// [`crate::PluginHost::register`]; the host validates this and rejects
    /// mismatches.
    fn id(&self) -> PluginId;

    /// Human-readable display name (may include version string).
    ///
    /// The default is `""` because the trait cannot return a borrow tied to a
    /// freshly-allocated [`PluginId`] without a lifetime; implementations
    /// override when they want a custom display name.
    fn name(&self) -> &'static str {
        ""
    }

    /// One-shot init. Called exactly once after registration, before any
    /// [`tick`](Plugin::tick) or [`shutdown`](Plugin::shutdown).
    ///
    /// The plugin may interact with the supplied [`PluginContext`] (emit
    /// diagnostics, etc.).
    ///
    /// # Errors
    ///
    /// Any [`PluginError`] returned here marks the plugin **failed**; the
    /// host will not call `tick` or `shutdown` on a failed plugin.
    fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>;

    /// Optional per-frame tick. Default no-op.
    ///
    /// # Errors
    ///
    /// Errors here mark the plugin failed (per failure-class plugin-fatal
    /// isolation) but the engine continues.
    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    /// One-shot shutdown. Called exactly once when the host shuts down OR
    /// when the plugin is unregistered. After this returns the plugin is
    /// dropped.
    ///
    /// # Errors
    ///
    /// Errors here are surfaced as diagnostics but do not block shutdown
    /// of other plugins (plugin-fatal isolation per PLAN.md §1.13).
    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_id_round_trips_through_serde() {
        let id = PluginId::new("rge.test");
        let encoded = ron::to_string(&id).expect("serialize");
        let back: PluginId = ron::from_str(&encoded).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn plugin_id_from_str_and_string_constructors() {
        let from_str: PluginId = "rge.from-str".into();
        let from_string: PluginId = String::from("rge.from-string").into();
        let from_new = PluginId::new("rge.from-new");

        assert_eq!(from_str.as_str(), "rge.from-str");
        assert_eq!(from_string.as_str(), "rge.from-string");
        assert_eq!(from_new.as_str(), "rge.from-new");
        assert_eq!(format!("{from_str}"), "rge.from-str");
    }

    #[test]
    fn plugin_error_constructors_set_correct_variant() {
        let init_err = PluginError::init("missing dep");
        let shutdown_err = PluginError::shutdown("flush failed");
        let runtime_err = PluginError::runtime_fault("transient I/O blip");
        let contract_err = PluginError::contract_violation("World");
        // Panic variant has no public constructor; build it directly to
        // round-trip the Display impl.
        let panic_err = PluginError::Panic {
            phase: PluginPhase::Tick,
            payload: "boom".to_string(),
        };

        assert!(
            matches!(init_err, PluginError::InitFailed { ref reason } if reason == "missing dep")
        );
        assert!(
            matches!(shutdown_err, PluginError::ShutdownFailed { ref reason } if reason == "flush failed")
        );
        assert!(
            matches!(runtime_err, PluginError::RuntimeFault { ref reason } if reason == "transient I/O blip")
        );
        assert!(
            matches!(contract_err, PluginError::ContractViolation { resource_type } if resource_type == "World")
        );
        assert!(
            matches!(panic_err, PluginError::Panic { phase, ref payload } if phase == PluginPhase::Tick && payload == "boom")
        );

        // Display impls go through thiserror's #[error("...")] templates.
        assert_eq!(init_err.to_string(), "plugin init failed: missing dep");
        assert_eq!(
            shutdown_err.to_string(),
            "plugin shutdown failed: flush failed"
        );
        assert_eq!(
            runtime_err.to_string(),
            "plugin runtime fault: transient I/O blip"
        );
        assert_eq!(
            contract_err.to_string(),
            "plugin contract violation: missing resource of type World"
        );
        assert_eq!(panic_err.to_string(), "plugin panicked during tick: boom");
    }

    #[test]
    fn plugin_error_panic_variant_displays_phase_and_payload() {
        for (phase, expected_phase_str) in [
            (PluginPhase::Init, "init"),
            (PluginPhase::Tick, "tick"),
            (PluginPhase::Shutdown, "shutdown"),
        ] {
            let err = PluginError::Panic {
                phase,
                payload: "plugin panicked!".to_string(),
            };
            let rendered = err.to_string();
            assert!(
                rendered.contains(expected_phase_str),
                "phase {phase:?} should render as {expected_phase_str:?}; got {rendered}",
            );
            assert!(
                rendered.contains("plugin panicked!"),
                "payload should appear in display; got {rendered}",
            );
        }
    }

    #[test]
    fn plugin_phase_display() {
        assert_eq!(PluginPhase::Init.to_string(), "init");
        assert_eq!(PluginPhase::Tick.to_string(), "tick");
        assert_eq!(PluginPhase::Shutdown.to_string(), "shutdown");
    }
}
