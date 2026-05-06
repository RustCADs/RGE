# KERNEL_AUDIT_LEDGER

| Companion to | PLAN.md §6.16.6 (Command Bus / audit-ledger projection) + PLAN.md §1.13 (failure-class taxonomy) + PLAN.md §1.6.8 (Replay-Stable v1.0 determinism mode) |
|---|---|
| Status | Stable v1; 26 tests passing (7 event + 12 ledger + 4 replay unit tests + 3 integration tests in `kernel/audit-ledger/tests/`); consumed by `editor-actions::CommandBus` for Action projection per PLAN §6.16 |
| Audience | Subsystem authors emitting projectable events (Action / CadCheckpoint / plugin-defined); replay-handler authors building golden-test or determinism-mode infrastructure; orchestrators that need cross-process content-addressed event identity |
| Sibling doc | `EDITOR_ACTIONS_COMMAND_BUS.md` — primary consumer; every `Action::apply` projects to the ledger via `EventKind::Action`; the bus's undo-stack cursor and the ledger cursor advance in lockstep |
| Reference impls | `kernel/audit-ledger/src/{lib,event,ledger,replay}.rs` (substrate) · `kernel/audit-ledger/tests/{cursor_undo_redo,replay_byte_identical,replay_handler_can_halt}.rs` (integration) · `crates/editor-actions/src/bus.rs` (consumer wiring) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the audit-ledger event-store substrate; the `EDITOR_ACTIONS_COMMAND_BUS.md` companion covers the consumer (Action submission / cursor-in-lockstep / undo-tail truncate).

## 1. Why a substrate

Every editor mutation must be (a) projectable for replay / golden tests, (b) auditable for telemetry, (c) deterministically identified for cross-process dedup. Without a substrate, every mutating subsystem invents its own event-id scheme: editor-actions would reach for one BLAKE3 recipe, future cad-checkpoints would reach for another, plugin-defined event streams would each fork. Three months later, the workspace would have N parallel projection paths and zero shared replay surface.

PLAN §6.16.6 commits to **one canonical AuditLedger** as the projection target for every projectable event — Action mutations today, CAD checkpoints next phase, plugin-defined kinds when plugins land. The substrate's three load-bearing properties:

- **Content-derived `EventId`.** A 32-byte BLAKE3 over `(kind_tag, payload)` — stable across machines for identical input. Two events with the same kind+payload always produce the same id, enabling content-addressed de-duplication by callers.
- **Append-only sequence.** Monotonic `seq: u64` per ledger; combined with a cursor, this gives the Command Bus its undo / redo projection (events `[0, cursor)` are applied; `[cursor, len)` available to redo).
- **Halt-on-false replay.** A handler closure receives each event in order; returning `false` consumes the current event then halts. This is the substrate that makes deterministic-replay-mode (PLAN §1.6.8) and golden-test diff (Phase 6 W11 1000-tick byte-identical gate) possible.

PLAN §1.13 line 573 promotes audit-ledger checksum failure to **kernel-fatal**: if the ledger itself can't be trusted, snapshot-restore-then-replay (the standard recovery path) cannot be trusted either.

> **Disambiguation note (2026-05-09)**: `physics::physics_input_ledger::PhysicsInputLedger` (renamed 2026-05-09 from the W11-era `stubs::audit_ledger::AuditLedger`) is a **separate per-tick physics-domain ledger**, NOT a renamed version of THIS kernel substrate. Physics records `PhysicsInput { Force / Impulse / JointMotor }` per-tick `TickRecord`s with a structurally different shape from the kernel's generic `Event { id, seq, timestamp_ms, kind, payload }`. The two intentionally co-exist: physics needs typed-payload per-tick ergonomics that don't fit the generic event-stream shape. See `crates/physics/src/physics_input_ledger.rs` module-doc + ADR-114 amendment-table footnote for the divergence rationale. Future work could (a) extend the kernel substrate with first-class per-tick / typed-payload abstractions and migrate physics, or (b) keep the two ledgers separate permanently (current decision per audit-debt registry).

