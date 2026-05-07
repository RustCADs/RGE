//! Unit tests for [`crate::host::PluginHost`].
//!
//! Sub-module of [`crate::host`]; see that module's `//!` docs for the design
//! rationale of the panic-recovery / leak-detection paths these tests cover.
//!
//! # Layout
//!
//! Pre-emptive Phase 5 split (audit-3 carryover): `host.rs` was approaching
//! the 1000-line hard cap once the `RuntimeFault` severity test landed, so the
//! `#[cfg(test)] mod tests` block was extracted here. Each file groups tests
//! by the orchestrator-side concern they exercise:
//!
//! | sub-module       | what it covers                                       |
//! |------------------|------------------------------------------------------|
//! | `fixtures`       | [`TestPlugin`] behavior matrix + `LyingPlugin`       |
//! | `registration`   | `register` / `state` / `iter_ids` / empty-host       |
//! | `lifecycle`      | `init_all` / `tick_all` / `shutdown_all` / `unregister` happy paths |
//! | `diagnostics`    | auto-emit policy (Pairing-5) + severity discrimination (audit-2 A5.1) |
//! | `panic_recovery` | `catch_unwind` per-phase recovery (audit-2 A5.1)     |
//! | `resource_leak`  | TypeId-snapshot diff per-phase leak detection        |
//!
//! All test files import via `use super::super::*;` (= `crate::host::*`) for
//! [`PluginHost`] / [`PluginState`] / [`PluginHostError`] etc., plus
//! `super::fixtures::*` for the [`TestPlugin`] fixture.

mod diagnostics;
mod fixtures;
mod lifecycle;
mod panic_recovery;
mod registration;
mod resource_leak;
