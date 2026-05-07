//! Resource-leak detection tests.
//!
//! Sub-module of [`crate::host::host_tests`]; covers the
//! TypeId-snapshot diff that runs around every plugin lifecycle call. A
//! plugin that takes a resource out of [`crate::context::PluginContext`]
//! but never puts it back is detected via the
//! `pre_call_resources.difference(&post_call_resources)` check; the
//! plugin is marked [`crate::host::PluginState::Failed`] and an
//! Error-severity diagnostic is auto-emitted, even if the plugin
//! returned `Ok`.

use std::sync::{Arc, Mutex};

use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};

use super::fixtures::TestPlugin;
use crate::context::PluginContext;
use crate::host::{PluginHost, PluginState};
use crate::plugin::PluginId;

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
        Box::new(TestPlugin::new("leaky-shutdown", log).with_shutdown_resource_take_no_putback()),
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
