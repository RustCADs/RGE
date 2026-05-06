//! `rge-kernel-plugin-host` — Tier-1 plugin lifecycle substrate.
//!
//! Failure class: plugin-fatal
//!
//! Per PLAN.md §10.1 / §10.4 (dogfood rule): "Tier 2 uses the same `Plugin`
//! trait as Tier 3." This crate defines that [`Plugin`] trait + its lifecycle
//! ([`PluginHost`]) + the [`PluginContext`] exposed to plugins at init time.
//!
//! # Dogfood rule
//!
//! Tier-2 subsystems (gfx, physics, editor-ui, cad-projection) implement
//! [`Plugin`] exactly like Tier-3 sandboxed WASM plugins. Tier-3 sandboxing
//! (capability gating, WASM isolation) is NOT this crate's concern — that
//! belongs to a future `runtime-wasmtime` × `plugin-host` integration.
//!
//! # Failure class
//!
//! Plugin failures are **plugin-fatal** per PLAN.md §1.13: a plugin failing
//! during init / live / shutdown does not take down the kernel; the host marks
//! the plugin failed, surfaces a diagnostic, and the engine continues. The
//! host itself failing (rare; host invariant violation) is also plugin-fatal —
//! the engine continues without plugin support.
//!
//! # NOT in this dispatch
//!
//! - WASM plugin loading (Tier-3; needs `runtime-wasmtime` integration)
//! - Plugin discovery (`crates/plugin-discovery`)
//! - Capability manifest (`PLUGIN_API.md` companion doc; deferred)
//! - Hot-reload (separate substrate; `script-host` handles WASM hot-reload)
//! - Plugin dep resolution (PLAN §10.1 lists this; v0 ships lifecycle only)
//! - Action registration via plugins (PLAN §6.16; needs editor-actions integration)
//!
//! # Quick start
//!
//! ```rust
//! use rge_kernel_diagnostics::DiagnosticAggregator;
//! use rge_kernel_plugin_host::{
//!     Plugin, PluginContext, PluginError, PluginHost, PluginId,
//! };
//!
//! struct ExamplePlugin;
//! impl Plugin for ExamplePlugin {
//!     fn id(&self) -> PluginId { PluginId::new("example") }
//!     fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
//!         Ok(())
//!     }
//! }
//!
//! let mut diags = DiagnosticAggregator::new();
//! let mut ctx = PluginContext::new(&mut diags);
//! let mut host = PluginHost::new();
//! host.register(PluginId::new("example"), Box::new(ExamplePlugin)).unwrap();
//! host.init_all(&mut ctx);
//! host.shutdown_all(&mut ctx);
//! ```

#![forbid(unsafe_code)]

pub mod context;
pub mod host;
pub mod plugin;

pub use context::PluginContext;
pub use host::{
    InitReport, PluginHost, PluginHostError, PluginRecord, PluginState, ShutdownReport, TickReport,
};
pub use plugin::{Plugin, PluginError, PluginId, PluginPhase};
