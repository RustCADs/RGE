// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! A live wasmtime instance with a typed `tick(dt: f32)` export.
//!
//! After a wasm trap (panic, OOM, divide-by-zero, ...) the instance
//! is **quarantined** — `tick()` returns immediately with the recorded
//! panic. The editor continues running. This is the W04 deliverable
//! "Panic recovery (trap → diagnostic; instance quarantined)".

use std::sync::{Arc, Mutex};

use rge_runtime_wasmtime::HostState;
use wasmtime::{Instance as WasmtimeInstance, Store, TypedFunc};

use crate::engine::EngineError;
use crate::panic_recovery::{PanicRegistry, PanicReport};

/// A running wasm instance bound to a `HostState`.
pub struct Instance {
    inst: WasmtimeInstance,
    store: Store<HostState>,
    plugin_id: blake3::Hash,
    quarantined: Option<PanicReport>,
    panics: Arc<Mutex<PanicRegistry>>,
}

impl core::fmt::Debug for Instance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Instance")
            .field("plugin_id", &self.plugin_id)
            .field("quarantined", &self.quarantined.is_some())
            .field("tick_counter", &self.store.data().tick_counter)
            .finish_non_exhaustive()
    }
}

impl Instance {
    pub(crate) fn new(
        inst: WasmtimeInstance,
        store: Store<HostState>,
        plugin_id: blake3::Hash,
        panics: Arc<Mutex<PanicRegistry>>,
    ) -> Self {
        Self {
            inst,
            store,
            plugin_id,
            quarantined: None,
            panics,
        }
    }

    /// Plugin id (blake3 of the .wasm bytes).
    #[must_use]
    pub fn plugin_id(&self) -> blake3::Hash {
        self.plugin_id
    }

    /// Read-only access to the host state for assertion in tests.
    #[must_use]
    pub fn host(&self) -> &HostState {
        self.store.data()
    }

    /// Mutable access to the host state for tests / direct hooks.
    pub fn host_mut(&mut self) -> &mut HostState {
        self.store.data_mut()
    }

    /// Tick counter on the host side.
    #[must_use]
    pub fn tick_count(&self) -> u32 {
        self.store.data().tick_counter
    }

    /// True iff the instance has been quarantined after a wasm trap.
    /// Quarantined instances ignore further `tick()` calls and surface
    /// the original panic report on inspection.
    #[must_use]
    pub fn is_quarantined(&self) -> bool {
        self.quarantined.is_some()
    }

    /// Recorded panic report, if any.
    #[must_use]
    pub fn panic_report(&self) -> Option<&PanicReport> {
        self.quarantined.as_ref()
    }

    /// Invoke the wasm `tick(dt: f32)` export. On wasm trap the
    /// instance is quarantined; subsequent calls become no-ops.
    ///
    /// # Errors
    /// - `EngineError::Trap` if the wasm code traps (panic, OOM,
    ///   divide-by-zero) or if the instance has already been quarantined.
    /// - `EngineError::LinkerMissing` if the module does not export a
    ///   `tick(f32)` function.
    pub fn tick(&mut self, dt: f32) -> Result<(), EngineError> {
        if let Some(report) = &self.quarantined {
            return Err(EngineError::Trap(format!(
                "instance quarantined after trap: {}",
                report.message
            )));
        }
        let func: TypedFunc<f32, ()> = self
            .inst
            .get_typed_func::<f32, ()>(&mut self.store, "tick")
            .map_err(|e| EngineError::LinkerMissing(format!("export `tick(f32)`: {e}")))?;
        match func.call(&mut self.store, dt) {
            Ok(()) => Ok(()),
            Err(e) => {
                let report = PanicReport {
                    plugin_id: self.plugin_id,
                    message: e.to_string(),
                };
                self.quarantined = Some(report.clone());
                if let Ok(mut g) = self.panics.lock() {
                    g.push(report.clone());
                }
                Err(EngineError::Trap(report.message))
            }
        }
    }

    /// Approximate per-instance memory footprint in bytes — sum of
    /// all exported memories' current size. Used by the W04 baseline
    /// timings in BASELINE.md.
    pub fn memory_footprint_bytes(&mut self) -> usize {
        let Some(mem) = self.inst.get_memory(&mut self.store, "memory") else {
            return 0;
        };
        mem.data_size(&self.store)
    }
}
