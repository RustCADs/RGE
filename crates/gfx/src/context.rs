//! wgpu Instance + Adapter + Device + Queue — the core GPU context.
//!
//! Initialised synchronously via [`pollster::block_on`]. No window / surface
//! required; this is headless-only substrate.
//!
//! # Pipeline-state-object cache
//!
//! `GfxContext` owns a [`PipelineCache<wgpu::RenderPipeline>`] behind a
//! [`RefCell`] so the existing `*::new(...)` constructors on
//! [`crate::pipeline::TrianglePipeline`] / [`crate::mesh_pipeline::MeshPipeline`]
//! / [`crate::lit_mesh_pipeline::LitMeshPipeline`] can route through it
//! transparently. Identical `(shader, layout, color format, depth state)`
//! inputs across repeated `*::new(...)` calls now return shared
//! `Arc<wgpu::RenderPipeline>` allocations without changing the call-site
//! signature. The cache is `RefCell` (not `Mutex`) — `GfxContext` is
//! single-threaded by construction; no caller holds it across threads.
//!
//! Callers that need an explicit, scoped cache (e.g. to assert miss/hit
//! counts in a test) can continue to use the additive `*::new_cached(...,
//! &mut PipelineCache<wgpu::RenderPipeline>)` constructors against a
//! locally-held cache.

use std::cell::RefCell;

use tracing::debug;

use crate::pso_cache::PipelineCache;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when initialising a [`GfxContext`].
#[derive(Debug, thiserror::Error)]
pub enum GfxContextError {
    /// No GPU adapter was found that satisfies the headless requirements.
    #[error("no compatible GPU adapter found")]
    NoAdapter,

    /// wgpu returned an error when creating the logical device.
    #[error("device request failed: {0}")]
    DeviceRequest(String),
}

// ---------------------------------------------------------------------------
// GfxContext
// ---------------------------------------------------------------------------

/// Core wgpu GPU context: Instance, Adapter, Device, and Queue.
///
/// Initialised headless (no surface). The adapter is selected by wgpu from
/// all available backends; on Windows/Linux this is typically Vulkan, on macOS
/// Metal, with DX12/OpenGL/WebGL as fallbacks.
pub struct GfxContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Process-local PSO memoization cache. `RefCell` so the existing
    /// `*::new(&GfxContext, ...)` constructors can mutably index into it
    /// from behind a shared `&GfxContext` borrow. See the module-level
    /// doc-comment for the threading-model rationale.
    pso_cache: RefCell<PipelineCache<wgpu::RenderPipeline>>,
}

impl GfxContext {
    /// Initialise a headless GPU context.
    ///
    /// Uses [`wgpu::Backends::all()`] so that the best available backend is
    /// selected per platform. Returns [`GfxContextError::NoAdapter`] when no
    /// GPU is present (e.g., a headless CI runner without a virtual GPU) —
    /// callers should check for this and skip GPU-dependent work gracefully.
    ///
    /// # Errors
    ///
    /// - [`GfxContextError::NoAdapter`] — no GPU adapter available.
    /// - [`GfxContextError::DeviceRequest`] — adapter found but device creation failed.
    pub fn new_headless() -> Result<Self, GfxContextError> {
        pollster::block_on(Self::init_async())
    }

    async fn init_async() -> Result<Self, GfxContextError> {
        // wgpu 29: Instance::new takes InstanceDescriptor by value (not ref).
        // InstanceDescriptor has no Default impl; use new_without_display_handle()
        // which fills all fields with safe defaults, then override backends.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        // wgpu 29: request_adapter now returns Result<Adapter, RequestAdapterError>
        // (no longer Option). Map the error to NoAdapter.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| GfxContextError::NoAdapter)?;

        let info = adapter.get_info();
        debug!(
            adapter = %info.name,
            backend = ?info.backend,
            "wgpu adapter selected"
        );

        // wgpu 29: request_device no longer takes a trace path second arg;
        // signature is request_device(desc: &DeviceDescriptor) -> Result<(Device, Queue), ...>.
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e: wgpu::RequestDeviceError| GfxContextError::DeviceRequest(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pso_cache: RefCell::new(PipelineCache::new()),
        })
    }

    /// Borrow the [`wgpu::Device`].
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Borrow the [`wgpu::Queue`].
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Return adapter information (name, backend, driver, etc.).
    ///
    /// Useful for logging / diagnostics — does not need a mutable borrow.
    #[must_use]
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }

    /// Borrow the [`wgpu::Instance`] (rarely needed externally, but available
    /// for surface creation in follow-up surface-integration work).
    #[must_use]
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Borrow the [`wgpu::Adapter`].
    ///
    /// Returns `None` only if the context was constructed in a configuration
    /// that drops the adapter post-init (sub-δ.1.A always returns `Some`,
    /// preserving headless-init behaviour). The accessor exists so
    /// `SurfaceContext::new` can call `Surface::get_default_config(adapter, ...)`
    /// to negotiate the platform's preferred surface format / present mode
    /// without hardcoding an assumption that may not hold across all
    /// driver/OS pairs (Wayland / WebGL2 in particular). Symmetric with
    /// the existing [`GfxContext::instance`] accessor that was already
    /// reserved "for follow-up surface-integration work".
    #[must_use]
    pub fn adapter(&self) -> Option<&wgpu::Adapter> {
        Some(&self.adapter)
    }

    /// Borrow the context-owned PSO cache as a [`RefCell`].
    ///
    /// Returned as `&RefCell<...>` (not `&mut PipelineCache<...>`) so the
    /// existing `*::new(&GfxContext, ...)` constructors can call
    /// [`RefCell::borrow_mut`] from behind a shared `&self` — the whole
    /// point of the interior-mutability wrapper. Callers that hold the
    /// `GfxContext` by `&mut` can equivalently use [`pso_cache_mut`].
    #[must_use]
    pub fn pso_cache(&self) -> &RefCell<PipelineCache<wgpu::RenderPipeline>> {
        &self.pso_cache
    }

    /// Borrow the context-owned PSO cache as `&mut PipelineCache<...>`
    /// when the caller already holds the `GfxContext` mutably.
    ///
    /// Bypasses the `RefCell` runtime borrow check (statically proven
    /// exclusive via `&mut self`). Useful for tests that want to clear
    /// the cache or inspect hit/miss statistics without going through
    /// `borrow_mut()`.
    #[must_use]
    pub fn pso_cache_mut(&mut self) -> &mut PipelineCache<wgpu::RenderPipeline> {
        self.pso_cache.get_mut()
    }
}
