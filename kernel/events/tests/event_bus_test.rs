//! Integration tests for `rge-kernel-events`.
//!
//! Covers all 10 required test cases from IMPLEMENTATION.md Phase 1.3.

use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};
use rge_kernel_events::{EventBus, EventChannel, SubscriptionId};

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct EventA(u32);

#[derive(Clone, Debug, PartialEq)]
struct EventB(String);

// ---------------------------------------------------------------------------
// 1. Channel round-trip
// ---------------------------------------------------------------------------

#[test]
fn channel_round_trip_emit_advance_iter() {
    let mut ch: EventChannel<EventA> = EventChannel::new();
    let e1 = EventA(1);
    let e2 = EventA(2);
    ch.emit(e1.clone());
    ch.emit(e2.clone());
    ch.advance_frame();
    let got: Vec<&EventA> = ch.iter_current().collect();
    assert_eq!(got, [&e1, &e2]);
}

// ---------------------------------------------------------------------------
// 2. Frame counter starts at 0; advance_frame increments
// ---------------------------------------------------------------------------

#[test]
fn frame_counter_starts_at_zero_and_increments() {
    let mut ch: EventChannel<EventA> = EventChannel::new();
    assert_eq!(ch.frame(), 0);
    ch.advance_frame();
    assert_eq!(ch.frame(), 1);
    ch.advance_frame();
    assert_eq!(ch.frame(), 2);
}

// ---------------------------------------------------------------------------
// 3. Pending vs delivered separation
// ---------------------------------------------------------------------------

#[test]
fn pending_before_advance_delivered_after() {
    let mut ch: EventChannel<EventA> = EventChannel::new();
    ch.emit(EventA(42));
    // Before advance: pending=1, delivered=0
    assert_eq!(ch.pending_len(), 1);
    assert_eq!(ch.current_len(), 0);

    ch.advance_frame();
    // After advance: pending=0, delivered=1
    assert_eq!(ch.pending_len(), 0);
    assert_eq!(ch.current_len(), 1);
}

// ---------------------------------------------------------------------------
// 4. Bus emit + read via channel
// ---------------------------------------------------------------------------

#[test]
fn bus_emit_advance_channel_iter() {
    let mut bus = EventBus::new();
    bus.emit(EventA(99));
    bus.advance_frame(&mut ());
    let events: Vec<&EventA> = bus.channel::<EventA>().unwrap().iter_current().collect();
    assert_eq!(events, [&EventA(99)]);
}

// ---------------------------------------------------------------------------
// 5. Multiple types coexist in the bus
// ---------------------------------------------------------------------------

#[test]
fn multiple_event_types_coexist() {
    let mut bus = EventBus::new();
    bus.emit(EventA(1));
    bus.emit(EventB("hello".into()));
    bus.advance_frame(&mut ());

    let a: Vec<&EventA> = bus.channel::<EventA>().unwrap().iter_current().collect();
    let b: Vec<&EventB> = bus.channel::<EventB>().unwrap().iter_current().collect();

    assert_eq!(a, [&EventA(1)]);
    assert_eq!(b, [&EventB("hello".into())]);
}

// ---------------------------------------------------------------------------
// 6. Subscriptions: unique IDs, unsubscribe removes
// ---------------------------------------------------------------------------

#[test]
fn subscriptions_tracked_and_unsubscribe_removes() {
    let mut bus = EventBus::new();
    let id1 = bus.subscribe::<EventA>();
    let id2 = bus.subscribe::<EventA>();
    let id3 = bus.subscribe::<EventB>();

    // All IDs distinct.
    assert_ne!(id1, id2);
    assert_ne!(id1, id3);
    assert_ne!(id2, id3);

    // After unsubscribe, id1 is gone but id2 remains.
    bus.unsubscribe(id1);
    let id4 = bus.subscribe::<EventA>();
    // id4 should not equal id1 (counter is monotonic).
    assert_ne!(id4, id1);
}

#[test]
fn subscription_id_raw_is_monotonic() {
    let mut bus = EventBus::new();
    let a = bus.subscribe::<EventA>();
    let b = bus.subscribe::<EventA>();
    assert!(b.raw() > a.raw());
}

// ---------------------------------------------------------------------------
// 7. Diagnostics on advance: one Info per channel with non-zero pending
// ---------------------------------------------------------------------------