## 2. `EventId`

Lives at `kernel/audit-ledger/src/event.rs`. The deterministic 32-byte identifier:

```rust
pub struct EventId(pub [u8; 32]);

impl EventId {
    pub fn compute(kind: &EventKind, payload: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(kind.kind_tag().as_bytes());
        hasher.update(b"\x00");          // separator
        hasher.update(payload);
        Self(*hasher.finalize().as_bytes())
    }
}
```

> **Source-truth flag:** the dispatch spec described `compute` as taking `(kind: &str, payload: &[u8])`. Source-truth: `compute(kind: &EventKind, payload: &[u8])` — the function takes the typed `EventKind` and pulls its stable string tag via `kind.kind_tag()`. The mechanism is the same; this doc reflects the actual signature.

The `b"\x00"` separator is load-bearing. Without it, `kind_tag = "Action"` + `payload = b"foo"` would hash identically to a hypothetical kind whose tag is `"Actio"` and payload is `b"nfoo"`. The separator forecloses that prefix-collision class. The `kind_tag_separator_prevents_prefix_collision` regression test pins the property.

Two events with the same `(kind, payload)` always produce the same `EventId`. That is intentional — it lets callers content-address by id and de-duplicate replayed actions. `EventId` derives `Hash + Eq + Ord` so it works as a map key and supports sorted iteration. `Display` renders as `blake3:<8-hex>…` for log lines; `to_hex()` returns the full 64-char hex string for debugging.

## 3. `EventKind`

```rust
pub enum EventKind {
    Action,                  // editor-actions::Action; payload owned by Command Bus
    CadCheckpoint,           // CAD checkpoint (Phase 4-Geometry); payload owned by cad-core
    Custom(String),          // plugin-defined; payload owned by registering plugin
}
```

Open enum so plugins register their own kinds without forking the ledger crate. `kind_tag()` returns the stable string used as the BLAKE3 prefix — `"Action"`, `"CadCheckpoint"`, or the inner `Custom` name. **Changing a tag is a breaking change** that invalidates all existing event ids; the event-id format is stable across runs precisely because the tags are stable.

The three regression tests `event_id_is_deterministic_for_identical_input`, `event_id_differs_for_different_kind_same_payload`, and `event_id_custom_kind_uses_name_as_tag` pin the determinism + tag-discrimination properties.

## 4. `Event`

```rust
pub struct Event {
    pub id: EventId,             // deterministic, recomputable
    pub seq: u64,                // monotonic per ledger; not deterministic across runs
    pub timestamp_ms: u64,       // wall-clock; for human inspection only
    pub kind: EventKind,
    pub payload: Vec<u8>,        // opaque; format owned by producer
}
```

Append-only — never mutated after `AuditLedger::record` returns. `id` and `payload` are deterministic; `seq` and `timestamp_ms` are runtime-derived and intentionally NOT part of the determinism story (`seq` orders within one run; `timestamp_ms` is for human inspection in logs). Replay reconstructs state from `id` + `payload` regardless of `seq` / `timestamp_ms`.

## 5. `AuditLedger`

```rust
pub struct AuditLedger {
    events: Vec<Event>,
    cursor: u64,                 // in [0, events.len()]
}
```

Append-only `Vec<Event>` plus an undo-projection cursor. Persistence (disk flush, journal rotation) is a Phase-3+ concern — for now, the ledger holds events for the duration of the run.

### Core methods

