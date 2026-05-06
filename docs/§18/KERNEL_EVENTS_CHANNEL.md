# KERNEL_EVENTS_CHANNEL

| Companion to | PLAN.md §1.7 (diagnostics philosophy) + PLAN.md §6.14 (subsystem integration map — kernel/events as the cross-subsystem event substrate) |
|---|---|
| Status | Stable v1; 40 tests passing (23 unit + 16 unit in subscription/channel + 16 integration); double-buffered per-channel + TypeId-keyed bus + `SubscriptionId` substrate landed pre-2026-05-08 |
| Audience | Every Tier-1 + Tier-2 author needing to publish or consume cross-subsystem events — anim-graph emitting bone-events, physics emitting trigger-events, script-host receiving anim-events, hot-reload listeners, etc. |
| Sibling doc | `KERNEL_DIAGNOSTICS.md` — `EventBus::advance_frame` emits one `Diagnostic::info` per channel with pending events, routed through the standard `DiagnosticSink` |
| Reference impls | `kernel/events/src/{lib,bus,channel,subscription}.rs` (substrate) · `kernel/events/tests/event_bus_test.rs` (10-case integration covering Phase 1.3 requirements) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the kernel/events substrate; subsystem-specific event types (DamageEvent, BoneEvent, AssetReloadedEvent, …) are defined by their producing subsystems and consumed via the bus.

## 1. Why a substrate

Without one, N subsystems would each invent their own pubsub. Anim-graph would push bone events into a private `Vec`; physics would push trigger events into a different `Vec`; script-host would poll both via `unsafe { ... }` access patterns; hot-reload would need yet a third channel. PLAN §1.7 commits to one [`EventChannel<E>`] + one [`EventBus`] + one [`SubscriptionId`] shape so cross-subsystem events use a single uniform plumbing.

The bus is **frame-queued**: events emitted during frame N are visible during frame N+1. This decouples emission timing from consumption timing — a subsystem whose tick runs first can still emit events that subsystems later in the frame will see, and a subsystem whose tick runs last can still emit events that subsystems early in the next frame will see. No "events emitted during my tick are immediately visible" race; no "events emitted after my tick run are missed" race.

Per the lib-level module-doc, the design goals are:

- **No callbacks.** Subscribers iterate channels themselves; no closure storage; no dynamic dispatch explosion.
- **No async.** Frame-queued delivery is fully synchronous within a frame tick (no `tokio`, no `futures`).
- **Diagnostics-first.** [`EventBus::advance_frame`] emits one `Severity::Info` diagnostic per channel with pending events, using the standard [`DiagnosticSink`] interface.

## 2. `EventChannel<E>` double-buffer

Lives at `kernel/events/src/channel.rs`. The typed FIFO channel for one event type:

```rust
pub struct EventChannel<E> {
    pending: VecDeque<E>,
    delivered: VecDeque<E>,
    frame: u64,
}
```

> **Source-truth flag:** the dispatch spec described the buffers as `front: Vec<E>` + `back: Vec<E>`. Source-truth: the field names are `pending` (events emitted during the current frame; not yet visible) and `delivered` (events that were pending before the last `advance_frame`; visible to consumers during the current frame). The container type is `VecDeque<E>`, not `Vec<E>` — chosen because emission is FIFO and the swap-on-advance semantics (`std::mem::swap(&mut delivered, &mut pending)`) plus `pending.clear()` on the post-swap pending buffer reuses allocations across frames. Naming change reflects the producer/consumer perspective; the substrate behaviour matches the dispatch spec's intent. This doc reflects the source-truth.

### Lifecycle

```text
[systems emit events]  →  ch.emit(event)         // pushed to `pending`
[frame boundary]       →  ch.advance_frame()      // pending ↔ delivered (swap), pending cleared
[systems read events]  →  ch.iter_current()       // iterates delivered
```

`emit(event)` pushes to the back of `pending`. Never visible this frame.

