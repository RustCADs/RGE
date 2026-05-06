//! [`HostState`] — the `Store<T>` data type for the script host.
//!
//! The wasmtime `Store<HostState>` persists across tick calls, but
//! `&mut World` and `&mut EventBus` cannot live inside the store beyond a
//! single tick because the borrow checker (correctly) forbids aliasing.
//!
//! # Call-scope pattern
//!
//! `HostState` stores raw pointers to `World`, `EventBus`, and
//! `DiagnosticAggregator` that are **only valid during an active `tick()`
//! call**. They are set immediately before calling the wasm export and
//! cleared immediately after (whether the call succeeds or traps). Host
//! functions trap if they find `None` in these slots.
//!
//! # Safety justification
//!
//! The raw pointer pattern is sound because:
//!
//! 1. Pointers are derived from `&mut T` references whose lifetimes are tied
//!    to the current stack frame of `ScriptInstance::tick`.
//! 2. Pointers are set and cleared within [`with_call_scope`], which uses a
//!    `defer`-style cleanup to clear them even on panic/trap.
//! 3. Wasmtime host functions run synchronously inside `func.call(...)` — no
//!    other thread holds the store or the pointed-to values during that window.
//! 4. `HostState` is inside a wasmtime `Store`, which is `!Send` by default,
//!    preventing cross-thread pointer escape.

use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink as _, Severity};
use rge_kernel_ecs::World;
use rge_kernel_events::EventBus;

// ---------------------------------------------------------------------------
// Guard helper — defined before HostState to satisfy items_after_statements
// ---------------------------------------------------------------------------

/// RAII guard for the call scope: clears all raw pointers from [`HostState`]
/// on drop (or panic).
struct CallScopeGuard(*mut HostState);

impl Drop for CallScopeGuard {
    fn drop(&mut self) {
        // SAFETY: pointer is valid for the duration of with_call_scope (same
        // argument as the install step). Cleared here before the &mut
        // lifetimes of world/events/diagnostics end.
        #[allow(unsafe_code)]
        unsafe {
            (*self.0).world_ptr = None;
            (*self.0).diagnostics_ptr = None;
            (*self.0).events_ptr = None;
        }
    }
}

// ---------------------------------------------------------------------------
// HostState
// ---------------------------------------------------------------------------

/// Per-instance data stored in a wasmtime `Store<HostState>`.
///
/// During a `tick()` call the raw pointer fields are populated via
/// [`with_call_scope`]; outside a tick they are `None` and host functions trap.
pub struct HostState {
    /// Non-null only during an active tick call.
    pub(crate) world_ptr: Option<*mut World>,
    /// Non-null only during an active tick call.
    pub(crate) diagnostics_ptr: Option<*mut DiagnosticAggregator>,
    /// Non-null only during an active tick call.
    pub(crate) events_ptr: Option<*mut EventBus>,
    /// Last error written by a host function (non-trapping failure path).
    pub last_error: Option<String>,
}

// SAFETY: HostState lives inside a wasmtime Store which is !Send.
// Raw pointers are only populated for the duration of a synchronous
// host call scoped to the tick() call site; they are never transferred
// across threads.
#[allow(unsafe_code)]
unsafe impl Send for HostState {}

impl HostState {
    /// Construct an empty `HostState` (no active scope).
    #[must_use]
    pub fn new() -> Self {
        Self {
            world_ptr: None,
            diagnostics_ptr: None,
            events_ptr: None,
            last_error: None,
        }
    }

    /// Borrow the world within a tick scope.
    ///
    /// # Panics
    ///
    /// Panics (as a wasm trap) when called outside an active call scope —
    /// this surfaces as a wasm trap and quarantines the instance per the
    /// plugin-fatal contract.
    pub(crate) fn world(&mut self) -> &mut World {
        // SAFETY: pointer is set from a &mut World with lifetime >= the
        // enclosing tick() call. with_call_scope ensures it is cleared
        // before the borrow ends. Wasmtime host fns are synchronous.
        #[allow(unsafe_code)]
        unsafe {
            &mut *self
                .world_ptr
                .expect("world_ptr accessed outside tick scope")
        }
    }

    /// Borrow the diagnostics aggregator within a tick scope.
    ///
    /// # Panics
    ///
    /// Panics when called outside an active call scope.
    pub(crate) fn diagnostics(&mut self) -> &mut DiagnosticAggregator {
        // SAFETY: same proof as world(). Pointer cleared by with_call_scope.
        #[allow(unsafe_code)]
        unsafe {
            &mut *self
                .diagnostics_ptr
                .expect("diagnostics_ptr outside tick scope")
        }
    }

    /// Emit a diagnostic with a given severity (within scope).
    pub(crate) fn emit_severity(&mut self, severity: Severity, message: impl Into<String>) {
        let msg = message.into();
        if self.diagnostics_ptr.is_some() {
            let diag = match severity {
                Severity::Error => Diagnostic::error(&msg),
                Severity::Warning => Diagnostic::warning(&msg),
                Severity::Info => Diagnostic::info(&msg),
                Severity::Suggestion => Diagnostic::suggestion(&msg),
            };
            self.diagnostics().emit(diag);
        } else {
            tracing::warn!(
                "[script-host] out-of-scope diag ({}): {msg}",
                severity.label()
            );
        }
    }
}

impl Default for HostState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// with_call_scope — the ONLY place raw pointers are written
// ---------------------------------------------------------------------------

/// Install call-scope pointers into `state`, run `f`, then clear them.
///
/// This is the only function that writes to the raw-pointer fields of
/// [`HostState`]. It uses a [`CallScopeGuard`] to ensure pointers are cleared
/// even if `f` panics.
///
/// # Safety
///
/// - `world`, `diagnostics`, and `events` must remain alive for the entire
///   execution of `f` (guaranteed by the caller holding `&mut` references).
/// - `state` must not be aliased from another thread while `f` runs (upheld
///   by wasmtime's `!Send` `Store` and the Rust borrow on `&mut World`).
///
/// # Panics
///
/// Does not catch panics from `f`; they propagate normally. The [`CallScopeGuard`]
/// still clears all pointers before the panic unwinds.
pub(crate) fn with_call_scope<F, R>(
    state: *mut HostState,
    world: &mut World,
    diagnostics: &mut DiagnosticAggregator,
    events: &mut EventBus,
    f: F,
) -> R
where
    F: FnOnce() -> R,
{
    // SAFETY: `state` is a raw pointer to a HostState that is live for the
    // duration of this function (caller holds &mut Store<HostState>). The
    // pointers we store point into the caller's stack frame and are valid for
    // the duration of `f`. CallScopeGuard clears them in Drop.
    #[allow(unsafe_code)]
    unsafe {
        (*state).world_ptr = Some(std::ptr::from_mut::<World>(world));
        (*state).diagnostics_ptr = Some(std::ptr::from_mut::<DiagnosticAggregator>(diagnostics));
        (*state).events_ptr = Some(std::ptr::from_mut::<EventBus>(events));
    }

    let _guard = CallScopeGuard(state);

    f()
    // _guard drops here (or on panic), clearing the pointers.
}
