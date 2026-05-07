// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! `rge-runtime-wasmtime` — Wasmtime cap-gate API: effect specifiers, capability tickets, host
//! state, hand-rolled `.wasm` header validator.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: cap-gate validation failures (header validator rejects
//! malformed magic, `grant_check` denies a capability, manifest parse error,
//! linker missing import) are transient and recoverable in-place — the
//! caller refuses to instantiate the offending plugin and surfaces a
//! diagnostic. The actual bytecode-execution risk (traps, plugin isolation)
//! lives in the sibling `rge-runtime-wasmtime-engine` (plugin-fatal). This
//! crate is engine-independent; no PIE state is owned. Matches pak-format +
//! io-image (validation / format-adapter failures).
//!
//! This crate is
//! **engine-independent** — the actual bytecode execution lives in the
//! sibling [`rge-runtime-wasmtime-engine`](../runtime-wasmtime-engine)
//! crate (W04 activates the `engine_wasmtime` feature there).
//!
//! ## Two cap-gate paths
//!
//! ### Path A — In-process Rust plugins (compile-time gate)
//!
//! [`effect_specifier::Plugin<EFFECTS>`] + [`effect_specifier::CapTicket<CAPS>`]
//! const-generic typestate. [`assert_compile_time_gate!`] surfaces a
//! missing capability as a `cargo check` error.
//!
//! ### Path B — Dynamic `.wasm` plugins (runtime gate)
//!
//! [`runtime::WasmRuntime::instantiate`] runs [`effect_specifier::grant_check`]
//! at instantiation time using the manifest scanned from the `.wasm`
//! blob. Same predicate as Path A, fired at runtime because the
//! const-generic mask is not known until the blob is parsed.
//!
//! ## What ships in W04
//!
//! - The cap-gate API (this crate) — engine-independent.
//! - The `engine_wasmtime` feature flipped in
//!   [`rge-runtime-wasmtime-engine`](../runtime-wasmtime-engine), which
//!   pulls `wasmtime` + `wat` and exposes `Engine::compile` /
//!   `Engine::instantiate` plus the W04 hello-world tick test.

#![forbid(unsafe_code)]

pub mod cap_ticket;
pub mod effect_specifier;
pub mod host;
pub mod runtime;

// Convenient prelude.
pub use cap_ticket::DynCapTicket;
pub use effect_specifier::{
    cap_set_satisfies, effect_set_subset, grant_check, BoundPlugin, CapMarker, CapSet, CapTicket,
    Capability, Effect, EffectSet, GrantError, Plugin,
};
pub use host::{short_hash, HostState};
pub use runtime::{
    bind_plugin, cap_report, load_wasm_blob, LoadError, LoadedPlugin, WasmRuntime,
    WASM_HEADER_BYTES, WASM_MAGIC, WASM_VERSION_COMPONENT_PREVIEW, WASM_VERSION_V1,
};
