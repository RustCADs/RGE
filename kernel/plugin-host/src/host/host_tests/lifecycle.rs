//! Lifecycle happy-path + plugin-fatal isolation tests.
//!
//! Sub-module of [`crate::host::host_tests`]; covers the
//! `init_all` / `tick_all` / `shutdown_all` / `unregister` paths through
//! their non-panic, non-leak scenarios. Failure-isolation tests live here
//! because they exercise the same matched-state-transition code path as the
//! happy paths (just with one plugin returning `Err`); panic-recovery and
//! leak-detection sit in dedicated sibling files.

use std::sync::{Arc, Mutex};

use rge_kernel_diagnostics::DiagnosticAggregator;

use super::fixtures::TestPlugin;
use crate::context::PluginContext;
use crate::host::{PluginHost, PluginHostError, PluginState};
use crate::plugin::PluginId;

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
fn unregister_returns_not_found_for_missing_id() {
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    let mut host = PluginHost::new();
    let err = host
        .unregister(&PluginId::new("missing"), &mut ctx)
        .expect_err("should fail");
    assert!(matches!(err, PluginHostError::NotFound { .. }));
}
