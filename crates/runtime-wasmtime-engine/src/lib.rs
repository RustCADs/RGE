// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! WASM bytecode execution engine — **Phase 3 critical path**.
//!
//! As of W04 (2026-05-05) the deferred `engine_wasmtime` feature is
//! ON by default; this crate now pulls `wasmtime` + `wit-bindgen` and
//! exposes [`Engine::compile`] / [`Engine::instantiate`] for the
//! constitutional WASM hot-reload bet (PLAN.md §5.1, §5.5).
//!
//! ## Module layout
//!
//! - [`engine`] — wasmtime engine + Path B runtime cap-gate.
//! - [`instance`] — typed `tick(dt: f32)` invocation, quarantine after trap.
//! - [`panic_recovery`] — trap → diagnostic record, drainable from the editor.
//!
//! ## Cap-gate enforcement at host-function call sites
//!
//! [`Engine::instantiate`] binds a different host-function set into
//! the wasmtime [`Linker`](wasmtime::Linker) depending on the plugin's
//! declared effects. A module that imports `wasi:sockets/tcp` without
//! declaring `<network>` fails at instantiate-time with
//! [`EngineError::LinkerMissing`].
//!
//! ## Hot-reload (deferred to Phase 3.3)
//!
//! W04 only proves engine activation — full hot-reload p95 budget
//! validation lands in W20 / `script-host` (Phase 3.3). If the budget
//! misses by Phase 3.3, ADR-077's escape clause activates.

#![forbid(unsafe_code)]

#[cfg(feature = "engine_wasmtime")]
pub mod engine;
#[cfg(feature = "engine_wasmtime")]
pub mod instance;
#[cfg(feature = "engine_wasmtime")]
pub mod panic_recovery;

#[cfg(feature = "engine_wasmtime")]
pub use engine::{Engine, EngineError};
#[cfg(feature = "engine_wasmtime")]
pub use instance::Instance;
#[cfg(feature = "engine_wasmtime")]
pub use panic_recovery::{PanicRegistry, PanicReport};

/// Crate version, sourced from `Cargo.toml` at compile time.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo_pkg_version() {
        assert!(!version().is_empty());
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[cfg(feature = "engine_wasmtime")]
    #[test]
    fn engine_constructs_with_default_config() {
        let _e = Engine::new().expect("engine constructs");
    }
}
