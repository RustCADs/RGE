// SPLIT-EXEMPTION: implementation is ~670 lines; the rest is a unit-test
// suite (a TestPlugin behavior matrix + ~25 lifecycle tests covering
// register / init / tick / shutdown / unregister + the Phase 0 audit-2
// catch_unwind / leak-detection / contract-violation / panic-recovery
// closures). Splitting host.rs from its test module would force tests into
// a sibling file and lose `super::*` access to the private `TestPlugin`
// struct without a meaningful reduction in cohesion.
//! [`PluginHost`] — owns plugins and manages their lifecycle.
//!
//! The host enforces a strict state machine: every registered plugin is
//! `Pending` until [`init_all`](PluginHost::init_all) advances it to
//! `Initialized` (or `Failed`). Only `Initialized` plugins receive
//! [`tick_all`](PluginHost::tick_all) calls. [`shutdown_all`](PluginHost::shutdown_all)
//! drains the registry in reverse insertion order (LIFO), skipping
//! `Failed` plugins so a broken plugin's `shutdown` is never called twice.
//!
//! All lifecycle errors are isolated: one plugin's failure marks just that
//! plugin `Failed`, never propagates to others, and never takes down the
//! host (PLAN.md §1.13 plugin-fatal isolation).
//!
//! # Panic-safety (post-2026-05-08 audit-2 / Pairing 3 / N1 finding A5.1)
//!
//! Every direct call into a plugin's lifecycle method is wrapped in
//! [`std::panic::catch_unwind`]. The host treats plugins as **untrusted
//! execution domains** (per the kernel/userspace boundary equivalence in
//! the audit framing): a panic inside the plugin must not corrupt the
//! orchestrator's state. The wrapper:
//!
//! 1. Snapshots the resource-registry [`std::any::TypeId`] set BEFORE the call.
//! 2. Invokes the plugin via [`std::panic::AssertUnwindSafe`] (safe — the
//!    surrounding scope is the panic-recovery boundary).
//! 3. Snapshots again AFTER the call (regardless of outcome).
//! 4. Diffs the two sets to detect leaked resources (a plugin took a resource
//!    out of the context but didn't put it back).
//! 5. Maps the outcome to one of `Ok` / `Err(PluginError)` / panic-payload.
//!    Each path emits a structured diagnostic and updates the plugin's
//!    [`PluginState`].
//!
//! This is the line of defense against plugin-discipline lapses: a plugin
//! panicking mid-`tick` after taking the [`rge_kernel_ecs::World`] would,
//! before this hardening, permanently lose World from the orchestrator.
//!
//! # Auto-emit allocation cost (post-LOW #5)
//!
//! Each plugin lifecycle Err / Panic / leak path allocates a `String` for
//! the diagnostic message. At plugin-tick rate (~60Hz × N plugins) the cost
//! is negligible (well under 1µs per allocation on commodity hardware) and
//! only fires on the failure path — successful plugin calls emit nothing.
//! If high-throughput plugin-failure scenarios surface (e.g. a
//! continuously-misconfigured ctx hammering the auto-emit at 60Hz), a future
//! dispatch could add rate-limiting or a structured `Diagnostic::Code` enum
//! to dedupe. Today the simple String-format approach is sufficient.

use std::any::Any;
use std::collections::BTreeMap;

use thiserror::Error;

use crate::context::PluginContext;
use crate::plugin::{Plugin, PluginError, PluginId, PluginPhase};

/// Lifecycle state of a registered plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Registered, but [`Plugin::init`](crate::Plugin::init) has not yet been called.
    Pending,
    /// [`Plugin::init`](crate::Plugin::init) returned `Ok`.
    Initialized,
    /// One of the lifecycle calls returned an error. Plugin will not receive
    /// further [`tick`](crate::Plugin::tick) or [`shutdown`](crate::Plugin::shutdown) calls.
    Failed,
    /// [`Plugin::shutdown`](crate::Plugin::shutdown) is currently in progress.
    ShuttingDown,
    /// [`Plugin::shutdown`](crate::Plugin::shutdown) returned successfully.
    Shutdown,
}

/// One plugin held by the host.
///
/// Currently exposed as a read-only inspection target via
/// [`PluginHost::get`] (see audit-1 + audit-2 carryover note: the only
/// public consumer is reflective tooling — debug overlays, plugin
/// inspectors, future hot-reload integrations). Kept public despite no
/// in-tree consumer because the field shape is part of the host's
/// contract for those forward use cases.
pub struct PluginRecord {
    /// Current lifecycle state.
    pub state: PluginState,
    /// The plugin instance.
    pub plugin: Box<dyn Plugin>,
}

impl std::fmt::Debug for PluginRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRecord")
            .field("state", &self.state)
            .field(
                "plugin",
                &format_args!("Box<dyn Plugin>(id={})", self.plugin.id()),
            )
            .finish()
    }
}

/// Host-level errors (validation failures during register / unregister).
///
/// Plugin-emitted lifecycle errors flow through [`InitReport`] / [`TickReport`]
/// / [`ShutdownReport`] instead — those are not "host failed", just
/// "this plugin failed in isolation".
#[derive(Debug, Error)]
pub enum PluginHostError {
    /// A plugin was already registered with this id.
    #[error("plugin {id} already registered")]
    DuplicateId {
        /// The id that collided.
        id: PluginId,
    },
    /// No plugin is registered under this id.
    #[error("plugin {id} not found")]
    NotFound {
        /// The id that was not found.
        id: PluginId,
    },
    /// The plugin's [`Plugin::id`](crate::Plugin::id) returned a different
    /// value than the one passed to [`PluginHost::register`].
    #[error("plugin {id} id-mismatch: registered as {registered}, plugin reports {reported}")]
    IdMismatch {
        /// The id that was used as a registration key.
        id: PluginId,
        /// The id passed to `register`.
        registered: PluginId,
        /// The id reported by the plugin.
        reported: PluginId,
    },
    /// An invalid lifecycle transition was attempted (reserved for future
    /// fine-grained APIs).
    #[error("plugin {id} cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// The plugin id.
        id: PluginId,
        /// The current state.
        from: PluginState,
        /// The state requested.
        to: PluginState,
    },
}

