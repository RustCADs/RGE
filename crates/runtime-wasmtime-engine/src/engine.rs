// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! Wasmtime engine — compile + instantiate `.wasm` modules under a
//! cap-gate from the [`rge-runtime-wasmtime`] crate.
//!
//! Phase 3 critical path: this is the first wave that actually pulls
//! wasmtime + cranelift. The engine is wrapped behind the
//! [`engine_wasmtime`] feature flag (default-on as of W04) so the
//! escape clause from PLAN.md §1.4 can still re-disable it without
//! re-architecting the cap-gate API.

use std::sync::{Arc, Mutex};

use rge_runtime_wasmtime::{grant_check, CapSet, EffectSet, GrantError, HostState, LoadedPlugin};
use wasmtime::{Caller, Engine as WasmtimeEngine, Linker, Module, Store};

use crate::instance::Instance;
use crate::panic_recovery::{PanicRegistry, PanicReport};

/// Errors raised by the engine during compile / instantiate / call.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// wasmtime compile failed (malformed wasm, validation error).
    #[error("wasmtime compile failed: {0}")]
    Compile(String),

    /// Linker missing import — typically a cap-gated host function the
    /// plugin tried to import without declaring the matching effect.
    #[error("linker missing import: {0}")]
    LinkerMissing(String),

    /// Capability gate rejected the instantiate.
    #[error("capability gate: {0}")]
    CapabilityGate(#[from] GrantError),

    /// The plugin tried to call a host function whose effect-set
    /// requirement was not declared in the plugin manifest.
    #[error(
        "plugin called host function `{name}` requiring `{effect_tag}` but did not declare it"
    )]
    UndeclaredEffect {
        /// Host-function name the plugin tried to call.
        name: String,
        /// Effect-tag the host function requires.
        effect_tag: &'static str,
    },

    /// Instance call trapped (panic, division-by-zero, OOM, ...).
    #[error("wasm trap: {0}")]
    Trap(String),

    /// Generic wasmtime error wrap.
    #[error("wasmtime error: {0}")]
    Wasmtime(String),
}

impl From<wasmtime::Error> for EngineError {
    fn from(e: wasmtime::Error) -> Self {
        EngineError::Wasmtime(e.to_string())
    }
}

/// The W04 wasmtime engine. Holds a single `wasmtime::Engine` per
/// editor session and a `PanicRegistry` so traps can be diagnosed
/// after the instance is quarantined.
pub struct Engine {
    inner: WasmtimeEngine,
    panics: Arc<Mutex<PanicRegistry>>,
}

impl core::fmt::Debug for Engine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Engine")
            .field(
                "panics_recorded",
                &self.panics.lock().map_or(0, |p| p.len()),
            )
            .finish_non_exhaustive()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new().expect("default wasmtime config valid")
    }
}

impl Engine {
    /// Construct a new engine with the W04-default wasmtime config:
    /// Cranelift JIT, no async, no fuel (fuel landing in script-host
    /// per PLAN.md §5.5).
    ///
    /// # Errors
    /// Returns `EngineError::Wasmtime` if wasmtime rejects the config.
    pub fn new() -> Result<Self, EngineError> {
        let mut cfg = wasmtime::Config::new();
        cfg.cranelift_opt_level(wasmtime::OptLevel::Speed);
        cfg.consume_fuel(false);
        cfg.epoch_interruption(false);
        // Wasmtime 23+ wraps panics in traps by default; W04 defers
        // async / multi-memory until script-host (Phase 3.2).
        let inner = WasmtimeEngine::new(&cfg).map_err(|e| EngineError::Wasmtime(e.to_string()))?;
        Ok(Self {
            inner,
            panics: Arc::new(Mutex::new(PanicRegistry::default())),
        })
    }

    /// Compile a `.wasm` blob into a wasmtime [`Module`].
    ///
    /// # Errors
    /// Returns `EngineError::Compile` on malformed wasm or validation failure.
    pub fn compile(&self, bytes: &[u8]) -> Result<Module, EngineError> {
        Module::new(&self.inner, bytes).map_err(|e| EngineError::Compile(e.to_string()))
    }