```rust
impl AuditLedger {
    pub fn new() -> Self;
    pub fn record(&mut self, kind: EventKind, payload: Vec<u8>) -> EventId;
    pub fn iter(&self) -> impl Iterator<Item = &Event>;
    pub fn iter_reverse(&self) -> impl DoubleEndedIterator<Item = &Event>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn cursor(&self) -> u64;
    pub fn set_cursor(&mut self, cursor: u64) -> Result<(), LedgerError>;
    pub fn redo_stream(&self) -> impl Iterator<Item = &Event>;
    pub fn undo_stream(&self) -> impl DoubleEndedIterator<Item = &Event>;
    pub fn truncate(&mut self, target: u64) -> Result<(), LedgerError>;
    pub fn clear(&mut self);
}
```

> **Source-truth flag:** the dispatch spec described methods `append / cursor_advance / cursor_position / iter_from_cursor`. Source-truth: `record / set_cursor / cursor / redo_stream + undo_stream`. The semantics are equivalent (record IS append; set_cursor replaces both advance/retreat in one method; cursor IS cursor_position; redo_stream IS iter_from_cursor). This doc reflects the actual API.

`record` computes the `EventId`, assigns the next monotonic `seq`, captures wall-clock millis (truncating u128→u64 — bounds on the order of 585 million years, acceptable), checks the BLAKE3 collision guard (see §7), and pushes the new `Event`. Returns the assigned id so the caller can log / index it.

## 6. Cursor pattern for undo / redo projection

The cursor partitions the event sequence into two streams:

```text
                    cursor
                       │
   ┌───────────────────┴──────────┐
   │                              │
   ▼                              ▼
   events [0, cursor)             events [cursor, len)
   ── undo stream                 ── redo stream
   (already applied)              (available to replay)
```

- `redo_stream()` returns events `[cursor, len)` — events the caller has NOT yet projected (or has un-projected via undo). Iteration is forward, oldest-first within the redo region.
- `undo_stream()` returns events `[0, cursor)` in **reverse** — newest-applied first. This is what the Command Bus iterates when the user hits "undo": the most recently applied event is reverted first, then `cursor -= 1`.
- `set_cursor(n)` validates `n <= len` (else `LedgerError::CursorOutOfRange`) and assigns. The Command Bus calls this after every `Action::apply` (cursor += 1), every undo (cursor -= 1), and every redo (cursor += 1).
- `truncate(target)` validates `target >= cursor` (else `LedgerError::TruncateBeforeCursor` — would orphan undo state) and discards the redo tail. The Command Bus calls this at the start of every fresh `submit` after a partial undo: the new submission invalidates the redo tail.

The pattern mirrors `EDITOR_ACTIONS_COMMAND_BUS.md` §5's `UndoStack { entries, cursor }` exactly. The bus advances both cursors in lockstep so a replay handler iterating the ledger sees the same `[0, cursor)` slice the bus's undo-stack sees.

## 7. Replay handler with halt-on-false

```rust
pub struct ReplayResult {
    pub events_applied: u64,
    pub events_skipped: u64,
    pub stopped_at_seq: Option<u64>,
}

impl AuditLedger {
    pub fn replay<F>(&self, mut handler: F) -> ReplayResult
    where F: FnMut(&Event) -> bool;
}
```

The handler runs once per event in append order. Returning `true` continues; returning `false` consumes the current event (it is counted in `events_applied`) and then halts — subsequent events count toward `events_skipped`. If all events are consumed, `stopped_at_seq` is `None`; otherwise `Some(seq_of_last_applied_event)`.

`replay` does NOT mutate the ledger (cursor / events / sequence are untouched). The integration test `replay_handler_halts_after_seq_4` pins the consume-then-halt boundary at `seq == 4`: 5 applied (0..=4), 5 skipped (5..=9), `stopped_at_seq = Some(4)`.

The `replay_produces_identical_event_log` integration test pins the byte-identical reconstruction property: replay every event into a fresh ledger via `record(kind.clone(), payload.clone())` and the resulting `(id, kind, payload)` triple matches the source for every entry. `seq` and `timestamp_ms` differ — by design (they are runtime-derived, not determinism-bearing).

