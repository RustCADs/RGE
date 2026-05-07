//! Registration / state-inspection / iteration tests.
//!
//! Sub-module of [`crate::host::host_tests`]; covers the `register` /
//! `state` / `iter_ids` / `count` surface of [`crate::host::PluginHost`]
//! plus the `IdMismatch` / `DuplicateId` validation paths.

use std::sync::{Arc, Mutex};

use super::fixtures::{LyingPlugin, TestPlugin};
use crate::host::{PluginHost, PluginHostError, PluginState};
use crate::plugin::PluginId;

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
fn state_returns_current_plugin_state() {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use crate::context::PluginContext;

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