`advance_frame()` swaps `pending ↔ delivered`, calls `pending.clear()` to drop the old delivered set, and increments `frame`. Allocation-free in steady state because `VecDeque` capacity is preserved across the swap; only the first few frames allocate as the worst-case-frame capacity is reached.

`iter_current() -> impl Iterator<Item = &E>` walks `delivered`. Empty iterator when no events were pending before the last advance.

### Auxiliary accessors

```rust
pub fn pending_len(&self) -> usize;   // events queued for next frame
pub fn current_len(&self) -> usize;   // events in this frame's delivered buffer
pub fn frame(&self) -> u64;           // frames advanced so far
pub fn clear(&mut self);              // drop both buffers; frame counter unchanged
```

`clear()` is rarely called in normal operation — it's used primarily by tests asserting clean-state invariants. The frame counter is intentionally NOT reset by `clear()`.

## 3. `EventBus` — heterogeneous typed registry

Lives at `kernel/events/src/bus.rs`. The bus owns one [`EventChannel<E>`] per event type, keyed by [`TypeId`]:

```rust
pub struct EventBus {
    channels: HashMap<TypeId, Box<dyn AnyChannel>>,
    next_subscription: u64,
    subscriptions: HashMap<TypeId, Vec<SubscriptionId>>,
    frame: u64,
}
```

> **Source-truth flag:** the dispatch spec described the channel storage as `BTreeMap<TypeId, Box<dyn Any + Send>>`. Source-truth: `HashMap<TypeId, Box<dyn AnyChannel>>` — `HashMap` (not `BTreeMap`) because per-channel iteration order during `advance_frame` is determined by HashMap iteration order, but the substrate's correctness does not depend on cross-channel deterministic order (each channel's internal FIFO is deterministic; cross-channel ordering between e.g. EventA and EventB delivered in the same frame is not a contract). The boxed type is a private `dyn AnyChannel` trait (not `dyn Any + Send`) — `AnyChannel` exposes only the operations that don't require the concrete `E` (`advance_frame`, `pending_len_before_advance`, `type_name`, `as_any` upcast for downcasting). Typed access goes through `as_any().downcast_ref::<ChannelEntry<E>>()`. This doc reflects the source-truth.

The `ChannelEntry<E>` struct wraps the raw `EventChannel<E>` + caches the pre-advance pending count (so the diagnostic emitted at `advance_frame` time has the count from before the swap) + caches the event type's `std::any::type_name::<E>()` string (so diagnostics can name the type without re-querying TypeId).

### Channel access

```rust
pub fn emit<E: Clone + Send + 'static>(&mut self, event: E);
pub fn channel<E: Clone + Send + 'static>(&self) -> Option<&EventChannel<E>>;
pub fn channel_mut<E: Clone + Send + 'static>(&mut self) -> &mut EventChannel<E>;
```

`emit::<E>(event)` creates the channel for `E` lazily on first emit (no upfront registration). `channel::<E>()` returns `None` if no event of type `E` has ever been emitted; `channel_mut::<E>()` always succeeds, creating the channel if absent.

The `Clone` bound is required because the future broadcast/multi-subscriber-per-channel design (deferred — see §7) will need to clone delivered events for multiple consumers. The `Send + 'static` bounds match the bus's `Box<dyn AnyChannel>` storage.

### Subscription tracking — advisory only

```rust
#[must_use]
pub fn subscribe<E: Send + 'static>(&mut self) -> SubscriptionId;
pub fn unsubscribe(&mut self, id: SubscriptionId);
```

> **Source-truth flag:** the dispatch spec described `subscribe<E>() -> SubscriptionId; iter<E>(SubscriptionId) -> impl Iterator<Item=&E>`. Source-truth: `subscribe<E>()` returns a `SubscriptionId` (an opaque `u64` newtype); `iter` does NOT exist on the bus. Subscriptions are **advisory only** — the bus does NOT invoke callbacks. Consumers call `bus.channel::<E>()?.iter_current()` directly. The `subscribe` / `unsubscribe` surface is preserved for diagnostics + ordering hints + future multi-subscriber broadcast, but today subscription tracking is bookkeeping-only and not required for event consumption. This doc reflects the source-truth.