This is the substrate that makes Phase 6 W11's 1000-tick byte-identical replay gate (per PLAN §1.6.8 Replay-Stable v1.0) possible: the replay handler reconstructs state from `(EventId, payload)` and the determinism-stable BLAKE3-id discriminates whether two reconstructions agree.

## 8. BLAKE3 collision guard

`record` does NOT detect intentional duplicates: the same `(kind, payload)` at different `seq` is valid and receives the same `EventId` (the `same_id_same_payload_duplicate_is_allowed` regression test pins this). The collision guard fires only when the **same id appears with a different payload** — which is a BLAKE3 preimage collision (astronomically unlikely with honest input) or a caller bug.

When that happens, the ledger logs `tracing::error!` with the seq + id and returns the computed id without appending the corrupted entry. This is conservative — a future Phase-3+ version may promote the return type to `Result<EventId, LedgerError>` so callers can branch — but at v1, the silent-discard-with-error-log keeps the surface uncluttered for the 99.9999%-of-the-time honest path.

## 9. Failure class — kernel-fatal

`kernel/audit-ledger/src/lib.rs` line 1 declares:

```rust
//! Failure class: kernel-fatal
```

Per PLAN §1.13 line 573 — "audit-ledger checksum failure = kernel-fatal". The lib.rs module-doc explains:

