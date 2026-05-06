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

use std::collections::BTreeMap;

use thiserror::Error;

use crate::context::PluginContext;
use crate::plugin::{Plugin, PluginId};

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
    /// [`Plugin::shutdown`](crate::Plugin::shutdown) is called best-effort
    /// (any error is silently absorbed — plugin-fatal isolation; the plugin
    /// itself can emit a diagnostic via the supplied context if it cares).
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
            // Best-effort shutdown; drop any error per plugin-fatal isolation.
            // (Plugin authors who care about surfacing the error should emit a
            // diagnostic via the supplied context inside their `shutdown` body.)
            drop(record.plugin.shutdown(ctx));
        }
        Ok(())
    }

    /// Initialize every [`Pending`](PluginState::Pending) plugin in
    /// registration order.
    ///
    /// Failures are isolated: one plugin's init failure marks it
    /// [`Failed`](PluginState::Failed) but other plugins still init.
    pub fn init_all(&mut self, ctx: &mut PluginContext<'_>) -> InitReport {
        let mut report = InitReport::default();
        for id in self.insertion_order.clone() {
            if let Some(record) = self.plugins.get_mut(&id) {
                if record.state == PluginState::Pending {
                    match record.plugin.init(ctx) {
                        Ok(()) => {
                            record.state = PluginState::Initialized;
                            report.initialized.push(id);
                        }
                        Err(e) => {
                            // Auto-emit synthetic Diagnostic::error so the
                            // diagnostic stream is the single source of truth
                            // for plugin failures (Pairing-5 closure). Plugin
                            // authors that emit their own diagnostic before
                            // returning Err produce both: the auto-emit is
                            // additive, not a replacement.
                            let msg = e.to_string();
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!("plugin {id} init failed: {msg}"),
                            ));
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
                        }
                    }
                }
            }
        }
        report
    }

    /// Tick every [`Initialized`](PluginState::Initialized) plugin in
    /// registration order.
    pub fn tick_all(&mut self, ctx: &mut PluginContext<'_>) -> TickReport {
        let mut report = TickReport::default();
        for id in self.insertion_order.clone() {
            if let Some(record) = self.plugins.get_mut(&id) {
                if record.state == PluginState::Initialized {
                    match record.plugin.tick(ctx) {
                        Ok(()) => report.ticked += 1,
                        Err(e) => {
                            // Auto-emit (see init_all above).
                            let msg = e.to_string();
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!("plugin {id} tick failed: {msg}"),
                            ));
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
                        }
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
    pub fn shutdown_all(&mut self, ctx: &mut PluginContext<'_>) -> ShutdownReport {
        let mut report = ShutdownReport::default();
        // LIFO: shutdown in reverse of insertion order.
        let order: Vec<_> = self.insertion_order.iter().rev().cloned().collect();
        for id in order {
            if let Some(mut record) = self.plugins.remove(&id) {
                if record.state == PluginState::Initialized {
                    record.state = PluginState::ShuttingDown;
                    match record.plugin.shutdown(ctx) {
                        Ok(()) => {
                            record.state = PluginState::Shutdown;
                            report.shutdown.push(id);
                        }
                        Err(e) => {
                            // Auto-emit (see init_all above).
                            let msg = e.to_string();
                            ctx.emit_diagnostic(rge_kernel_diagnostics::Diagnostic::error(
                                format!("plugin {id} shutdown failed: {msg}"),
                            ));
                            record.state = PluginState::Failed;
                            report.failed.push((id, msg));
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
    /// Plugins whose `init` returned an error, paired with the formatted
    /// error string.
    pub failed: Vec<(PluginId, String)>,
}

/// Result of a [`PluginHost::tick_all`] call.
#[derive(Debug, Default)]
pub struct TickReport {
    /// Number of plugins whose `tick` returned `Ok`.
    pub ticked: usize,
    /// Plugins whose `tick` returned an error, paired with the formatted
    /// error string.
    pub failed: Vec<(PluginId, String)>,
}

/// Result of a [`PluginHost::shutdown_all`] call.
#[derive(Debug, Default)]
pub struct ShutdownReport {
    /// Plugins that successfully shut down.
    pub shutdown: Vec<PluginId>,
    /// Plugins whose `shutdown` returned an error, paired with the formatted
    /// error string.
    pub failed: Vec<(PluginId, String)>,
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;
    use crate::plugin::{Plugin, PluginError};

    /// Test helper: a plugin that records its lifecycle events into a shared
    /// log so tests can assert ordering.
    struct TestPlugin {
        id: PluginId,
        log: Arc<Mutex<Vec<String>>>,
        fail_init: bool,
        fail_tick: bool,
        fail_shutdown: bool,
    }

    impl TestPlugin {
        fn new(id: &str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                id: PluginId::new(id),
                log,
                fail_init: false,
                fail_tick: false,
                fail_shutdown: false,
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
    }

    impl Plugin for TestPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log.lock().unwrap().push(format!("init:{}", self.id));
            if self.fail_init {
                Err(PluginError::init(format!("{} failed init", self.id)))
            } else {
                Ok(())
            }
        }

        fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log.lock().unwrap().push(format!("tick:{}", self.id));
            if self.fail_tick {
                Err(PluginError::runtime(format!("{} failed tick", self.id)))
            } else {
                Ok(())
            }
        }

        fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .push(format!("shutdown:{}", self.id));
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
}