`subscribe::<E>()` increments the bus's internal counter and pushes the new `SubscriptionId` into the per-type subscriber list. `unsubscribe(id)` retains all entries except `id` from every per-type list (linear scan; subscriptions are infrequent and the lists short).

The advisory model is intentional: the canonical event-consumption pattern is `bus.channel::<E>()?.iter_current()` per frame, not closure callbacks. This avoids the closure-storage / dynamic-dispatch explosion that callback-based event buses incur and keeps the substrate "no async / no callbacks" per the design goals.

## 4. `SubscriptionId`

Lives at `kernel/events/src/subscription.rs`. Opaque `u64` newtype:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    pub(crate) fn from_raw(n: u64) -> Self;
    #[must_use]
    pub fn raw(self) -> u64;
}
```

`from_raw` is `pub(crate)` so external callers cannot mint arbitrary IDs — only the bus's `next_subscription` counter does. `raw(self)` returns the underlying `u64` for logging / debugging / serialization. IDs are increment-only and globally unique within a bus instance; two `subscribe` calls on the same bus return distinct IDs even when unsubscribing in between. The `Serialize + Deserialize` derives let subscription IDs round-trip through replay logs.

## 5. Frame-queued delivery

The frame-queued contract is the load-bearing property of the substrate. From the bus's frame-lifecycle doc-comment:

```text
[systems emit events]  →  bus.emit::<E>(event)        // queued in pending
[frame boundary]       →  bus.advance_frame(&mut sink) // pending → delivered
[systems read events]  →  bus.channel::<E>()?.iter_current()
```

Concretely: emit during frame N writes to the back buffer of the channel; the orchestrator (kernel/app between ticks) calls `bus.advance_frame(sink)` to swap; subscribers reading during frame N+1 see events from frame N. This decouples emission timing from consumption timing.

Two corollaries:

1. **Tick order doesn't matter for event delivery.** A tick that runs early can emit events; a tick that runs late will still see them (next frame). A tick that runs late can also emit events; a tick that runs early in the *next* frame will see them. No "I have to be the last subsystem to tick to receive frame-N events" footgun.
2. **One frame of latency.** If subsystem A emits an event during frame N, subsystem B sees it during frame N+1 — so multi-step interactions take frames to propagate. For most editor-tier event flow this is fine; tight feedback loops (e.g. physics ↔ tessellation in a single frame) use direct API calls instead.

`advance_frame(sink)` walks `channels.values_mut()` in HashMap order (intentionally non-deterministic across runs), advancing each channel and emitting one diagnostic per non-empty channel. The `frame` counter on the bus increments by 1 per call.

## 6. Diagnostics integration

Per the `advance_frame` doc-comment + the `KERNEL_DIAGNOSTICS.md` sibling-doc convention:

```rust
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
```

> **Source-truth flag:** the dispatch spec described "channel-overflow diagnostics" with structured prefix `"event channel <type> overflow: dropped <N> events"`. Source-truth: there is **no overflow / drop semantics** in the current substrate. Channels are unbounded `VecDeque`s; emission never fails; events are never dropped silently. The diagnostic emitted by `advance_frame` is `Severity::Info` (NOT `Warning`) and the message is `"events: advanced channel \`{name}\` with {count} pending event(s)"`. The diagnostic exists for **observability** of frame-by-frame event flow, not for an error condition. The dispatch spec's overflow story is unimplemented; if a future bounded-channel mode is added (per §8 below), this doc and the Severity should be revisited at that time. This doc reflects the source-truth.

Pass `&mut ()` as the sink to silently discard diagnostics (the blanket `impl DiagnosticSink for ()` makes `()` a no-op sink — see `KERNEL_DIAGNOSTICS.md` §7). Tests routinely use `&mut ()` to suppress per-test diagnostic noise; production callers route through a real `DiagnosticAggregator` or streaming sink.

The diagnostic message contains the event type's `std::any::type_name::<E>()` (e.g. `event_bus_test::EventA`) and the pending count so an operator can trace event flow without additional tooling. One diagnostic per non-empty channel per advance — channels with zero pending events emit nothing (verified by the `empty_advance_emits_no_diagnostics` integration test).

Cross-ref `KERNEL_DIAGNOSTICS.md` §"Plugin-host auto-emit policy" for the canonical auto-emit shape; the events bus uses the same `DiagnosticSink::emit(Diagnostic)` discipline and the same `Severity` taxonomy.

## 7. Worked example — three subsystems exchanging events

A worked example mirroring the lib-level Quick start example but with three subsystems and two event types, illustrating the full lifecycle:

```rust
#[derive(Clone)] struct DamageEvent { entity: u64, amount: u32 }
#[derive(Clone)] struct HealEvent   { entity: u64, amount: u32 }