- **Why kernel-fatal**: an audit-ledger checksum failure (entry payload doesn't match its `EventId`; entry sequence has a hash break) means PIE state cannot be trusted. Replay can't reconstruct state. Undo / redo can't trust history. The kernel-fatal class signals: stop the engine, snapshot-restore is the only recourse.
- **Recoverable variants** of the substrate (cursor out of range; benign hash-collision logged) surface as `LedgerError` and are **caller-recoverable**: the kernel-fatal class applies specifically to integrity-violation paths.

The lib-level recovery-model paragraph captures the nuance: ledger corruption (hash collision, cursor out of range) is recoverable via snapshot restore — the ledger replays from the last good snapshot. Continuing without the ledger would silently lose the undo / redo audit trail, which is unsafe; a full snapshot restore restores it correctly.

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/audit-ledger` does not appear in `tools/architecture-lints/exemptions.toml`.

## 10. `LedgerError`

```rust
pub enum LedgerError {
    HashCollision(u64),                                    // collision at seq
    CursorOutOfRange { requested: u64, len: u64 },         // set_cursor too far
    TruncateBeforeCursor { target: u64, cursor: u64 },     // truncate would orphan undo
}
```

`HashCollision` is the BLAKE3-collision-detected variant returned (logged) when `record` sees an existing event with the same id but a different payload. `CursorOutOfRange` is the post-condition violation when a caller sets the cursor past `len`. `TruncateBeforeCursor` is the post-condition violation when a truncation target falls below the current cursor.

`set_cursor` and `truncate` propagate `Result<(), LedgerError>`; `record` returns the computed id even on collision (the caller can decide how to proceed). The `editor-actions::BusError::LedgerError(#[from] LedgerError)` variant forwards these for consumer-side handling.

## 11. Determinism gate

The replay handler's exit semantics are the determinism contract: same source `AuditLedger` + same handler MUST produce identical `ReplayResult` across processes. Specifically:

- `events_applied` is deterministic (handler is the only side-effect; same handler same call sequence).
- `stopped_at_seq` is deterministic (predicate on per-event content, not wall-clock or insertion order).
- `events_skipped = total - events_applied` is deterministic.

PLAN §1.6.8 Replay-Stable v1.0 promotes this property to a CI gate: Phase 6 W11's 1000-tick replay produces a byte-identical event log across machines. The regression-stable BLAKE3-id substrate is the keystone — without deterministic event identity, "byte-identical event log" would be undefined for a stream that includes wall-clock timestamps (which is why `id` derives only from `(kind, payload)` and `timestamp_ms` is excluded from the determinism story).

## 12. Consumer surface

The substrate's downstream users today + tomorrow:

- **`editor-actions::CommandBus`** — primary consumer per `EDITOR_ACTIONS_COMMAND_BUS.md` §9. Every `Action::apply` commits via `ledger.record(EventKind::Action, action.payload())`. The bus's `UndoStack` cursor and the ledger cursor advance in lockstep. Submit truncates the ledger redo-tail (`ledger.truncate(cursor)`) symmetrically with the stack's `entries.truncate(cursor)`. `Action::payload()` defaults to `name().as_bytes()`; richer impls override for replay diagnostics. The 5-test `audit_ledger_projection.rs` integration suite covers the lockstep advance + truncate semantics.
- **CAD checkpoints (Phase 4-Geometry)** — future consumer. `EventKind::CadCheckpoint` is reserved; a checkpoint commits to the ledger when `CadGraph::commit` advances HEAD. Payload format is owned by `cad-core` (likely a CheckpointId + RON-encoded GraphSnapshot diff per `CAD_CORE_MODEL.md` §5).
- **Plugin-defined events** — future consumer. `EventKind::Custom(String)` lets a plugin register its own event kind without forking the ledger. The plugin owns the payload format end-to-end; the ledger only stores the bytes.
- **Replay-mode tests** — future consumer per PLAN §1.6.8. A test fixture iterates `ledger.replay(|event| { ... })`, reconstructing state from `N=0` and asserting byte-identity against a captured baseline. Phase 6 W11's 1000-tick gate is the canonical example.
- **Telemetry / golden tests** — future consumer. Cross-run comparison of event streams (filtered by kind, sequenced by seq) for regression detection.

## 13. References

- **PLAN.md §6.16.6** — Command Bus / audit-ledger projection (full design).
- **PLAN.md §1.13 line 573** — failure-class taxonomy; "audit-ledger checksum fail = kernel-fatal".
- **PLAN.md §1.6.8** — Replay-Stable v1.0 determinism mode; the 1000-tick byte-identical replay gate.
- **`EDITOR_ACTIONS_COMMAND_BUS.md`** — sibling §18 doc; primary consumer; `ledger.record` invocation pattern + cursor-in-lockstep + `BusError::LedgerError` forwarding + `audit_ledger_projection.rs` integration test description.
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; future `EventKind::CadCheckpoint` consumer (`CheckpointHistory::commit` will project to the ledger when phase 4-Geometry checkpoints land).
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; `FailureClass::KernelFatal` enum variant the audit-ledger declaration maps to.
- **`kernel/audit-ledger/src/lib.rs`** — module roots + failure-class declaration + recovery-model paragraph.
- **`kernel/audit-ledger/src/event.rs`** — `EventId::compute` + `EventKind` enum + `Event` struct + 7 unit tests.
- **`kernel/audit-ledger/src/ledger.rs`** — `AuditLedger` + `LedgerError` + 12 unit tests pinning monotonic seq, cursor bounds, truncate-below-cursor rejection, redo/undo stream partition, hash-collision guard.
- **`kernel/audit-ledger/src/replay.rs`** — `replay` + `ReplayResult` + 4 unit tests pinning halt-on-false consume-then-halt + no-mutation guarantee + handler-sees-events-in-order.
- **`kernel/audit-ledger/tests/cursor_undo_redo.rs`** — integration test for the full cursor / undo / redo / truncate scenario.
- **`kernel/audit-ledger/tests/replay_byte_identical.rs`** — integration test pinning the byte-identical reconstruction (id + kind + payload match; seq + timestamp_ms differ by design).
- **`kernel/audit-ledger/tests/replay_handler_can_halt.rs`** — integration test pinning halt-after-seq-4 boundary semantics.
- **`crates/editor-actions/src/bus.rs`** — consumer wiring; the seven-step `submit` flow that drives ledger advance + truncate.
