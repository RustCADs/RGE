//! [`EventBus`] — heterogeneous, typed event channel registry.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use rge_kernel_diagnostics::{Diagnostic, DiagnosticSink};

use crate::channel::EventChannel;
use crate::subscription::SubscriptionId;

// ---------------------------------------------------------------------------
// Private type-erased channel abstraction
// ---------------------------------------------------------------------------

/// Private object-safe trait erasing the concrete type parameter of
/// [`EventChannel<E>`] so the bus can iterate heterogeneous channels without
/// knowing their type.
///
/// Only operations that do not require the concrete `E` are exposed here.
/// Typed access goes through the concrete [`ChannelEntry<E>`] via [`Any`].
trait AnyChannel: Any + Send + 'static {
    /// Advance this channel to the next frame. Called by
    /// [`EventBus::advance_frame`].
    fn advance_frame(&mut self);

    /// Number of events that were pending *before* the most recent advance.
    /// Used for diagnostic emission.
    fn pending_len_before_advance(&self) -> usize;

    /// The `std::any::type_name` of the event type, captured at insertion.
    fn type_name(&self) -> &'static str;

    /// Upcast to `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Upcast to `&mut dyn Any` for mutable downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Concrete wrapper stored in the bus's channel map.
///
/// Carries the [`EventChannel<E>`] plus a cached pre-advance pending count
/// (for diagnostics) and the event type's name string.
struct ChannelEntry<E: Clone + Send + 'static> {
    channel: EventChannel<E>,
    /// Snapshot of `channel.pending_len()` captured immediately before the
    /// most recent call to `AnyChannel::advance_frame`.
    pre_advance_pending: usize,
    /// `std::any::type_name::<E>()` captured at construction.
    type_name: &'static str,
}

impl<E: Clone + Send + 'static> ChannelEntry<E> {
    fn new() -> Self {
        Self {
            channel: EventChannel::new(),
            pre_advance_pending: 0,
            type_name: std::any::type_name::<E>(),
        }
    }
}

impl<E: Clone + Send + 'static> AnyChannel for ChannelEntry<E> {
    fn advance_frame(&mut self) {
        self.pre_advance_pending = self.channel.pending_len();
        self.channel.advance_frame();
    }

    fn pending_len_before_advance(&self) -> usize {
        self.pre_advance_pending
    }

    fn type_name(&self) -> &'static str {
        self.type_name
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// Heterogeneous, typed event channel registry.
///
/// The bus owns one [`EventChannel<E>`] per event type (keyed by [`TypeId`]).
/// Channels are created lazily on first emit or channel access. Subscriptions
/// are advisory only — the bus does **not** invoke callbacks. Consumers call
/// [`channel`] or [`channel_mut`] and iterate the delivered events themselves
/// each frame.
///
/// # Frame lifecycle
///
/// ```text
/// [systems emit events]  →  bus.emit::<E>(event)         // queued in pending
/// [frame boundary]       →  bus.advance_frame(&mut sink)  // pending → delivered
/// [systems read events]  →  bus.channel::<E>()?.iter_current()
/// ```
///
/// [`channel`]: Self::channel
/// [`channel_mut`]: Self::channel_mut
pub struct EventBus {
    /// Type-erased channel storage, one entry per distinct event type.
    channels: HashMap<TypeId, Box<dyn AnyChannel>>,
    /// Monotonic counter for issuing [`SubscriptionId`]s.
    next_subscription: u64,
    /// Subscription tracking per event type (advisory, not functional).
    subscriptions: HashMap<TypeId, Vec<SubscriptionId>>,
    /// Current frame index; mirrors the per-channel frame counters.
    frame: u64,
}