let mut bus = EventBus::new();

// Frame N — physics ticks, damage events emitted.
bus.emit(DamageEvent { entity: 7, amount: 10 });
bus.emit(DamageEvent { entity: 9, amount: 25 });

// Frame N — anim-graph ticks, heal event emitted.
bus.emit(HealEvent { entity: 7, amount: 5 });

// Orchestrator advances at frame boundary; diagnostics flow to aggregator.
let mut sink = DiagnosticAggregator::new();
bus.advance_frame(&mut sink);
// sink now contains 2 Info diagnostics (one per non-empty channel).

// Frame N+1 — script-host reads delivered events.
for d in bus.channel::<DamageEvent>().unwrap().iter_current() { /* ... */ }
for h in bus.channel::<HealEvent>().unwrap().iter_current()   { /* ... */ }
```

Note that the **emission order within a single channel is preserved** (DamageEvent for entity 7 arrives before entity 9 in the iter), but the **cross-channel order between DamageEvent and HealEvent is not specified** — both emitted in frame N are visible in frame N+1, but the iteration order between channels in `advance_frame` follows HashMap iteration order.

## 8. Channel capacity policy

> **Source-truth flag:** the dispatch spec asked whether channels are bounded or unbounded. Source-truth: **unbounded** — both `pending` and `delivered` are `VecDeque<E>` with no capacity cap. There is no `with_capacity` constructor on `EventChannel<E>` or `EventBus`; there is no drop / spillover policy; there is no "channel full" error path. A subsystem that emits 1M events in a frame allocates 1M slots' worth of `VecDeque` capacity. This is consistent with the "no async / no callbacks / lightweight" design goal but does mean a runaway producer can balloon allocator usage. If/when bounded-mode is added (anticipated for the future Tier-3 sandboxed plugin event-bus integration), the API will likely grow `with_capacity` + an overflow `DiagnosticSink::emit` on drop; that work is deferred until a use-case materialises.

For now, capacity governance is the producer's responsibility — a subsystem emitting events at 60 Hz should not accumulate without a consumer reading (and `advance_frame` clearing) at the same rate.

## 9. Consumers across the workspace

The substrate is consumed (per Cargo.toml deps) by the subsystems whose integration map appears in PLAN §6.14:

- **anim-graph** (future) — emits BoneEvent / AnimEventTag; consumed by script-host for game logic + physics for ragdoll triggers.
- **physics** (future) — emits TriggerEvent / CollisionEvent; consumed by script-host for game logic.
- **script-host** (future) — consumes anim + physics events; emits ScriptEvent (e.g. `EntitySpawned`, `WorldStateChanged`) for editor + telemetry.
- **kernel/app** (future) — drives `advance_frame` between ticks; consumes lifecycle events for orchestrator-level coordination.

Today the substrate is shipped + tested in isolation; producer + consumer wiring is added as the downstream subsystems land. The 40-test surface in `kernel/events/` covers every API path so consumers integrate against a known-stable contract.

## 10. Performance characteristics

Verified against source:

- **`emit`** — O(1) `VecDeque::push_back`. Allocation only on capacity growth.
- **`iter_current`** — O(channel-size). No copies; iterator yields `&E`.
- **`advance_frame`** (per channel) — O(1) `std::mem::swap` + O(prev-delivered-size) `clear()` (drops the dropped events; if `E: Drop` this is the cost of running their Drop impl).
- **`advance_frame`** (per bus) — O(channel-count) walk over `channels.values_mut()` + the per-channel cost above.
- **`subscribe`** — O(1) push into the per-TypeId `Vec<SubscriptionId>`.
- **`unsubscribe`** — O(total-subscriber-count) linear retain. Subscriptions are infrequent and lists short; this is intentional simplicity.

The bus has no allocator for steady-state frames once `pending` capacity is reached; all the per-frame work is queue swaps + iteration. This matches the "no async / no callbacks / lightweight" constraint at lib-level.

## 11. Test surface

Per the `kernel/events/tests/event_bus_test.rs` integration suite (10 numbered cases plus edge cases) + the in-module unit tests:

- **23 unit tests in `bus.rs`** — emit / advance / channel access / subscribe + unsubscribe / multiple-types / no-op sink / advance-emits-Info / counter-increment.
- **8 unit tests in `channel.rs`** — pending vs delivered separation / FIFO order / clear-drops-both / previous-delivered-dropped-on-advance / Default impl.
- **3 unit tests in `subscription.rs`** — `from_raw` round-trip / value-equality / Copy semantics.
- **16 integration tests in `event_bus_test.rs`** — covers all 10 Phase 1.3 required cases + edge cases (empty advance / channel count / monotonic IDs).

Total: **40 tests** matching the dispatch spec's "40+ tests" requirement.

## 12. Failure class — recoverable

Per the `//! Failure class: recoverable` declaration at `kernel/events/src/lib.rs` and PLAN §1.13. The substrate itself doesn't fail catastrophically:

