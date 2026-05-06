//! [`ScriptModule`] and [`ScriptInstance`] — compiled module + live instance.

use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::World;
use rge_kernel_events::EventBus;
use wasmtime::{Engine, Instance as WasmInstance, Linker, Module, Store, TypedFunc};

use crate::ecs_bridge::EcsBridge;
use crate::host_state::{with_call_scope, HostState};

// ---------------------------------------------------------------------------
// ScriptError
// ---------------------------------------------------------------------------

/// Errors that a script operation can produce.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The wasm bytes failed validation or compilation.
    #[error("compile failed: {0}")]
    Compile(String),
    /// Instantiation (linker + host-function binding) failed.
    #[error("instantiate failed: {0}")]
    Instantiate(String),
    /// The module's exported `tick(f32)` trapped.
    #[error("tick trapped: {0}")]
    TickTrap(String),
    /// The module does not export the required function.
    #[error("module missing required export `{0}`")]
    MissingExport(&'static str),
}

// ---------------------------------------------------------------------------
// ScriptModule — compiled, not yet instantiated
// ---------------------------------------------------------------------------

/// A compiled but not-yet-instantiated WASM module.
///
/// Holds the compiled [`wasmtime::Module`] plus a BLAKE3 digest of the
/// original bytes. The digest is used for change-detection during hot-reload
/// (same digest → skip re-instantiation).
pub struct ScriptModule {
    inner: Module,
    digest: [u8; 32],
    name: String,
}

impl ScriptModule {
    /// Compile `bytes` into a [`ScriptModule`].
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::Compile`] if wasmtime rejects the bytes.
    pub fn from_bytes(
        engine: &Engine,
        name: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, ScriptError> {
        let digest = *blake3::hash(bytes).as_bytes();
        let inner = Module::new(engine, bytes).map_err(|e| ScriptError::Compile(e.to_string()))?;
        Ok(Self {
            inner,
            digest,
            name: name.into(),
        })
    }

    /// BLAKE3 digest of the original wasm bytes.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Human-readable name (e.g. plugin file path).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The compiled wasmtime module (crate-internal).
    pub(crate) fn inner(&self) -> &Module {
        &self.inner
    }
}

impl std::fmt::Debug for ScriptModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptModule")
            .field("name", &self.name)
            .field("digest", &hex_digest(&self.digest))
            .finish_non_exhaustive()
    }
}

fn hex_digest(d: &[u8; 32]) -> String {
    d.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------------------
// ScriptInstance — live instance with host-function bindings
// ---------------------------------------------------------------------------

/// A live script instance — owns a wasmtime `Store<HostState>` + `Instance`.
///
/// Instantiated from a [`ScriptModule`] via [`ScriptInstance::instantiate`].
/// The instance exposes a single wasm export: `tick(dt: f32)`.
pub struct ScriptInstance {
    instance: WasmInstance,
    store: Store<HostState>,
    tick_fn: TypedFunc<f32, ()>,
}

impl ScriptInstance {
    /// Instantiate `module`, wiring the ECS bridge host functions.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::Instantiate`] or [`ScriptError::MissingExport`]
    /// on failure.
    pub fn instantiate(engine: &Engine, module: &ScriptModule) -> Result<Self, ScriptError> {
        let host = HostState::new();
        let mut store = Store::new(engine, host);
        let mut linker: Linker<HostState> = Linker::new(engine);

        EcsBridge::install(&mut linker)
            .map_err(|e| ScriptError::Instantiate(format!("linker setup: {e}")))?;

        let instance = linker
            .instantiate(&mut store, module.inner())
            .map_err(|e| ScriptError::Instantiate(e.to_string()))?;

        let tick_fn = instance
            .get_typed_func::<f32, ()>(&mut store, "tick")
            .map_err(|_| ScriptError::MissingExport("tick(f32)"))?;

        Ok(Self {
            instance,
            store,
            tick_fn,
        })
    }

    /// Call the module's exported `tick(dt: f32)` function.
    ///
    /// Installs `world`, `events`, and `diagnostics` into the [`HostState`]
    /// for the duration of the call, then clears them before returning.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::TickTrap`] if the wasm code traps.
    pub fn tick(
        &mut self,
        dt: f32,
        world: &mut World,
        events: &mut EventBus,
        diagnostics: &mut DiagnosticAggregator,
    ) -> Result<(), ScriptError> {
        // Split the store borrow: get a raw pointer to HostState for
        // with_call_scope, then call tick_fn with &mut self.store.
        // SAFETY: `state_ptr` is derived from `self.store.data_mut()` and
        // points into the store's heap allocation. `with_call_scope` writes
        // to it before the tick call and clears it after. The tick_fn call
        // is synchronous and single-threaded; no aliasing occurs because
        // `state_ptr` only writes to the pointer slots while `tick_fn.call`
        // may only READ those slots via Caller::data_mut inside host fns —
        // the store data allocation is stable for the lifetime of the store.
        #[allow(unsafe_code)]
        let state_ptr: *mut HostState = self.store.data_mut();

        let tick_fn = self.tick_fn.clone();
        let store = &mut self.store;

        with_call_scope(state_ptr, world, diagnostics, events, || {
            tick_fn
                .call(store, dt)
                .map_err(|e| ScriptError::TickTrap(e.to_string()))
        })
    }

    /// Call the wasm module's `init_entity(handle: i64)` export within a tick scope.
    ///
    /// This is a test-support API: the WAT fixtures expose `init_entity` to let
    /// the host register which entity the module should operate on. Production
    /// modules would use a different configuration mechanism.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::MissingExport`] if the module does not export
    /// `init_entity(i64)`, or [`ScriptError::TickTrap`] if it traps.
    pub fn call_init_entity(
        &mut self,
        handle: i64,
        world: &mut World,
        events: &mut EventBus,
        diagnostics: &mut DiagnosticAggregator,
    ) -> Result<(), ScriptError> {
        let init_fn = self
            .instance
            .get_typed_func::<i64, ()>(&mut self.store, "init_entity")
            .map_err(|_| ScriptError::MissingExport("init_entity(i64)"))?;

        #[allow(unsafe_code)]
        let state_ptr: *mut HostState = self.store.data_mut();
        let store = &mut self.store;

        with_call_scope(state_ptr, world, diagnostics, events, || {
            init_fn
                .call(store, handle)
                .map_err(|e| ScriptError::TickTrap(e.to_string()))
        })
    }

    /// Access the wasmtime instance (for test inspection).
    #[must_use]
    pub fn raw_instance(&self) -> &WasmInstance {
        &self.instance
    }

    /// Access the store (for test inspection).
    #[must_use]
    pub fn store(&self) -> &Store<HostState> {
        &self.store
    }
}

impl std::fmt::Debug for ScriptInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptInstance").finish_non_exhaustive()
    }
}