impl EventBus {
    /// Construct an empty bus at frame 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            next_subscription: 0,
            subscriptions: HashMap::new(),
            frame: 0,
        }
    }

    /// Emit one event of type `E`, creating a channel for `E` if none exists.
    ///
    /// The event will not be visible to consumers until the next call to
    /// [`advance_frame`].
    ///
    /// [`advance_frame`]: Self::advance_frame
    pub fn emit<E: Clone + Send + 'static>(&mut self, event: E) {
        self.entry_mut::<E>().channel.emit(event);
    }

    /// Return a shared reference to the channel for `E`, or `None` if no
    /// event of type `E` has ever been emitted or subscribed.
    #[must_use]
    pub fn channel<E: Clone + Send + 'static>(&self) -> Option<&EventChannel<E>> {
        self.channels
            .get(&TypeId::of::<E>())
            .and_then(|any| any.as_any().downcast_ref::<ChannelEntry<E>>())
            .map(|entry| &entry.channel)
    }

    /// Return a mutable reference to the channel for `E`, creating it if
    /// needed.
    pub fn channel_mut<E: Clone + Send + 'static>(&mut self) -> &mut EventChannel<E> {
        &mut self.entry_mut::<E>().channel
    }

    /// Register advisory interest in events of type `E`.
    ///
    /// Returns a unique [`SubscriptionId`]. Does **not** attach a callback;
    /// callers must poll [`channel`] themselves. Subscription tracking is used
    /// only for diagnostics and ordering hints.
    ///
    /// [`channel`]: Self::channel
    #[must_use]
    pub fn subscribe<E: Send + 'static>(&mut self) -> SubscriptionId {
        let id = SubscriptionId::from_raw(self.next_subscription);
        self.next_subscription += 1;
        self.subscriptions
            .entry(TypeId::of::<E>())
            .or_default()
            .push(id);
        id
    }

    /// Remove a subscription by ID.
    ///
    /// If `id` is not registered (already removed or never issued by this
    /// bus), the call is a no-op.
    pub fn unsubscribe(&mut self, id: SubscriptionId) {
        for subs in self.subscriptions.values_mut() {
            subs.retain(|&s| s != id);
        }
    }

    /// Advance every channel to the next frame and emit one [`Info`]
    /// diagnostic for each channel that had pending events.
    ///
    /// The diagnostic message includes the event type name (via
    /// [`std::any::type_name`]) and the pending count so operators can trace
    /// event flow without additional tooling.
    ///
    /// Pass `&mut ()` as the sink to silently discard diagnostics.
    ///
    /// [`Info`]: rge_kernel_diagnostics::Severity::Info
    pub fn advance_frame(&mut self, sink: &mut dyn DiagnosticSink) {
        for any in self.channels.values_mut() {
            any.advance_frame();
            let count = any.pending_len_before_advance();
            if count > 0 {
                let name = any.type_name();
                sink.emit(Diagnostic::info(format!(
                    "events: advanced channel `{name}` with {count} pending event(s)"
                )));
            }
        }
        self.frame += 1;
    }

    /// The current frame index. Starts at `0` and increments with each call
    /// to [`advance_frame`].
    ///
    /// [`advance_frame`]: Self::advance_frame
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Number of distinct event-type channels registered in this bus.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Return a mutable reference to the typed [`ChannelEntry<E>`] for `E`,
    /// inserting a fresh one if absent.
    fn entry_mut<E: Clone + Send + 'static>(&mut self) -> &mut ChannelEntry<E> {
        self.channels
            .entry(TypeId::of::<E>())
            .or_insert_with(|| Box::new(ChannelEntry::<E>::new()))
            .as_any_mut()
            .downcast_mut::<ChannelEntry<E>>()
            .expect("TypeId invariant: entry type matches key")
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Ping(u32);

    #[derive(Clone, Debug, PartialEq)]
    struct Pong(u32);

    #[test]
    fn new_bus_is_empty() {
        let bus = EventBus::new();
        assert_eq!(bus.frame(), 0);
        assert_eq!(bus.channel_count(), 0);
    }

    #[test]
    fn emit_creates_channel_lazily() {
        let mut bus = EventBus::new();
        bus.emit(Ping(1));
        assert_eq!(bus.channel_count(), 1);
    }

    #[test]
    fn channel_is_none_before_any_emit() {
        let bus = EventBus::new();
        assert!(bus.channel::<Ping>().is_none());
    }

    #[test]
    fn emit_and_advance_delivers_events() {
        let mut bus = EventBus::new();
        bus.emit(Ping(1));
        bus.emit(Ping(2));
        bus.advance_frame(&mut ());
        let events: Vec<&Ping> = bus.channel::<Ping>().unwrap().iter_current().collect();
        assert_eq!(events, [&Ping(1), &Ping(2)]);
    }

    #[test]
    fn frame_counter_increments_on_advance() {
        let mut bus = EventBus::new();
        assert_eq!(bus.frame(), 0);
        bus.advance_frame(&mut ());
        assert_eq!(bus.frame(), 1);
    }

    #[test]
    fn multiple_types_in_same_bus() {
        let mut bus = EventBus::new();
        bus.emit(Ping(10));
        bus.emit(Pong(20));
        bus.advance_frame(&mut ());
        assert_eq!(
            bus.channel::<Ping>().unwrap().iter_current().next(),
            Some(&Ping(10))
        );
        assert_eq!(
            bus.channel::<Pong>().unwrap().iter_current().next(),
            Some(&Pong(20))
        );
    }

    #[test]
    fn subscribe_returns_unique_ids() {
        let mut bus = EventBus::new();
        let a = bus.subscribe::<Ping>();
        let b = bus.subscribe::<Ping>();
        let c = bus.subscribe::<Pong>();
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn unsubscribe_removes_id() {
        let mut bus = EventBus::new();
        let id = bus.subscribe::<Ping>();
        bus.unsubscribe(id);
        let key = TypeId::of::<Ping>();
        assert!(bus
            .subscriptions
            .get(&key)
            .map_or(true, |v| !v.contains(&id)));
    }

    #[test]
    fn advance_frame_emits_diagnostic_for_nonempty_channel() {
        let mut bus = EventBus::new();
        bus.emit(Ping(1));
        let mut agg = DiagnosticAggregator::new();
        bus.advance_frame(&mut agg);
        assert_eq!(agg.len(), 1);
        let msg = &agg.iter().next().unwrap().message;
        assert!(msg.contains('1'), "message should contain count: {msg}");
    }

    #[test]
    fn advance_frame_no_diagnostic_for_empty_channel() {
        let mut bus = EventBus::new();
        bus.emit(Ping(1));
        bus.advance_frame(&mut ());
        let mut agg = DiagnosticAggregator::new();
        bus.advance_frame(&mut agg);
        assert_eq!(agg.len(), 0);
    }

    #[test]
    fn no_op_sink_unit() {
        let mut bus = EventBus::new();
        bus.emit(Ping(99));
        bus.advance_frame(&mut ());
    }

    #[test]
    fn default_impl_matches_new() {
        let bus = EventBus::default();
        assert_eq!(bus.frame(), 0);
        assert_eq!(bus.channel_count(), 0);
    }
}