    /// Instantiate a compiled module under a cap ticket. The runtime
    /// gate (Path B) runs **before** wasmtime even compiles — a
    /// `<reads-phi>`-declaring plugin against a `compute.exec`-only
    /// runtime is rejected without any cranelift work.
    ///
    /// The host function set the engine binds is gated by the plugin's
    /// declared effect set:
    ///
    /// - `host_record_tick(f32)` always available — exercises
    ///   `<computes>` only.
    /// - `host_random()` only linked when `<varies>` declared.
    /// - `host_log_audit()` only linked when `<transacts>` declared.
    /// - `host_read_phi(...)` only linked when `<reads-phi>` declared.
    /// - `wasi_sockets_tcp_connect(...)` only linked when `<network>`
    ///   declared.
    ///
    /// A plugin that imports `wasi:sockets/tcp` without declaring
    /// `<network>` therefore fails at link time with
    /// `EngineError::LinkerMissing` — that's the W04 cap-gate test.
    ///
    /// # Errors
    /// - `EngineError::CapabilityGate` when the plugin's declared effects
    ///   require capabilities the runtime was not granted (Path B gate).
    /// - `EngineError::Compile` if wasmtime rejects the bytes.
    /// - `EngineError::LinkerMissing` if the plugin imports a host
    ///   function whose effect was not declared (cap-gate at link time).
    pub fn instantiate(
        &self,
        loaded: &LoadedPlugin,
        granted: CapSet,
    ) -> Result<Instance, EngineError> {
        // Path B runtime cap-gate.
        grant_check(loaded.effects, granted)?;

        let module = self.compile(&loaded.bytes)?;

        let host = HostState::new(granted, loaded.plugin_id);
        let mut store = Store::new(&self.inner, host);
        let mut linker: Linker<HostState> = Linker::new(&self.inner);

        bind_host_functions(&mut linker, loaded.effects)?;

        let inst = linker
            .instantiate(&mut store, &module)
            .map_err(|e| classify_link_error(e, loaded.effects))?;

        Ok(Instance::new(
            inst,
            store,
            loaded.plugin_id,
            self.panics.clone(),
        ))
    }

    /// Read all panic reports recorded since engine construction.
    /// Cleared after read so the editor can re-render without dupes.
    ///
    /// # Panics
    ///
    /// Panics if the internal panic-registry mutex was poisoned by a
    /// previous panic in `bind_host_functions` / `instantiate`. Such
    /// poisoning surfaces a hard substrate fault that cannot recover
    /// by any policy short of shutting the runtime; the panic here is
    /// the right shape because the engine is already in an
    /// unrecoverable state.
    #[must_use]
    pub fn drain_panics(&self) -> Vec<PanicReport> {
        let mut g = self.panics.lock().expect("panic registry poisoned");
        g.drain()
    }

    /// Inner wasmtime engine (for advanced consumers).
    #[must_use]
    pub fn inner(&self) -> &WasmtimeEngine {
        &self.inner
    }
}

