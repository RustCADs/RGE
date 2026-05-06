//! [`EventHooks`] — advisory event-bus subscription tracking for scripts.
//!
//! Wasmtime host-function wiring for `rge.event.emit` / `rge.event.subscribe`
//! is deferred to Phase 4-Foundation. This module provides the subscription
//! tracker so the API shape is stable for the Phase 3.3 prototype.

use rge_kernel_events::{EventBus, SubscriptionId};

// ---------------------------------------------------------------------------
// EventHooks
// ---------------------------------------------------------------------------

/// Tracks event-bus subscriptions held on behalf of a running script instance.
///
/// Subscriptions are auto-cleared when [`unsubscribe_all`] is called or when
/// the instance is dropped. The wasmtime host-function wiring that lets scripts
/// call `bus.subscribe` / `bus.emit` directly is a **Phase 4-Foundation**
/// extension.
///
/// [`unsubscribe_all`]: Self::unsubscribe_all
#[derive(Debug, Default)]
pub struct EventHooks {
    subs: Vec<SubscriptionId>,
}

impl EventHooks {
    /// Construct an empty hooks tracker.
    #[must_use]
    pub fn new() -> Self {
        Self { subs: Vec::new() }
    }

    /// Subscribe to events of type `E` on behalf of the script.
    ///
    /// Returns the [`SubscriptionId`] so the caller can track it.
    pub fn subscribe<E: Send + 'static>(&mut self, bus: &mut EventBus) -> SubscriptionId {
        let id = bus.subscribe::<E>();
        self.subs.push(id);
        id
    }

    /// Remove all subscriptions registered through this hook tracker.
    pub fn unsubscribe_all(&mut self, bus: &mut EventBus) {
        for id in self.subs.drain(..) {
            bus.unsubscribe(id);
        }
    }

    /// Number of active subscriptions.
    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subs.len()
    }
}