/// Per-plugin lifecycle manager.
///
/// Plugins registered first init first; plugins registered first shutdown
/// last (LIFO). The registry is a [`BTreeMap`] for deterministic iteration
/// + lookup; insertion order is tracked separately in a [`Vec`].
pub struct PluginHost {
    plugins: BTreeMap<PluginId, PluginRecord>,
    /// Insertion order for deterministic init / LIFO shutdown.
    insertion_order: Vec<PluginId>,
}

impl PluginHost {
    /// Construct an empty host.
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugins: BTreeMap::new(),
            insertion_order: Vec::new(),
        }
    }

    /// Register a plugin.
    ///
    /// Validates that the plugin's [`Plugin::id`](crate::Plugin::id) matches
    /// `id`. The plugin starts in [`PluginState::Pending`];
    /// [`Plugin::init`](crate::Plugin::init) is NOT called until
    /// [`init_all`](PluginHost::init_all).
    ///
    /// # Errors
    ///
    /// * [`PluginHostError::DuplicateId`] if a plugin is already registered
    ///   under this id.
    /// * [`PluginHostError::IdMismatch`] if the plugin's `id()` differs from
    ///   the registered id.
    pub fn register(
        &mut self,
        id: PluginId,
        plugin: Box<dyn Plugin>,
    ) -> Result<(), PluginHostError> {
        if self.plugins.contains_key(&id) {
            return Err(PluginHostError::DuplicateId { id });
        }
        let reported = plugin.id();
        if reported != id {
            return Err(PluginHostError::IdMismatch {
                id: id.clone(),
                registered: id,
                reported,
            });
        }
        self.insertion_order.push(id.clone());
        self.plugins.insert(
            id,
            PluginRecord {
                state: PluginState::Pending,
                plugin,
            },
        );
        Ok(())
    }

    /// Unregister a plugin.
    ///
    /// If the plugin is in [`PluginState::Initialized`], its
    /// [`Plugin::shutdown`](crate::Plugin::shutdown) is called best-effort,
    /// wrapped in [`std::panic::catch_unwind`] and resource-leak-checked.
    ///
    /// Per the LOW #5 invariant ("the diagnostic stream is the single source
    /// of truth for plugin failures"), any shutdown error or panic is
    /// surfaced as a [`rge_kernel_diagnostics::Diagnostic::warning`] (NOT
    /// error — host-initiated unregister is non-fatal by design); resource
    /// leaks are also surfaced as warnings. The plugin is removed from the
    /// registry regardless of outcome.
    ///
    /// # Errors
    ///
    /// [`PluginHostError::NotFound`] if no plugin is registered under `id`.
    pub fn unregister(
        &mut self,
        id: &PluginId,
        ctx: &mut PluginContext<'_>,
    ) -> Result<(), PluginHostError> {
        let mut record = self
            .plugins
            .remove(id)
            .ok_or_else(|| PluginHostError::NotFound { id: id.clone() })?;
        self.insertion_order.retain(|i| i != id);
        if record.state == PluginState::Initialized {
            record.state = PluginState::ShuttingDown;

            // Snapshot resources before the shutdown call so we can detect
            // any leak afterwards regardless of outcome.
            let pre_call_resources = ctx.snapshot_resource_ids();

            // Wrap in catch_unwind: a host-initiated unregister-shutdown panic
            // must not corrupt the host (which is mid-iteration in the wider
            // orchestrator caller).
            let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                record.plugin.shutdown(ctx)
            }));

            let post_call_resources = ctx.snapshot_resource_ids();
            let leaked: Vec<_> = pre_call_resources
                .difference(&post_call_resources)
                .copied()
                .collect();

            match panic_result {
                Ok(Ok(())) => {
                    if !leaked.is_empty() {
                        // A "successful" shutdown that leaked resources is
                        // still non-fatal at the orchestrator level.
                        ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::warning(
                            format!(
                                "plugin {id} unregister-shutdown leaked {n} resource(s); orchestrator state may be incomplete",
                                n = leaked.len()
                            ),
                        ));
                    }
                }
                Ok(Err(plugin_err)) => {
                    // Shutdown returned an error. Warning, not error: the
                    // host explicitly invoked the unregister, so there's no
                    // "the engine is broken" semantic — we just couldn't
                    // tear the plugin down cleanly.
                    ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::warning(format!(
                        "plugin {id} unregister-shutdown failed: {plugin_err}"
                    )));
                    if !leaked.is_empty() {
                        ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::warning(
                            format!(
                                "plugin {id} unregister-shutdown leaked {n} resource(s); orchestrator state may be incomplete",
                                n = leaked.len()
                            ),
                        ));
                    }
                }
                Err(panic_payload) => {
                    let payload_str = panic_payload_to_string(&panic_payload);
                    ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::warning(format!(
                        "plugin {id} PANICKED during unregister-shutdown: {payload_str}"
                    )));
                    if !leaked.is_empty() {
                        ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::warning(
                            format!(
                                "plugin {id} unregister-shutdown leaked {n} resource(s) during panic; orchestrator state may be incomplete",
                                n = leaked.len()
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Initialize every [`Pending`](PluginState::Pending) plugin in
    /// registration order.
    ///
    /// Failures are isolated: one plugin's init failure marks it
    /// [`Failed`](PluginState::Failed) but other plugins still init.
    /// Panics raised by a plugin's `init` body are caught via
    /// [`std::panic::catch_unwind`]; the panicking plugin is marked
    /// [`Failed`](PluginState::Failed) and a structured diagnostic is
    /// emitted. Resources held by the panicking plugin during the panic
    /// are unrecoverable; the leak is reported separately.
    pub fn init_all(&mut self, ctx: &mut PluginContext<'_>) -> InitReport {
        let mut report = InitReport::default();
        for id in self.insertion_order.clone() {
            if let Some(record) = self.plugins.get_mut(&id) {
                if record.state != PluginState::Pending {
                    continue;
                }

                let pre_call_resources = ctx.snapshot_resource_ids();
                let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    record.plugin.init(ctx)
                }));
                let post_call_resources = ctx.snapshot_resource_ids();
                let leaked: Vec<_> = pre_call_resources
                    .difference(&post_call_resources)
                    .copied()
                    .collect();

                match panic_result {
                    Ok(Ok(())) => {
                        if leaked.is_empty() {
                            record.state = PluginState::Initialized;
                            report.initialized.push(id);
                        } else {
                            // Plugin returned Ok but leaked resources —
                            // disciplinary failure (orchestrator state
                            // incomplete; downstream plugins or callers may
                            // panic when they go to retrieve a missing
                            // resource).
                            let msg = format!(
                                "plugin {id} returned Ok but leaked {n} resource(s); orchestrator state may be incomplete",
                                n = leaked.len()
                            );
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                msg.clone(),
                            ));
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
                        }
                    }
                    Ok(Err(plugin_err)) => {
                        emit_plugin_err_diagnostic(ctx, &id, PluginPhase::Init, &plugin_err);
                        if !leaked.is_empty() {
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!(
                                    "plugin {id} leaked {n} resource(s) on init failure",
                                    n = leaked.len()
                                ),
                            ));
                        }
                        let msg = plugin_err.to_string();
                        record.state = PluginState::Failed;
                        report.failed.push((id, msg));
                    }
                    Err(panic_payload) => {
                        let payload_str = panic_payload_to_string(&panic_payload);
                        ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(format!(
                            "plugin {id} PANICKED during init: {payload_str}"
                        )));
                        if !leaked.is_empty() {
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!(
                                    "plugin {id} leaked {n} resource(s) during panic; orchestrator state may be incomplete",
                                    n = leaked.len()
                                ),
                            ));
                        }
                        let panic_err = PluginError::Panic {
                            phase: PluginPhase::Init,
                            payload: payload_str,
                        };
                        record.state = PluginState::Failed;
                        report.failed.push((id, panic_err.to_string()));
                    }
                }
            }
        }
        report
    }

    /// Tick every [`Initialized`](PluginState::Initialized) plugin in
    /// registration order.
    ///
    /// Panics raised by a plugin's `tick` body are caught via
    /// [`std::panic::catch_unwind`]; the panicking plugin is marked
    /// [`Failed`](PluginState::Failed) and a structured diagnostic is
    /// emitted. Resources held by the panicking plugin during the panic
    /// are unrecoverable; the leak is reported separately. Other plugins
    /// in the same `tick_all` call continue to run.
    pub fn tick_all(&mut self, ctx: &mut PluginContext<'_>) -> TickReport {
        let mut report = TickReport::default();
        for id in self.insertion_order.clone() {
            if let Some(record) = self.plugins.get_mut(&id) {
                if record.state != PluginState::Initialized {
                    continue;
                }

                let pre_call_resources = ctx.snapshot_resource_ids();
                let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    record.plugin.tick(ctx)
                }));
                let post_call_resources = ctx.snapshot_resource_ids();
                let leaked: Vec<_> = pre_call_resources
                    .difference(&post_call_resources)
                    .copied()
                    .collect();

                match panic_result {
                    Ok(Ok(())) => {
                        if leaked.is_empty() {
                            report.ticked += 1;
                        } else {
                            let msg = format!(
                                "plugin {id} returned Ok but leaked {n} resource(s); orchestrator state may be incomplete",
                                n = leaked.len()
                            );
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                msg.clone(),
                            ));
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
                        }
                    }
                    Ok(Err(plugin_err)) => {
                        emit_plugin_err_diagnostic(ctx, &id, PluginPhase::Tick, &plugin_err);
                        if !leaked.is_empty() {
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!(
                                    "plugin {id} leaked {n} resource(s) on tick failure",
                                    n = leaked.len()
                                ),
                            ));
                        }
                        let msg = plugin_err.to_string();
                        record.state = PluginState::Failed;
                        report.failed.push((id, msg));
                    }
                    Err(panic_payload) => {
                        let payload_str = panic_payload_to_string(&panic_payload);
                        ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(format!(
                            "plugin {id} PANICKED during tick: {payload_str}"
                        )));
                        if !leaked.is_empty() {
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!(
                                    "plugin {id} leaked {n} resource(s) during panic; orchestrator state may be incomplete",
                                    n = leaked.len()
                                ),
                            ));
                        }
                        let panic_err = PluginError::Panic {
                            phase: PluginPhase::Tick,
                            payload: payload_str,
                        };
                        record.state = PluginState::Failed;
                        report.failed.push((id, panic_err.to_string()));
                    }
                }
            }
        }
        report
    }

    /// Shutdown every [`Initialized`](PluginState::Initialized) plugin in
    /// REVERSE registration order (LIFO).
    ///
    /// [`Failed`](PluginState::Failed) and [`Pending`](PluginState::Pending)
    /// plugins are removed without a `shutdown` call. After this returns,
    /// the host's plugin set is empty.
    ///
    /// Panics raised by a plugin's `shutdown` body are caught via
    /// [`std::panic::catch_unwind`]; the panicking plugin is reported as
    /// failed in the [`ShutdownReport`] and a structured diagnostic is
    /// emitted. Resources held by the panicking plugin during the panic
    /// are unrecoverable; the leak is reported separately. Other plugins
    /// in the same `shutdown_all` call continue to run.
    pub fn shutdown_all(&mut self, ctx: &mut PluginContext<'_>) -> ShutdownReport {
        let mut report = ShutdownReport::default();
        // LIFO: shutdown in reverse of insertion order.
        let order: Vec<_> = self.insertion_order.iter().rev().cloned().collect();
        for id in order {
            if let Some(mut record) = self.plugins.remove(&id) {
                if record.state == PluginState::Initialized {
                    record.state = PluginState::ShuttingDown;

                    let pre_call_resources = ctx.snapshot_resource_ids();
                    let panic_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            record.plugin.shutdown(ctx)
                        }));
                    let post_call_resources = ctx.snapshot_resource_ids();
                    let leaked: Vec<_> = pre_call_resources
                        .difference(&post_call_resources)
                        .copied()
                        .collect();

                    match panic_result {
                        Ok(Ok(())) => {
                            if leaked.is_empty() {
                                record.state = PluginState::Shutdown;
                                report.shutdown.push(id);
                            } else {
                                let msg = format!(
                                    "plugin {id} returned Ok but leaked {n} resource(s); orchestrator state may be incomplete",
                                    n = leaked.len()
                                );
                                ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                    msg.clone(),
                                ));
                                record.state = PluginState::Failed;
                                report.failed.push((id, msg));
                            }
                        }
                        Ok(Err(plugin_err)) => {
                            emit_plugin_err_diagnostic(
                                ctx,
                                &id,
                                PluginPhase::Shutdown,
                                &plugin_err,
                            );
                            if !leaked.is_empty() {
                                ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                    format!(
                                        "plugin {id} leaked {n} resource(s) on shutdown failure",
                                        n = leaked.len()
                                    ),
                                ));
                            }
                            let msg = plugin_err.to_string();
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
                        }
                        Err(panic_payload) => {
                            let payload_str = panic_payload_to_string(&panic_payload);
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!("plugin {id} PANICKED during shutdown: {payload_str}"),
                            ));
                            if !leaked.is_empty() {
                                ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                    format!(
                                        "plugin {id} leaked {n} resource(s) during panic; orchestrator state may be incomplete",
                                        n = leaked.len()
                                    ),
                                ));
                            }
                            let panic_err = PluginError::Panic {
                                phase: PluginPhase::Shutdown,
                                payload: payload_str,
                            };
                            record.state = PluginState::Failed;
                            report.failed.push((id, panic_err.to_string()));
                        }
                    }
                }
                // Pending / Failed / ShuttingDown / Shutdown plugins are just
                // dropped — we do not call shutdown() on them (plugin-fatal
                // isolation).
            }
        }
        self.insertion_order.clear();
        report
    }

    /// Borrow the [`PluginRecord`] for `id` if registered.
    ///
    /// Reflective inspection target for forward use cases (debug overlays,
    /// plugin inspectors, future hot-reload integrations). No in-tree
    /// consumer today; kept stable so external tooling can introspect the
    /// host registry.
    #[must_use]
    pub fn get(&self, id: &PluginId) -> Option<&PluginRecord> {
        self.plugins.get(id)
    }

    /// Return the current [`PluginState`] for `id`, or `None` if not registered.
    #[must_use]
    pub fn state(&self, id: &PluginId) -> Option<PluginState> {
        self.plugins.get(id).map(|r| r.state)
    }

    /// Number of plugins currently registered (any state).
    #[must_use]
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Iterate registered plugin ids in `BTreeMap` order (lexicographic).
    pub fn iter_ids(&self) -> impl Iterator<Item = &PluginId> {
        self.plugins.keys()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHost")
            .field("plugins", &self.plugins)
            .field("insertion_order", &self.insertion_order)
            .finish()
    }
}