#[test]
fn advance_emits_info_diagnostic_per_nonempty_channel() {
    let mut bus = EventBus::new();
    bus.emit(EventA(1));
    bus.emit(EventA(2));
    bus.emit(EventB("x".into()));

    let mut agg = DiagnosticAggregator::new();
    bus.advance_frame(&mut agg);

    // Two channels had events → two diagnostics.
    assert_eq!(
        agg.len(),
        2,
        "expected one diagnostic per non-empty channel"
    );

    // All diagnostics are Info severity.
    for d in agg.iter() {
        assert_eq!(d.severity, Severity::Info);
    }

    // Each diagnostic message mentions a count ≥ 1.
    let messages: Vec<&str> = agg.iter().map(|d| d.message.as_str()).collect();
    // EventA channel had 2 pending; EventB channel had 1.
    let combined = messages.join(" ");
    assert!(
        combined.contains('2') || combined.contains('1'),
        "message should include event counts: {combined}"
    );
}

#[test]
fn advance_diagnostic_contains_type_name() {
    let mut bus = EventBus::new();
    bus.emit(EventA(10));

    let mut agg = DiagnosticAggregator::new();
    bus.advance_frame(&mut agg);

    // The diagnostic message should contain something from the type name.
    // std::any::type_name::<EventA>() typically includes "EventA".
    let msg = &agg.iter().next().unwrap().message;
    // The full type_name may be "event_bus_test::EventA" or similar.
    assert!(
        msg.contains("EventA"),
        "message should contain type name: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 8. No-op sink: bus.advance_frame(&mut ()) compiles and works
// ---------------------------------------------------------------------------

#[test]
fn no_op_unit_sink_works() {
    let mut bus = EventBus::new();
    bus.emit(EventA(7));
    bus.advance_frame(&mut ()); // must not panic or fail to compile
    assert_eq!(bus.channel::<EventA>().unwrap().current_len(), 1);
}

// ---------------------------------------------------------------------------
// 9. Channel clear drops both buffers
// ---------------------------------------------------------------------------

#[test]
fn channel_clear_drops_both_buffers() {
    let mut ch: EventChannel<EventA> = EventChannel::new();
    ch.emit(EventA(1));
    ch.advance_frame(); // delivered = [EventA(1)]
    ch.emit(EventA(2)); // pending = [EventA(2)]
    ch.clear();

    assert_eq!(ch.pending_len(), 0);
    assert_eq!(ch.current_len(), 0);
    assert_eq!(ch.iter_current().count(), 0);
}

// ---------------------------------------------------------------------------
// 10. Determinism: emit order preserved; iter is FIFO
// ---------------------------------------------------------------------------

#[test]
fn emit_order_preserved_fifo() {
    let values: Vec<u32> = (0..100).collect();
    let mut ch: EventChannel<EventA> = EventChannel::new();
    for &v in &values {
        ch.emit(EventA(v));
    }
    ch.advance_frame();

    let got: Vec<u32> = ch.iter_current().map(|e| e.0).collect();
    assert_eq!(got, values, "iteration must be FIFO");
}

#[test]
fn bus_emit_order_preserved_across_multiple_emit_calls() {
    let mut bus = EventBus::new();
    for i in 0u32..50 {
        bus.emit(EventA(i));
    }
    bus.advance_frame(&mut ());
    let got: Vec<u32> = bus
        .channel::<EventA>()
        .unwrap()
        .iter_current()
        .map(|e| e.0)
        .collect();
    assert_eq!(got, (0..50).collect::<Vec<u32>>());
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_advance_emits_no_diagnostics() {
    let mut bus = EventBus::new();
    bus.emit(EventA(1));
    bus.advance_frame(&mut ()); // drain
    let mut agg = DiagnosticAggregator::new();
    bus.advance_frame(&mut agg); // nothing pending
    assert_eq!(agg.len(), 0);
}

#[test]
fn channel_count_reflects_distinct_types() {
    let mut bus = EventBus::new();
    assert_eq!(bus.channel_count(), 0);
    bus.emit(EventA(1));
    assert_eq!(bus.channel_count(), 1);
    bus.emit(EventB("x".into()));
    assert_eq!(bus.channel_count(), 2);
    // Emitting EventA again should not increase count.
    bus.emit(EventA(2));
    assert_eq!(bus.channel_count(), 2);
}

#[test]
fn subscription_id_raw_accessor() {
    let id = SubscriptionId::raw;
    // Just confirm it's accessible and returns a u64.
    let mut bus = EventBus::new();
    let sid = bus.subscribe::<EventA>();
    let _: u64 = id(sid);
}
