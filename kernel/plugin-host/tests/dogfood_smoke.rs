//! §10.4 dogfood-rule contract foundation.
//!
//! v0 verifies the [`Plugin`] trait + lifecycle work end-to-end. Future
//! dispatches add real Tier-2 plugin impls (gfx, physics, editor-ui,
//! cad-projection) — the smoke structure here stays the same; only the
//! plugin types change.

use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_plugin_host::{
    Plugin, PluginContext, PluginError, PluginHost, PluginId, PluginState,
};

/// Dummy Tier-2-like plugin for the contract test. A real `gfx` plugin
/// would have the same shape — just with renderer-init logic in `init()`.
#[derive(Default)]
struct TestTier2Plugin {
    id: String,
    init_calls: u32,
    tick_calls: u32,
    shutdown_calls: u32,
}

impl TestTier2Plugin {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Self::default()
        }
    }
}

impl Plugin for TestTier2Plugin {
    fn id(&self) -> PluginId {
        PluginId::new(&self.id)
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.init_calls += 1;
        Ok(())
    }

    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.tick_calls += 1;
        Ok(())
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.shutdown_calls += 1;
        Ok(())
    }
}

#[test]
fn dogfood_full_lifecycle_runs_to_completion() {
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    let mut host = PluginHost::new();

    let plugin_id = PluginId::new("rge.test-tier2");
    host.register(
        plugin_id.clone(),
        Box::new(TestTier2Plugin::new("rge.test-tier2")),
    )
    .expect("register");
    assert_eq!(host.state(&plugin_id), Some(PluginState::Pending));

    let init_report = host.init_all(&mut ctx);
    assert_eq!(init_report.initialized.len(), 1);
    assert!(init_report.failed.is_empty());
    assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(tick_report.ticked, 1);
    assert!(tick_report.failed.is_empty());

    // Tick again to demonstrate repeat ticks.
    let _tick_report_2 = host.tick_all(&mut ctx);

    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(shutdown_report.shutdown.len(), 1);
    assert!(shutdown_report.failed.is_empty());
    assert_eq!(host.count(), 0);

    // Diagnostics sink saw nothing — TestTier2Plugin doesn't emit any.
    assert!(diags.is_empty());
}

#[test]
fn dogfood_three_plugins_lifecycle_lifo() {
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);
    let mut host = PluginHost::new();

    let id_a = PluginId::new("rge.gfx-like");
    let id_b = PluginId::new("rge.physics-like");
    let id_c = PluginId::new("rge.editor-like");

    host.register(id_a.clone(), Box::new(TestTier2Plugin::new("rge.gfx-like")))
        .expect("register a");
    host.register(
        id_b.clone(),
        Box::new(TestTier2Plugin::new("rge.physics-like")),
    )
    .expect("register b");
    host.register(
        id_c.clone(),
        Box::new(TestTier2Plugin::new("rge.editor-like")),
    )
    .expect("register c");

    let init_report = host.init_all(&mut ctx);
    assert_eq!(init_report.initialized.len(), 3);
    assert!(init_report.failed.is_empty());

    // Verify init ran in registration order: a, b, c.
    assert_eq!(init_report.initialized[0], id_a);
    assert_eq!(init_report.initialized[1], id_b);
    assert_eq!(init_report.initialized[2], id_c);

    // All three should be Initialized.
    assert_eq!(host.state(&id_a), Some(PluginState::Initialized));
    assert_eq!(host.state(&id_b), Some(PluginState::Initialized));
    assert_eq!(host.state(&id_c), Some(PluginState::Initialized));

    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(shutdown_report.shutdown.len(), 3);
    assert!(shutdown_report.failed.is_empty());

    // LIFO: shutdown order is reverse of registration order.
    assert_eq!(shutdown_report.shutdown[0], id_c);
    assert_eq!(shutdown_report.shutdown[1], id_b);
    assert_eq!(shutdown_report.shutdown[2], id_a);

    assert_eq!(host.count(), 0);
}
