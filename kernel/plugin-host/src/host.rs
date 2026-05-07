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
mod host_tests;
