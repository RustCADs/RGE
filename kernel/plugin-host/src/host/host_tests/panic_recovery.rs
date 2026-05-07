//! `catch_unwind` panic-recovery tests.
//!
//! Sub-module of [`crate::host::host_tests`]; covers the audit-2 A5.1 closure
//! that wraps every plugin lifecycle call in
//! [`std::panic::catch_unwind`]. A panicking plugin must:
//!
//! 1. Be marked [`crate::host::PluginState::Failed`].
//! 2. Have its [`crate::plugin::PluginError::Panic`]-flavored payload
//!    surfaced via the report's `failed` list.
//! 3. Surface a `PANICKED during {phase}` diagnostic in the sink.
//! 4. NOT prevent other plugins from running in the same lifecycle pass.
//!
//! Resource-leak interactions during panics live in `resource_leak.rs`.

use std::sync::{Arc, Mutex};

use rge_kernel_diagnostics::DiagnosticAggregator;

use super::fixtures::TestPlugin;
use crate::context::PluginContext;
use crate::host::{PluginHost, PluginState};
use crate::plugin::PluginId;

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

/// Audit-6 round-6 M4 closure — panic during put-back path.
///
/// Stages a `u32` resource. Plugin's `tick()` takes the `u32` from ctx,
/// then panics BEFORE calling `ctx.insert()` to put it back. The value
/// lives on the panicking stack frame; the panic unwinds and the value is
/// dropped (unrecoverable).
///
/// Asserts:
/// 1. The host's `catch_unwind` shield catches the panic — `tick_all` does
///    not propagate to the caller (no `should_panic` test).
/// 2. The plugin is marked [`PluginState::Failed`].
/// 3. The panic diagnostic is emitted (`"PANICKED during tick"`).
/// 4. **The leaked-resource diagnostic ALSO fires** because the
///    pre/post-snapshot diff detects the missing `u32` slot — this is
///    the load-bearing claim of M4: even when the panic occurs MID-resource-handoff
///    rather than at the start of tick, leak detection still works
///    correctly. ChatGPT cross-review framing: "rare but catastrophic
///    corruption class" — closes the test-coverage gap that M4 actually
///    pointed at.
#[test]
fn tick_all_recovers_from_panic_after_resource_take_with_leak_detection() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("midput"),
        Box::new(TestPlugin::new("midput", log.clone()).with_panic_after_resource_take_in_tick()),
    )
    .expect("register midput");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_tick_diag_count = diags.len();
    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        // Stage the u32 directly into THIS ctx (PluginContext registry is
        // per-call; resources don't survive between ctx instances per the
        // existing leak-detection test pattern).
        ctx.insert::<u32>(42_u32);
        assert!(
            ctx.contains::<u32>(),
            "u32 resource must be staged pre-tick"
        );
        let report = host.tick_all(&mut ctx);
        // Resource missing post-tick: the value was dropped on the
        // panicking stack frame; leak detection should report this.
        assert!(
            !ctx.contains::<u32>(),
            "u32 resource must be missing post-panic (lost on panicking stack frame)"
        );
        report
    };

    // Plugin must be Failed (catch_unwind caught + marked).
    assert_eq!(
        host.state(&PluginId::new("midput")),
        Some(PluginState::Failed),
    );
    assert_eq!(
        report.failed.len(),
        1,
        "exactly one failure expected; report: {report:?}"
    );
    assert_eq!(report.ticked, 0);

    // Both diagnostics fire: panic + leak.
    let new_messages: Vec<&str> = diags
        .iter()
        .skip(pre_tick_diag_count)
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        new_messages
            .iter()
            .any(|m| m.contains("PANICKED during tick") && m.contains("plugin midput")),
        "expected PANICKED-during-tick diagnostic for midput; got {new_messages:?}",
    );
    assert!(
        new_messages
            .iter()
            .any(|m| m.contains("leaked") && m.contains("midput")),
        "expected leak-detection diagnostic for midput; got {new_messages:?}",
    );
}
