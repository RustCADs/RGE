//! `rge-script-host` — WASM script host per IMPLEMENTATION.md Phase 3.2.
//!
//! Failure class: plugin-fatal
//!
//! ECS bridge + event hooks + state-preserving instance swap. This is the
//! "very small initially" version: direct host-function exposure via
//! [`wasmtime::Linker`], NOT yet a full WIT component-model bridge (Phase
//! 4-Foundation extension).
//!
//! # Safety policy
//!
//! `unsafe_code` is set to `deny` at the crate level (overriding the workspace
//! `forbid`) so that each unavoidable `unsafe` site in [`host_state`] can be
//! enabled locally with `#[allow(unsafe_code)]` alongside a `// SAFETY:` proof
//! comment. All other modules are safe Rust.
//!
//! The sole `unsafe` concern is the call-scope pattern in [`host_state`]:
//! raw pointers to `World`, `EventBus`, and `DiagnosticAggregator` are
//! installed before a wasm tick call and cleared afterwards. The proof is in
//! [`host_state::with_call_scope`].
//!
//! # Architecture
//!
//! ```text
//! ScriptModule      — compiled .wasm bytes + BLAKE3 digest
//!   └─ ScriptInstance — live wasmtime Store<HostState> + Instance
//!        ├─ EcsBridge  — host fns: entity_count / spawn / despawn / Counter
//!        └─ EventHooks — advisory subscription tracking (wiring Phase 4)
//!
//! swap::capture_state  — snapshot Counter components to RON
//! swap::restore_state  — re-insert snapshots after new instance loads
//! ```
//!
//! # Limitations (prototype scope)
//!
//! - Component bridge is hard-coded for `Counter(i64)`. Generic type-erased
//!   component access requires archetype iteration changes in `kernel/ecs`
//!   (Phase 4-Foundation).
//! - Event host-function wiring (`rge.event.emit`) is deferred to Phase 4.
//! - Full WIT component-model bridge (`rge:ecs/query`, `rge:ecs/observer`,
//!   `rge:asset/view`) is Phase 4-Foundation.
//! - `find_entity_by_handle` in [`ecs_bridge`] scans only Counter-bearing
//!   entities; entities without Counter cannot be found by handle yet.

#![warn(missing_docs)]

pub mod ecs_bridge;
pub mod event_hooks;
pub mod host_state;
pub mod script_module;
pub mod swap;

pub use ecs_bridge::EcsBridge;
pub use event_hooks::EventHooks;
pub use host_state::HostState;
pub use script_module::{ScriptError, ScriptInstance, ScriptModule};
pub use swap::{capture_state, restore_state, SwapError, SwapPlan, SwapResult};