/// Result of a [`PluginHost::init_all`] call.
#[derive(Debug, Default)]
pub struct InitReport {
    /// Plugins that successfully initialized.
    pub initialized: Vec<PluginId>,
    /// Plugins whose `init` returned an error or panicked, paired with the
    /// formatted error string.
    pub failed: Vec<(PluginId, String)>,
}

/// Result of a [`PluginHost::tick_all`] call.
#[derive(Debug, Default)]
pub struct TickReport {
    /// Number of plugins whose `tick` returned `Ok`.
    pub ticked: usize,
    /// Plugins whose `tick` returned an error or panicked, paired with the
    /// formatted error string.
    pub failed: Vec<(PluginId, String)>,
}

/// Result of a [`PluginHost::shutdown_all`] call.
#[derive(Debug, Default)]
pub struct ShutdownReport {
    /// Plugins that successfully shut down.
    pub shutdown: Vec<PluginId>,
    /// Plugins whose `shutdown` returned an error or panicked, paired with
    /// the formatted error string.
    pub failed: Vec<(PluginId, String)>,
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Auto-emit a [`rge_kernel_diagnostics::Diagnostic`] for a plugin that
/// returned `Err`.
///
/// Severity discrimination per audit-2 A5.1:
///
/// * [`PluginError::ContractViolation`] → `Warning` (caller misconfiguration;
///   not a plugin bug).
/// * Everything else → `Error` (genuine plugin failure).
///
/// Phase argument controls the prefix wording (`init failed` / `tick failed`
/// / `shutdown failed`) so the diagnostic stream stays grep-friendly.
fn emit_plugin_err_diagnostic(
    ctx: &mut PluginContext<'_>,
    id: &PluginId,
    phase: PluginPhase,
    plugin_err: &PluginError,
) {
    let msg = format!("plugin {id} {phase} failed: {plugin_err}");
    let diag = match plugin_err {
        PluginError::ContractViolation { .. } => rge_kernel_diagnostics::Diagnostic::warning(msg),
        _ => rge_kernel_diagnostics::Diagnostic::error(msg),
    };
    ctx.emit_diagnostic(diag);
}

/// Best-effort string extraction from a `catch_unwind` payload.
///
/// `std::panic::catch_unwind` yields `Box<dyn Any + Send + 'static>`; the
/// payload is whatever was passed to `panic!`. Common cases:
///
/// * `panic!("literal")` → `&'static str`
/// * `panic!("{}", value)` → `String`
/// * Custom payloads (rare) → unknown — we render a `type_id=` placeholder.
fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else {
        format!(
            "(non-string panic payload, type_id={:?})",
            (**payload).type_id()
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};

    use super::*;
    use crate::plugin::{Plugin, PluginError};

    /// Test helper: a plugin that records its lifecycle events into a shared
    /// log so tests can assert ordering.
    ///
    /// Allow `clippy::struct_excessive_bools` because this is a test fixture
    /// driving an N-way behavior matrix (8 independent failure / panic /
    /// resource-misuse modes); each flag reflects an orthogonal test
    /// dimension that doesn't compose with the others, so a state-machine
    /// rewrite would obscure the per-test setup. A "real" plugin never
    /// looks anything like this.
    #[allow(clippy::struct_excessive_bools)]
    struct TestPlugin {
        id: PluginId,
        log: Arc<Mutex<Vec<String>>>,
        fail_init: bool,
        fail_tick: bool,
        fail_shutdown: bool,
        panic_init: bool,
        panic_tick: bool,
        panic_shutdown: bool,
        /// On tick: take a `u32` from ctx but never put it back. Simulates a
        /// plugin that fails to honor the resource-handoff invariant. Returns
        /// `Ok(())` so the leak detection path is exercised independently of
        /// any `Err` return.
        leak_u32_in_tick: bool,
        /// On init: take a `u32` from ctx but never put it back. Used to
        /// drive the init-phase leak-detection path (audit-2 closure: tick
        /// has `tick_all_detects_resource_leak` but init lacked a
        /// counterpart until this dispatch).
        leak_u32_in_init: bool,
        /// On shutdown: take a `u32` from ctx but never put it back. Used to
        /// drive the shutdown-phase leak-detection path (audit-2 closure
        /// alongside `leak_u32_in_init`).
        leak_u32_in_shutdown: bool,
        /// On tick: return [`PluginError::ContractViolation`] for a missing
        /// resource. Used to verify warning-vs-error severity discrimination.
        emit_contract_violation_in_tick: bool,
    }

    impl TestPlugin {
        fn new(id: &str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                id: PluginId::new(id),
                log,
                fail_init: false,
                fail_tick: false,
                fail_shutdown: false,
                panic_init: false,
                panic_tick: false,
                panic_shutdown: false,
                leak_u32_in_tick: false,
                leak_u32_in_init: false,
                leak_u32_in_shutdown: false,
                emit_contract_violation_in_tick: false,
            }
        }

        fn with_init_failure(mut self) -> Self {
            self.fail_init = true;
            self
        }

        fn with_tick_failure(mut self) -> Self {
            self.fail_tick = true;
            self
        }

        fn with_shutdown_failure(mut self) -> Self {
            self.fail_shutdown = true;
            self
        }

        fn with_init_panic(mut self) -> Self {
            self.panic_init = true;
            self
        }

        fn with_tick_panic(mut self) -> Self {
            self.panic_tick = true;
            self
        }

        fn with_shutdown_panic(mut self) -> Self {
            self.panic_shutdown = true;
            self
        }

        /// Plugin variant that takes a `u32` from ctx in `tick` and never
        /// puts it back. Used to drive the leak-detection path.
        fn with_resource_take_no_putback(mut self) -> Self {
            self.leak_u32_in_tick = true;
            self
        }

        /// Plugin variant that takes a `u32` from ctx in `init` and never
        /// puts it back. Drives the init-phase leak-detection path.
        fn with_init_resource_take_no_putback(mut self) -> Self {
            self.leak_u32_in_init = true;
            self
        }

        /// Plugin variant that takes a `u32` from ctx in `shutdown` and
        /// never puts it back. Drives the shutdown-phase leak-detection
        /// path.
        fn with_shutdown_resource_take_no_putback(mut self) -> Self {
            self.leak_u32_in_shutdown = true;
            self
        }

        /// Plugin variant whose `tick` returns
        /// [`PluginError::ContractViolation`] (not `RuntimeFault`). Used to
        /// verify warning-vs-error severity discrimination.
        fn with_contract_violation_in_tick(mut self) -> Self {
            self.emit_contract_violation_in_tick = true;
            self
        }
    }