/// Wire host-function imports into the linker according to the
/// plugin's declared effects. Each effect group is bound only when
/// the corresponding effect bit is set in `effects`. **This is the
/// host-side cap-gate enforcement at link time** — see W04 spec
/// "Cap ticket enforcement at host-function call sites".
fn bind_host_functions(
    linker: &mut Linker<HostState>,
    effects: EffectSet,
) -> Result<(), EngineError> {
    use rge_runtime_wasmtime::Effect;

    // <computes> — always available. Per W04 spec: cap-ticket
    // enforcement at host-function call sites — the closure verifies
    // the host's CapSet covers ComputeExec before recording the tick.
    // (Defence-in-depth: the link-time gate above already excludes
    // plugins that didn't declare <computes>; this re-check guards
    // against host_state.caps being mutated between instantiate and
    // call, e.g. by a future hot-reload swap that re-issues a smaller
    // grant.)
    linker
        .func_wrap(
            "host",
            "host_record_tick",
            |mut caller: Caller<'_, HostState>, dt: f32| -> Result<(), wasmtime::Error> {
                let caps = caller.data().caps;
                if !caps.contains(rge_runtime_wasmtime::Capability::ComputeExec) {
                    return Err(wasmtime::Error::msg(
                        "host_record_tick called without compute.exec cap",
                    ));
                }
                caller.data_mut().record_tick(dt);
                Ok(())
            },
        )
        .map_err(|e| EngineError::Wasmtime(e.to_string()))?;

    // <varies> — only linked when declared.
    if effects.contains(Effect::Varies) {
        linker
            .func_wrap(
                "host",
                "host_random",
                |caller: Caller<'_, HostState>| -> Result<f32, wasmtime::Error> {
                    let caps = caller.data().caps;
                    if !caps.contains(rge_runtime_wasmtime::Capability::EntropyRead) {
                        return Err(wasmtime::Error::msg(
                            "host_random called without entropy.read cap",
                        ));
                    }
                    Ok(0.5) // Stub: real impl wires kernel RNG.
                },
            )
            .map_err(|e| EngineError::Wasmtime(e.to_string()))?;
    }

    // <transacts> — only linked when declared.
    if effects.contains(Effect::Transacts) {
        linker
            .func_wrap(
                "host",
                "host_log_audit",
                |mut caller: Caller<'_, HostState>, code: i32| -> Result<(), wasmtime::Error> {
                    let caps = caller.data().caps;
                    if !caps.contains(rge_runtime_wasmtime::Capability::IrWrite) {
                        return Err(wasmtime::Error::msg(
                            "host_log_audit called without ir.write cap",
                        ));
                    }
                    caller
                        .data_mut()
                        .audit(Effect::Transacts, format!("audit code {code}"));
                    Ok(())
                },
            )
            .map_err(|e| EngineError::Wasmtime(e.to_string()))?;
    }

    // <reads-phi> — only linked when declared.
    if effects.contains(Effect::ReadsPhi) {
        linker
            .func_wrap(
                "host",
                "host_read_phi",
                |mut caller: Caller<'_, HostState>,
                 _field_id: i32|
                 -> Result<i32, wasmtime::Error> {
                    let caps = caller.data().caps;
                    if !caps.contains(rge_runtime_wasmtime::Capability::PhiRead) {
                        return Err(wasmtime::Error::msg(
                            "host_read_phi called without phi.read cap",
                        ));
                    }
                    caller.data_mut().audit(Effect::ReadsPhi, "phi read");
                    Ok(0)
                },
            )
            .map_err(|e| EngineError::Wasmtime(e.to_string()))?;
    }

    // <network> — only linked when declared. The W04 cap-gate test
    // module imports `wasi:sockets/tcp` without declaring <network>;
    // because this branch never fires, the import is unresolved and
    // the linker fails at instantiate time.
    if effects.contains(Effect::Network) {
        linker
            .func_wrap(
                "wasi:sockets/tcp",
                "connect",
                |caller: Caller<'_, HostState>,
                 _host_ptr: i32,
                 _port: i32|
                 -> Result<i32, wasmtime::Error> {
                    let caps = caller.data().caps;
                    if !caps.contains(rge_runtime_wasmtime::Capability::NetworkOutbound) {
                        return Err(wasmtime::Error::msg(
                            "wasi:sockets/tcp.connect called without network.outbound cap",
                        ));
                    }
                    Ok(0) // Stub: real impl wires kernel/io-scheduler.
                },
            )
            .map_err(|e| EngineError::Wasmtime(e.to_string()))?;
    }

    Ok(())
}

/// Try to map a link-error (unknown import) into a cap-gate diagnostic
/// when the missing import looks like a network/phi/transacts host fn.
#[allow(
    clippy::needless_pass_by_value,
    reason = "owned `wasmtime::Error` matches the call-site shape (`classify_link_error(e, effects)` after a `?` short-circuit) — switching to `&wasmtime::Error` would force `&e` everywhere without functional benefit; the body's `Display`-via-`to_string` reads the same either way"
)]
fn classify_link_error(e: wasmtime::Error, _effects: EffectSet) -> EngineError {
    let msg = e.to_string();
    if msg.contains("unknown import") || msg.contains("incompatible import") {
        EngineError::LinkerMissing(msg)
    } else {
        EngineError::Wasmtime(msg)
    }
}