- `emit` is infallible (returns `()`).
- `advance_frame` is infallible (returns `()`).
- `channel` is `Option`-typed (returns `None` for missing types — not an error path).
- `subscribe` / `unsubscribe` are infallible.
- The diagnostic sink emit is infallible (per `KERNEL_DIAGNOSTICS.md` §7's contract).

Channel-state diagnostics route through the standard `DiagnosticSink` per §6; consumers handling those diagnostics (e.g. an editor inspector showing per-frame event counts) decide what to do with the `Severity::Info` records. The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/events` does not appear in the failure-class exemptions table.

## 13. References

- **PLAN.md §1.7** — diagnostics philosophy (the substrate kernel/events emits diagnostics through).
- **PLAN.md §6.14** — subsystem integration map; kernel/events is the substrate connecting anim → script-host (anim-events) and physics → script-host (trigger-events).
- **PLAN.md §1.13** — failure-class taxonomy (recoverable definition).
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; the `Diagnostic` + `DiagnosticSink` types `EventBus::advance_frame` emits through; `Severity::Info` semantics.
- **`kernel/events/src/lib.rs`** — module roots + failure-class declaration + design goals + Quick start example.
- **`kernel/events/src/channel.rs`** — `EventChannel<E>` + double-buffer (pending/delivered) + frame counter + iter_current.
- **`kernel/events/src/bus.rs`** — `EventBus` + `AnyChannel` private trait + `ChannelEntry<E>` + emit / channel / channel_mut / subscribe / unsubscribe / advance_frame.
- **`kernel/events/src/subscription.rs`** — `SubscriptionId` opaque `u64` newtype + `from_raw` (`pub(crate)`) + `raw` accessor.
- **`kernel/events/tests/event_bus_test.rs`** — 16 integration tests covering all 10 IMPLEMENTATION.md Phase 1.3 cases + edge cases.
- **`kernel/diagnostics/src/sink.rs`** — `DiagnosticSink` trait the bus emits through.