    // Allow `clippy::manual_assert` for the panic! calls below: these are
    // INTENTIONAL panics meant to drive the host's catch_unwind recovery
    // path. `assert!(!flag, "msg")` would have identical runtime behaviour
    // but reads as a precondition check rather than a deliberate panic
    // injection, which obscures the test intent.
    #[allow(clippy::manual_assert)]
    impl Plugin for TestPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log.lock().unwrap().push(format!("init:{}", self.id));
            if self.panic_init {
                panic!("test plugin {} init panic", self.id);
            }
            if self.leak_u32_in_init {
                // Take but don't put back — the init-phase leak path.
                let _ = ctx.take::<u32>();
                return Ok(());
            }
            if self.fail_init {
                Err(PluginError::init(format!("{} failed init", self.id)))
            } else {
                Ok(())
            }
        }

        fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log.lock().unwrap().push(format!("tick:{}", self.id));
            if self.panic_tick {
                panic!("test plugin {} tick panic", self.id);
            }
            if self.leak_u32_in_tick {
                // Take but don't put back — the leak path.
                let _ = ctx.take::<u32>();
                return Ok(());
            }
            if self.emit_contract_violation_in_tick {
                return Err(PluginError::contract_violation("World"));
            }
            if self.fail_tick {
                Err(PluginError::runtime_fault(format!(
                    "{} failed tick",
                    self.id
                )))
            } else {
                Ok(())
            }
        }

        fn shutdown(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .push(format!("shutdown:{}", self.id));
            if self.panic_shutdown {
                panic!("test plugin {} shutdown panic", self.id);
            }
            if self.leak_u32_in_shutdown {
                // Take but don't put back — the shutdown-phase leak path.
                let _ = ctx.take::<u32>();
                return Ok(());
            }
            if self.fail_shutdown {
                Err(PluginError::shutdown(format!(
                    "{} failed shutdown",
                    self.id
                )))
            } else {
                Ok(())
            }
        }
    }

    /// Plugin whose `id()` returns a different value than registration —
    /// for `IdMismatch` test.
    struct LyingPlugin {
        actual_id: PluginId,
    }

    impl Plugin for LyingPlugin {
        fn id(&self) -> PluginId {
            self.actual_id.clone()
        }
        fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[test]
    fn host_new_is_empty() {
        let host = PluginHost::new();
        assert_eq!(host.count(), 0);
        assert_eq!(host.iter_ids().count(), 0);
    }

    #[test]
    fn register_adds_plugin_with_pending_state() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log)))
            .expect("register");
        assert_eq!(host.count(), 1);
        assert_eq!(host.state(&id), Some(PluginState::Pending));
    }

    #[test]
    fn register_rejects_duplicate_id() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log.clone())))
            .expect("first register");
        let err = host
            .register(id.clone(), Box::new(TestPlugin::new("a", log)))
            .expect_err("second register");
        assert!(matches!(err, PluginHostError::DuplicateId { id: ref e } if *e == id));
    }

    #[test]
    fn register_rejects_id_mismatch() {
        let mut host = PluginHost::new();
        let registered = PluginId::new("registered-name");
        let lying = LyingPlugin {
            actual_id: PluginId::new("actual-name"),
        };
        let err = host
            .register(registered.clone(), Box::new(lying))
            .expect_err("register should fail");
        assert!(matches!(
            err,
            PluginHostError::IdMismatch { ref id, ref reported, .. }
                if *id == registered && reported.as_str() == "actual-name"
        ));
    }

    #[test]
    fn init_all_transitions_pending_to_initialized() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();
        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log.clone())))
            .expect("register");

        let report = host.init_all(&mut ctx);
        assert_eq!(report.initialized, vec![id.clone()]);
        assert!(report.failed.is_empty());
        assert_eq!(host.state(&id), Some(PluginState::Initialized));
        assert_eq!(*log.lock().unwrap(), vec!["init:a"]);
    }

    #[test]
    fn init_all_marks_failed_plugins_failed_but_continues_others() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_init_failure()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        let report = host.init_all(&mut ctx);
        assert_eq!(report.initialized.len(), 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("b"));
        assert!(report.failed[0].1.contains("failed init"));

        assert_eq!(
            host.state(&PluginId::new("a")),
            Some(PluginState::Initialized)
        );
        assert_eq!(host.state(&PluginId::new("b")), Some(PluginState::Failed));
        assert_eq!(
            host.state(&PluginId::new("c")),
            Some(PluginState::Initialized)
        );

        // All three plugins had `init` called even though b failed.
        assert_eq!(*log.lock().unwrap(), vec!["init:a", "init:b", "init:c"]);
    }

    #[test]
    fn tick_all_only_ticks_initialized() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_init_failure()),
        )
        .expect("register b");

        host.init_all(&mut ctx);
        log.lock().unwrap().clear();

        let report = host.tick_all(&mut ctx);
        assert_eq!(report.ticked, 1);
        assert!(report.failed.is_empty());
        assert_eq!(*log.lock().unwrap(), vec!["tick:a"]);
    }

    #[test]
    fn tick_all_marks_failing_plugin_failed() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone()).with_tick_failure()),
        )
        .expect("register a");

        host.init_all(&mut ctx);
        let report = host.tick_all(&mut ctx);
        assert_eq!(report.ticked, 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(host.state(&PluginId::new("a")), Some(PluginState::Failed));
    }

    /// Audit-1 + audit-2 closure: multi-plugin tick-failure isolation.
    /// Three plugins; b fails tick; verify a and c still tick + are still
    /// Initialized; b is marked Failed; report carries one entry; all three
    /// plugins saw `tick:N` invocations in the log.
    #[test]
    fn tick_all_marks_failed_plugins_failed_but_continues_others() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_tick_failure()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        host.init_all(&mut ctx);
        log.lock().unwrap().clear();

        let report = host.tick_all(&mut ctx);
        assert_eq!(report.ticked, 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("b"));
        assert!(report.failed[0].1.contains("failed tick"));

        assert_eq!(
            host.state(&PluginId::new("a")),
            Some(PluginState::Initialized)
        );
        assert_eq!(host.state(&PluginId::new("b")), Some(PluginState::Failed));
        assert_eq!(
            host.state(&PluginId::new("c")),
            Some(PluginState::Initialized)
        );

        // Even with b failing, all three saw `tick:N` calls.
        assert_eq!(*log.lock().unwrap(), vec!["tick:a", "tick:b", "tick:c"]);
    }

    #[test]
    fn shutdown_all_runs_in_reverse_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone())),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        host.init_all(&mut ctx);
        log.lock().unwrap().clear();

        let report = host.shutdown_all(&mut ctx);
        assert_eq!(report.shutdown.len(), 3);
        assert!(report.failed.is_empty());
        assert_eq!(host.count(), 0);

        // LIFO: c shuts down before b before a.
        assert_eq!(
            *log.lock().unwrap(),
            vec!["shutdown:c", "shutdown:b", "shutdown:a"]
        );
    }

    #[test]
    fn shutdown_all_skips_failed_plugins() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_init_failure()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        host.init_all(&mut ctx);
        log.lock().unwrap().clear();

        let report = host.shutdown_all(&mut ctx);
        assert_eq!(report.shutdown.len(), 2); // only a + c
                                              // No shutdown call for the Failed plugin b.
        let calls = log.lock().unwrap().clone();
        assert!(!calls.iter().any(|c| c == "shutdown:b"));
        assert!(calls.iter().any(|c| c == "shutdown:a"));
        assert!(calls.iter().any(|c| c == "shutdown:c"));
    }

    #[test]
    fn shutdown_all_records_shutdown_failures() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone()).with_shutdown_failure()),
        )
        .expect("register a");

        host.init_all(&mut ctx);
        let report = host.shutdown_all(&mut ctx);
        assert_eq!(report.shutdown.len(), 0);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("failed shutdown"));
        assert_eq!(host.count(), 0);
    }

    #[test]
    fn unregister_runs_shutdown_if_initialized() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log.clone())))
            .expect("register");

        host.init_all(&mut ctx);
        log.lock().unwrap().clear();

        host.unregister(&id, &mut ctx).expect("unregister");
        assert_eq!(host.count(), 0);
        assert_eq!(*log.lock().unwrap(), vec!["shutdown:a"]);
    }

    #[test]
    fn unregister_does_not_run_shutdown_for_pending() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log.clone())))
            .expect("register");

        host.unregister(&id, &mut ctx).expect("unregister");
        assert_eq!(host.count(), 0);
        // Pending plugin should not have shutdown called.
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn init_all_auto_emits_diagnostic_on_plugin_init_failure() {
        // Pairing-5 closure: a plugin that fails init produces a synthetic
        // Diagnostic::error in the sink, even if the plugin itself doesn't
        // call ctx.emit_diagnostic. The host is the single source of truth
        // for plugin-failure surfacing.
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone()).with_init_failure()),
        )
        .expect("register");

        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx)
        };
        assert_eq!(report.failed.len(), 1);
        // Auto-emit produced exactly one error diagnostic.
        assert_eq!(diags.len(), 1);
        assert!(diags.has_errors());
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages[0].starts_with("plugin a init failed:"),
            "expected auto-emit prefix; got: {}",
            messages[0]
        );
    }

    #[test]
    fn init_all_does_not_auto_emit_diagnostic_on_success() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        // Successful init produces no auto-emit (the plugin can still emit
        // its own diagnostics, but TestPlugin doesn't).
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn tick_all_auto_emits_diagnostic_on_plugin_tick_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone()).with_tick_failure()),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        // After successful init, sink is empty.
        assert_eq!(diags.len(), 0);

        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.tick_all(&mut ctx)
        };
        assert_eq!(report.failed.len(), 1);
        assert_eq!(diags.len(), 1);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages[0].starts_with("plugin a tick failed:"));
    }

    #[test]
    fn shutdown_all_auto_emits_diagnostic_on_plugin_shutdown_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone()).with_shutdown_failure()),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        assert_eq!(diags.len(), 0);

        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.shutdown_all(&mut ctx)
        };
        assert_eq!(report.failed.len(), 1);
        assert_eq!(diags.len(), 1);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages[0].starts_with("plugin a shutdown failed:"));
    }

    #[test]
    fn init_all_auto_emits_one_diagnostic_per_failing_plugin() {
        // 3 plugins; b and c both fail init; expect exactly 2 auto-emits.
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_init_failure()),
        )
        .expect("b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone()).with_init_failure()),
        )
        .expect("c");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        // Two auto-emits, one per failure.
        assert_eq!(diags.len(), 2);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages[0].starts_with("plugin b init failed:"));
        assert!(messages[1].starts_with("plugin c init failed:"));
    }

    #[test]
    fn unregister_returns_not_found_for_missing_id() {
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();
        let err = host
            .unregister(&PluginId::new("missing"), &mut ctx)
            .expect_err("should fail");
        assert!(matches!(err, PluginHostError::NotFound { .. }));
    }

    #[test]
    fn state_returns_current_plugin_state() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        let mut host = PluginHost::new();

        let id = PluginId::new("a");
        host.register(id.clone(), Box::new(TestPlugin::new("a", log)))
            .expect("register");
        assert_eq!(host.state(&id), Some(PluginState::Pending));

        host.init_all(&mut ctx);
        assert_eq!(host.state(&id), Some(PluginState::Initialized));

        assert_eq!(host.state(&PluginId::new("ghost")), None);
    }

    #[test]
    fn iter_ids_yields_all_registered() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = PluginHost::new();
        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone())),
        )
        .expect("b");
        host.register(PluginId::new("c"), Box::new(TestPlugin::new("c", log)))
            .expect("c");

        let ids: Vec<&PluginId> = host.iter_ids().collect();
        assert_eq!(ids.len(), 3);
        // BTreeMap iteration order is sorted.
        assert_eq!(*ids[0], PluginId::new("a"));
        assert_eq!(*ids[1], PluginId::new("b"));
        assert_eq!(*ids[2], PluginId::new("c"));
    }

    // ===== Phase 0 audit-2 A5.1 closure: panic-recovery + leak detection =====

    /// Three plugins; b panics during init; a and c still init successfully;
    /// b is Failed; b's report entry contains "PANIC"; a panic-prefixed
    /// diagnostic is emitted.
    #[test]
    fn init_all_recovers_from_panicking_plugin() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_init_panic()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx)
        };

        assert_eq!(report.initialized.len(), 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("b"));
        let failed_msg = &report.failed[0].1;
        assert!(
            failed_msg.contains("panicked during init"),
            "expected panic phrasing in report; got: {failed_msg}",
        );

        assert_eq!(
            host.state(&PluginId::new("a")),
            Some(PluginState::Initialized)
        );
        assert_eq!(host.state(&PluginId::new("b")), Some(PluginState::Failed));
        assert_eq!(
            host.state(&PluginId::new("c")),
            Some(PluginState::Initialized)
        );

        // All three plugins entered init; b panicked but the host caught it.
        assert_eq!(*log.lock().unwrap(), vec!["init:a", "init:b", "init:c"]);

        // Auto-emit produced a PANIC-prefixed Diagnostic::error.
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("PANICKED during init") && m.contains("plugin b")),
            "expected PANICKED-during-init diagnostic for b; got {messages:?}",
        );
    }

    /// Tick variant of `init_all_recovers_from_panicking_plugin`.
    #[test]
    fn tick_all_recovers_from_panicking_plugin() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_tick_panic()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        log.lock().unwrap().clear();
        // Ignore init diagnostics (none, in this case — TestPlugin emits none
        // on success, init does NOT panic for these plugins).

        let pre_tick_diag_count = diags.len();
        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.tick_all(&mut ctx)
        };

        assert_eq!(report.ticked, 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("b"));
        assert!(report.failed[0].1.contains("panicked during tick"));

        assert_eq!(
            host.state(&PluginId::new("a")),
            Some(PluginState::Initialized)
        );
        assert_eq!(host.state(&PluginId::new("b")), Some(PluginState::Failed));
        assert_eq!(
            host.state(&PluginId::new("c")),
            Some(PluginState::Initialized)
        );

        assert_eq!(*log.lock().unwrap(), vec!["tick:a", "tick:b", "tick:c"]);

        // Exactly one new diagnostic from the tick: the PANICKED one.
        let new_messages: Vec<&str> = diags
            .iter()
            .skip(pre_tick_diag_count)
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            new_messages
                .iter()
                .any(|m| m.contains("PANICKED during tick") && m.contains("plugin b")),
            "expected PANICKED-during-tick diagnostic for b; got {new_messages:?}",
        );
    }

    /// Shutdown variant of `init_all_recovers_from_panicking_plugin`.
    #[test]
    fn shutdown_all_recovers_from_panicking_plugin() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("a"),
            Box::new(TestPlugin::new("a", log.clone())),
        )
        .expect("register a");
        host.register(
            PluginId::new("b"),
            Box::new(TestPlugin::new("b", log.clone()).with_shutdown_panic()),
        )
        .expect("register b");
        host.register(
            PluginId::new("c"),
            Box::new(TestPlugin::new("c", log.clone())),
        )
        .expect("register c");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        log.lock().unwrap().clear();
        let pre_shutdown_diag_count = diags.len();

        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.shutdown_all(&mut ctx)
        };
        // a and c shut down cleanly; b panicked → reported failed.
        assert_eq!(report.shutdown.len(), 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("b"));
        assert!(report.failed[0].1.contains("panicked during shutdown"));

        // Host registry is empty after shutdown_all regardless of outcomes.
        assert_eq!(host.count(), 0);

        // LIFO order: c, b, a all entered shutdown.
        assert_eq!(
            *log.lock().unwrap(),
            vec!["shutdown:c", "shutdown:b", "shutdown:a"],
        );

        let new_messages: Vec<&str> = diags
            .iter()
            .skip(pre_shutdown_diag_count)
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            new_messages
                .iter()
                .any(|m| m.contains("PANICKED during shutdown") && m.contains("plugin b")),
            "expected PANICKED-during-shutdown diagnostic for b; got {new_messages:?}",
        );
    }

    /// Resource-leak detection: a plugin that takes a `u32` from ctx but
    /// never puts it back is detected. Plugin is marked Failed; an
    /// error-severity diagnostic with "leaked" wording is emitted; ctx no
    /// longer contains the resource.
    #[test]
    fn tick_all_detects_resource_leak() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("leaky"),
            Box::new(TestPlugin::new("leaky", log.clone()).with_resource_take_no_putback()),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        let pre_tick_diag_count = diags.len();

        let mut ctx = PluginContext::new(&mut diags);
        let _ = ctx.insert(42u32);
        assert!(ctx.contains::<u32>());

        let report = host.tick_all(&mut ctx);

        // The plugin returned Ok but leaked → report says failed.
        assert_eq!(report.ticked, 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, PluginId::new("leaky"));
        assert!(
            report.failed[0].1.contains("leaked"),
            "report should mention leak; got {}",
            report.failed[0].1,
        );

        // Plugin marked Failed.
        assert_eq!(
            host.state(&PluginId::new("leaky")),
            Some(PluginState::Failed),
        );

        // ctx no longer contains u32 — it was taken and never put back.
        assert!(!ctx.contains::<u32>());

        // Drop ctx so the diagnostic borrow ends, then inspect diagnostics.
        drop(ctx);

        let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
        assert!(
            new_diags
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains("leaked")),
            "expected error-severity 'leaked' diagnostic; got {:?}",
            new_diags
                .iter()
                .map(|d| (d.severity, d.message.as_str()))
                .collect::<Vec<_>>()
        );
    }

    /// Init-phase leak detection (audit-2 gap-4 closure). Sibling to
    /// `tick_all_detects_resource_leak`: a plugin that takes a `u32` in
    /// `init` but never puts it back is detected via TypeId-snapshot diff,
    /// marked Failed, and surfaces an Error-severity "leaked" diagnostic.
    #[test]
    fn init_all_detects_resource_leak() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();
        let id = PluginId::new("leaky-init");

        host.register(
            id.clone(),
            Box::new(TestPlugin::new("leaky-init", log).with_init_resource_take_no_putback()),
        )
        .expect("register");

        let pre = diags.len();
        let mut ctx = PluginContext::new(&mut diags);
        let _ = ctx.insert(42u32);
        assert!(ctx.contains::<u32>());

        let report = host.init_all(&mut ctx);

        assert_eq!(report.initialized.len(), 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, id);
        assert!(report.failed[0].1.contains("leaked"));
        assert_eq!(host.state(&id), Some(PluginState::Failed));
        assert!(!ctx.contains::<u32>());
        drop(ctx);

        let new_diags: Vec<_> = diags.iter().skip(pre).collect();
        assert!(
            new_diags
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains("leaked")),
            "expected Error-severity 'leaked' diagnostic from init leak; got {:?}",
            new_diags
                .iter()
                .map(|d| (d.severity, d.message.as_str()))
                .collect::<Vec<_>>(),
        );
    }

    /// Shutdown-phase leak detection (audit-2 gap-4 closure). Sibling to
    /// `init_all_detects_resource_leak` + `tick_all_detects_resource_leak`:
    /// a plugin that takes a `u32` in `shutdown` but never puts it back is
    /// detected, reported as failed, and surfaces an Error-severity
    /// "leaked" diagnostic. Host registry drains regardless (LIFO).
    #[test]
    fn shutdown_all_detects_resource_leak() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();
        let id = PluginId::new("leaky-shutdown");

        host.register(
            id.clone(),
            Box::new(
                TestPlugin::new("leaky-shutdown", log).with_shutdown_resource_take_no_putback(),
            ),
        )
        .expect("register");
        {
            let mut ctx = PluginContext::new(&mut diags);
            assert!(host.init_all(&mut ctx).failed.is_empty());
        }

        let pre = diags.len();
        let mut ctx = PluginContext::new(&mut diags);
        let _ = ctx.insert(99u32);
        assert!(ctx.contains::<u32>());

        let report = host.shutdown_all(&mut ctx);

        assert_eq!(report.shutdown.len(), 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, id);
        assert!(report.failed[0].1.contains("leaked"));
        assert_eq!(host.count(), 0);
        assert!(!ctx.contains::<u32>());
        drop(ctx);

        let new_diags: Vec<_> = diags.iter().skip(pre).collect();
        assert!(
            new_diags
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains("leaked")),
            "expected Error-severity 'leaked' diagnostic from shutdown leak; got {:?}",
            new_diags
                .iter()
                .map(|d| (d.severity, d.message.as_str()))
                .collect::<Vec<_>>(),
        );
    }

    /// Severity discrimination: a plugin returning `PluginError::ContractViolation`
    /// produces a Warning auto-emit, NOT an Error. Other plugin errors continue
    /// to produce Errors.
    #[test]
    fn tick_all_emits_warning_for_contract_violation() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        host.register(
            PluginId::new("contract"),
            Box::new(TestPlugin::new("contract", log.clone()).with_contract_violation_in_tick()),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        let pre_tick_diag_count = diags.len();
        let report = {
            let mut ctx = PluginContext::new(&mut diags);
            host.tick_all(&mut ctx)
        };

        assert_eq!(report.ticked, 0);
        assert_eq!(report.failed.len(), 1);

        let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
        assert_eq!(
            new_diags.len(),
            1,
            "expected one warning diagnostic; got {} = {:?}",
            new_diags.len(),
            new_diags
                .iter()
                .map(|d| (d.severity, d.message.as_str()))
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            new_diags[0].severity,
            Severity::Warning,
            "ContractViolation must auto-emit as Warning, not Error",
        );
        assert!(
            new_diags[0].message.contains("contract violation"),
            "warning should reference contract violation; got: {}",
            new_diags[0].message,
        );
    }

    /// Per-LOW #5 invariant: an unregister-shutdown that errors emits a
    /// Warning (NOT an Error) — host-initiated unregister is non-fatal by
    /// design.
    #[test]
    fn unregister_emits_warning_on_shutdown_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut diags = DiagnosticAggregator::new();
        let mut host = PluginHost::new();

        let id = PluginId::new("u");
        host.register(
            id.clone(),
            Box::new(TestPlugin::new("u", log.clone()).with_shutdown_failure()),
        )
        .expect("register");

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.init_all(&mut ctx);
        }
        let pre_unregister_diag_count = diags.len();

        {
            let mut ctx = PluginContext::new(&mut diags);
            host.unregister(&id, &mut ctx).expect("unregister");
        }

        let new_diags: Vec<_> = diags.iter().skip(pre_unregister_diag_count).collect();
        assert_eq!(
            new_diags.len(),
            1,
            "expected exactly one warning diagnostic from unregister-shutdown failure",
        );
        assert_eq!(
            new_diags[0].severity,
            Severity::Warning,
            "unregister-shutdown failure must auto-emit as Warning, not Error",
        );
        assert!(
            new_diags[0].message.contains("unregister-shutdown failed"),
            "warning should reference unregister-shutdown; got: {}",
            new_diags[0].message,
        );
    }
}
