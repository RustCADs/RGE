# EDITOR_ACTIONS_COMMAND_BUS

| Companion to | PLAN.md §6.16 (Command Bus + Undo/Redo) + PLAN.md §13.7 (Command Bus / undo gates) |
|---|---|
| Status | Stable v1; the `command-bus` architecture lint actively enforces direct-mutation imports outside `crates/editor-actions/` since Phase 2 (2026-05-05); 40 tests passing |
| Audience | editor-ui authors + tool authors emitting mutations; subsystem authors implementing new `Action` types; orchestrator authors threading `&mut World` through the bus |
| Sibling doc | `KERNEL_ECS_WORLD.md` — the `World` types every `Action::apply` mutates; mutation surface re-exported by `kernel/ecs::lib.rs` for the lint to grep |
| Reference impls | `crates/editor-actions/src/{lib,action,bus,coalesce,compound,undo_stack}.rs` (substrate) · `crates/editor-actions/tests/{smoke_undo_byte_identical,coalesce_window,compound_atomic,save_mark_dirty,audit_ledger_projection,test_actions}.rs` (canaries) · `tools/architecture-lints/src/command_bus.rs` (lint enforcement) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the Command Bus mutation substrate; subsystem-specific `Action` impls (e.g. cad-projection's parametric-edit Actions, gfx's render-side Actions when those land) belong in their sibling §18 docs.

## 1. Why a substrate

Without one, every editor mutation site re-invents the same four properties:

- **Reversibility.** Spawn an entity → undo must despawn. Insert a component → undo must remove. Without a substrate, every tool author writes their own undo bookkeeping; bugs creep in differently in every tool.
- **Replayability.** PIE / golden tests / regression captures replay a mutation stream. Without a substrate, the mutation log fragments across N tool-private formats.
- **Auditability.** Telemetry + diagnostic surfaces want every mutation projected into a single typed stream. Without a substrate, that projection has to be re-implemented per-tool.
- **Coalesce semantics.** Typing five characters in a name field is one undo, not five — but only if a substrate detects "same target, within 500 ms" and merges. Without a substrate, every text-editing widget reinvents debounce.

PLAN §6.16 commits to the **Command Bus** pattern as the single resolution: every mutation is an [`Action`]; the [`UndoStack`] manages history; the [`AuditLedger`] projects every Action for downstream consumers. PLAN §13.7 promotes the rule to a CI gate ("every editor mutation goes through Command Bus" + "no editor crate touches runtime mutation outside the bus"). The `command-bus` architecture lint enforces the gate on every PR — see §10.

## 2. `Action` trait

Lives at `crates/editor-actions/src/action.rs`. The trait every reversible mutation implements:

```rust
pub trait Action: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn id(&self) -> ActionId;
    fn apply(&self, world: &mut World) -> Result<(), ActionResult>;
    fn revert(&self, world: &mut World) -> Result<(), ActionResult>;
    fn merge(&mut self, _next: &dyn Action) -> MergeOutcome { MergeOutcome::Distinct }
    fn payload(&self) -> Vec<u8> { self.name().as_bytes().to_vec() }
}
```

> **Source-truth flag:** the dispatch spec listed `apply` returning `Result<(), ActionError>`. The actual error type is `ActionResult` — see §3 below. The dispatch's "method list (`apply / revert / merge / name`)" is correct as far as it goes, but the source surface also includes `id()` (mandatory, no default — used by [`CoalesceWindow`]) and `payload()` (default = `name().as_bytes()`, override for richer audit-ledger payloads). This doc reflects the source-truth.

### Bound rationale

`Send + Sync + 'static` is required because the [`UndoStack`] stores `Box<dyn Action>` — heterogeneous, owned, with no borrow lifetime tying the action back to its construction site. `Send` lets a future hot-reload / cross-thread orchestrator move actions between threads (matching the `Plugin` trait's analogous bound — see `PLUGIN_API.md` §1.1). `Sync` is required because `BusEntry::apply` and `revert` take `&self` so an action's read paths must be safe across thread boundaries.

### Minimal `InsertComponent` impl

```rust
struct InsertMarker { entity: EntityId, value: u32 }

impl Action for InsertMarker {
    fn name(&self) -> &str { "insert-marker" }
    fn id(&self) -> ActionId {
        ActionId::new(format!("insert-marker(entity={:?})", self.entity))
    }
    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        world.insert(self.entity, Marker(self.value));
        Ok(())
    }
    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        world.remove::<Marker>(self.entity);
        Ok(())
    }
}
```

Note that `apply` checks `world.entity(self.entity).is_none()` and returns [`ActionResult::MissingEntity`] before mutating — this is the canonical pattern. The world's mutation surface (`insert` / `remove` / `replace` / `despawn`) is itself recoverable on missing entities (cross-ref `KERNEL_ECS_WORLD.md` §12), but `Action` impls SHOULD pre-validate so the audit-ledger payload is meaningful (no "applied to entity that didn't exist" entries).

## 3. `ActionId` + `ActionResult` + `MergeOutcome`

```rust
pub struct ActionId(pub String);

pub enum ActionResult {
    ApplyFailed(String),
    RevertFailed(String),
    MissingEntity(EntityId),
}

pub enum MergeOutcome { Merged, Distinct }
```

[`ActionId`] is the **coalesce target identity**. Convention: `"<verb>(entity=<id>,<distinguishing-axes>)"`. Example from the test fixtures: `"insert-marker(entity=0x1234)"`. Two consecutive same-id actions within the 500 ms window are eligible for merging; different ids never merge regardless of timing. Both `ActionId` and `EntityId` derive `Serialize + Deserialize` so audit-ledger payloads carrying ids round-trip through replay.

[`ActionResult`] is the error type returned by `apply` / `revert`. Three variants:

- **`ApplyFailed(String)`** — generic apply failure with a human-readable reason. Used when the inner work errored for non-missing-entity reasons.
- **`RevertFailed(String)`** — symmetric variant for revert paths. Surfaces in the bus's diagnostic stream as `failure_class = SnapshotRecoverable`.
- **`MissingEntity(EntityId)`** — the canonical "target entity not found" path. Pre-checked at the top of canonical `apply` impls.

`thiserror` derives the `Display` and `std::error::Error` impls. The bus wraps these via `BusError::ActionFailed(#[from] ActionResult)` for outward propagation (see §6).

[`MergeOutcome`] is the return type of [`Action::merge`]. `Merged` means "I absorbed `next`'s state; drop it from the stack"; `Distinct` means "we cannot merge — keep both". Default impl returns `Distinct` so opting into coalesce is a deliberate per-Action override.

## 4. `CompoundAction` — atomic rollback

Lives at `crates/editor-actions/src/compound.rs`. Bundles a `Vec<Box<dyn Action>>` into a single reversible unit. `apply` runs each inner action in order; if the `k`-th `apply` fails, inner actions `0..k` are reverted in reverse order before the error propagates:

```rust
fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
    for (i, action) in self.inner.iter().enumerate() {
        if let Err(e) = action.apply(world) {
            for prev in self.inner[..i].iter().rev() {
                if let Err(rev_err) = prev.revert(world) {
                    tracing::error!(/* ... */);
                }
            }
            return Err(e);
        }
    }
    Ok(())
}
```

The rollback is best-effort on the revert path — if a revert during rollback also fails, the inner failure is logged via `tracing::error!` but the outer error is the one that propagates (the user sees the cause of the outer failure, not a double-fault during cleanup). `revert` itself walks `self.inner.iter().rev()`, collecting the first error and continuing through the remaining reverts so partial revert always completes.

> **Source-truth flag:** the dispatch spec described "atomic rollback on inner failure" using `action[0]` / `action[1]` indexing. Source uses `enumerate()` + `self.inner[..i].iter().rev()` — equivalent semantically; this doc uses the source's actual flow so the citation grep'ing finds the right code.

`CompoundAction` is used for multi-step ops that must succeed-or-roll-back as a unit: "delete entity + remove from selection", "swap two parametric values atomically", "apply preset" (set N components in one undo step). Default `merge` returns `Distinct` — compounds never coalesce with adjacent actions even if their ids match (typing five chars individually coalesces; typing then committing as a compound does not).

`payload()` is overridden: the compound's name + each inner action's payload, length-prefixed (`u32 LE`, saturating for >4 GiB — though real payloads are KB-scale). The audit-ledger downstream consumer can recurse into the compound's structure for replay.

## 5. `UndoStack` + cursor

Lives at `crates/editor-actions/src/undo_stack.rs`. The ordered history:

```rust
pub struct UndoStack {
    pub(crate) entries: Vec<BusEntry>,
    pub(crate) cursor: u64,
    save_mark: Option<SaveMark>,
}
```

> **Source-truth flag:** the dispatch spec described the storage as `Vec<Box<dyn Action>>`. Source-truth: `Vec<BusEntry>`, where `BusEntry` is a `#[non_exhaustive]` enum with one variant today (`Action(Box<dyn Action>)`) and a future variant for `CadCheckpoint` per PLAN §6.16.4. This shape lets future CAD-graph checkpoints (which don't fit the `Action` mould) co-exist on the same undo stream without breaking ABI. This doc reflects the source-truth.

The `cursor: u64` tracks how many entries are in the "applied" portion. Entries at `[cursor, len)` are available for redo. `cursor == 0` means "nothing applied yet"; `cursor == entries.len()` means "all applied; nothing to redo". Re-submitting after an undo truncates the redo tail before pushing the new entry.

Public accessors are read-only (`len`, `is_empty`, `cursor`, `save_mark`); the `entries` and `cursor` fields are `pub(crate)` so only [`CommandBus`] can mutate them (matches PLAN §6.16's "single mediation layer" property — direct stack edits would bypass the apply/revert/audit-ledger discipline).

## 6. `CommandBus` — the bus

Lives at `crates/editor-actions/src/bus.rs`. Wraps the stack, the coalesce window, the audit ledger, and a diagnostic aggregator:

```rust
pub struct CommandBus {
    stack: UndoStack,
    coalesce: CoalesceWindow,
    ledger: AuditLedger,
    diagnostics: DiagnosticAggregator,
}
```

### `submit(action, world)`

Seven-step flow per the doc-comment on `submit`:

1. Capture wall-clock milliseconds via `wall_clock_ms()` (saturating to `u64::MAX` on absurd-future clocks).
2. If the action's id matches the most recent entry's id within the 500 ms coalesce window AND the stack is non-empty AND `cursor > 0`, attempt [`Action::merge`]:
   - **`Merged`** — the existing entry absorbs `action`; the world advances via a direct `action.apply` (no new stack entry, no new ledger event). Coalesce window is updated.
   - **`Distinct`** — fall through to normal submission.
3. **Apply** the action via `action.apply(world)`. On `Err(e)`, emit a `Diagnostic::error` carrying `failure_class = SnapshotRecoverable` to the bus's aggregator and return `BusError::ActionFailed(e)` — the world is left in whatever state the failed apply left it (Action authors are responsible for partial-failure cleanup inside their own `apply`).
4. **Truncate the redo tail** — `stack.entries.truncate(cursor)`; symmetric `ledger.truncate(cursor)` if the ledger is longer.
5. **Project to the audit ledger** — `ledger.record(EventKind::Action, action.payload())`.
6. **Push + advance cursor** — `stack.entries.push(BusEntry::Action(action))`; `stack.cursor += 1`.
7. **Update the coalesce window** — `coalesce.note_recorded(action_id, now_ms)`.

### `undo(world)` / `redo(world)`

Symmetric. `undo` errors with `BusError::NothingToUndo` when `cursor == 0`; otherwise calls `entries[cursor-1].revert(world)`, decrements cursor, syncs the ledger cursor, and resets the coalesce window (so undo across a coalesce boundary doesn't accidentally re-merge subsequent submissions). `redo` is the mirror (errors with `BusError::NothingToRedo` when `cursor >= len`).

The coalesce-reset on undo/redo is load-bearing: without it, undoing a typed-name coalesce group then re-typing would attempt to merge with the now-reverted-but-still-recent entry.

### `BusError`

```rust
pub enum BusError {
    NothingToUndo,
    NothingToRedo,
    ActionFailed(#[from] ActionResult),
    LedgerError(#[from] LedgerError),
}
```

`LedgerError` is forwarded from `kernel/audit-ledger`; in normal operation the bus advances ledger and stack cursors in lockstep so this variant is unreachable, but the surface preserves the error path for ledger-integrity violations.

## 7. `SaveMark`

Lives at `crates/editor-actions/src/undo_stack.rs`. A bookmark in the [`UndoStack`] identifying "the state at last explicit save":

```rust
pub struct SaveMark(pub u64);
```

The `u64` matches the cursor at the moment `mark_saved()` was called. `is_dirty()` returns `cursor != save_mark.0` (or `cursor > 0` when no save mark exists yet — a fresh stack with no edits is clean).

Used by editor-ui for two surfaces:

- **Save button enable/disable.** When `is_dirty() == false` the Save action is a no-op; the button should disable.
- **"Discard unsaved changes?" prompts.** On window-close / new-document / open-document, editor-ui consults `is_dirty()` and prompts if true.

`mark_saved()` on the [`CommandBus`] also resets the coalesce window so the next post-save submission starts a fresh history entry (a save boundary is a natural undo boundary). Cross-ref `EDITOR_STATE_MODEL.md` for how editor-ui surfaces consume `is_dirty()`.

## 8. `CoalesceWindow`

Lives at `crates/editor-actions/src/coalesce.rs`. Time-window helper:

```rust
pub struct CoalesceWindow {
    window_ms: u64,
    last_recorded_at: Option<u64>,
    last_id: Option<ActionId>,
}
```

`should_coalesce(next, now_ms)` returns `true` iff `Some(last) == Some(next)` AND `now_ms - last_recorded_at <= window_ms`. Default window per PLAN §6.16.7 is 500 ms (`CoalesceWindow::default_500ms()`), but `with_coalesce_window(ms)` lets the bus take a custom value (used by tests that need deterministic windowing).

Boundary semantics tested explicitly: exactly 500 ms apart **does** coalesce (`<=`); 501 ms does not. After 500 ms idle, the next same-id action starts a new history entry, so undo doesn't blow away 30 seconds of typing as one giant step.

`reset()` clears both `last_id` and `last_recorded_at`. The bus calls `reset()` on undo, redo, and `mark_saved()` so coalesce never crosses an undo/save boundary.

> **Source-truth flag:** the dispatch spec described coalesce as "the next same-ActionId action starts a new history entry". Source-truth confirms this with the additional detail that `note_recorded` records BOTH the id and the timestamp regardless of whether merging occurred (so a coalesce-merged action still extends the window — typing 30 chars over 8 seconds with each char within 500 ms of the previous all coalesces into one entry as long as no idle-gap exceeds the window).

## 9. `AuditLedger` projection

Every [`Action::apply`] commits to the [`AuditLedger`] via `ledger.record(EventKind::Action, action.payload())`. The default `payload()` returns `name().as_bytes()`; richer impls override to include parameters for replay diagnostics (cross-ref `CompoundAction::payload` for the canonical length-prefixed nested form).

The ledger's `record` returns an `EventId` deterministically computed via BLAKE3 over `(kind, payload)` (see `kernel/audit-ledger`). The ledger cursor advances in lockstep with the stack cursor; `set_cursor` is called after every push/undo/redo so the ledger replay handler can iterate `[0, cursor)` for "what's currently applied" or `[cursor, len)` for "what's available to redo".

The ledger is the substrate that makes **halt-on-false replay** possible: a replay handler iterates ledger events, calls `apply` on the corresponding Action, and on first error stops. The ledger is also the substrate that future telemetry / golden-test recording will project for cross-run comparison. Cross-ref `kernel/audit-ledger` for the lower-level event-store + truncate semantics.

## 10. The `command-bus` architecture lint

Lives at `tools/architecture-lints/src/command_bus.rs`. The lint that makes the "every editor mutation goes through the bus" rule a hard CI gate.

### Forbidden symbol list

Verified against `kernel/ecs/src/lib.rs` re-exports — every symbol below is `pub` from `kernel_ecs::` today. Importing any of these in any `crates/**` crate other than `crates/editor-actions/` fails CI immediately:

| Symbol | Kind |
|---|---|
| `kernel_ecs::Commands` | type / deferred-mutation API |
| `kernel_ecs::EntityMut` | type / mutable entity handle |
| `kernel_ecs::Mut` | type / component mutation guard |
| `kernel_ecs::insert` | World method / re-exported free fn |
| `kernel_ecs::remove` | World method / re-exported free fn |
| `kernel_ecs::replace` | World method / re-exported free fn |
| `kernel_ecs::insert_component` | free function |
| `kernel_ecs::remove_component` | free function |
| `kernel_ecs::despawn` | free function |
| `kernel_ecs::spawn_with` | free function |

> **Source-truth flag:** the dispatch spec listed the forbidden symbols speculatively. Source-truth: the list above is **exactly** the `FORBIDDEN_SYMBOLS` constant at `tools/architecture-lints/src/command_bus.rs` lines 81–98. The lint flipped from placeholder-active → active enforcement at Phase 2 verification on 2026-05-05.

### Scope

The lint applies **only to `crates/**`**. `kernel/**`, `runtime/**`, `editor/**`, and `tools/**` are intentionally skipped:

- `kernel/ecs` itself defines the surface (allow-list by construction).
- `runtime/**` system-scheduling code has legitimate `Commands` use; that pattern is governed by a separate runtime-system lint when the runtime layer matures.
- `editor/**` and `tools/**` are not user-facing plugin crates and are governed by different rules.

Read-only access (`kernel_ecs::Query`, `kernel_ecs::Res`, `kernel_ecs::EntityRef`, etc.) is **not** restricted — the lint targets only the mutation surface. The path filter at `is_target_crate_file` walks the components of the source path; `crates/editor-actions/` is the sole exemption inside `crates/**`.

### Implementation note

The lint walks `syn::ItemUse` AST nodes, flattens grouped imports (`use kernel_ecs::{Commands, EntityMut};` → two separate flat paths), and checks whether each flat path starts with `kernel_ecs` AND ends with a forbidden symbol. Renames (`use kernel_ecs::Commands as Cmd;`) are detected by the **original** name, not the alias — closing the obvious bypass.

Globs (`use kernel_ecs::*;`) are conservatively skipped by the lint because static analysis can't tell which symbols are imported. The gate catches concrete symbol imports the moment they land; glob bypasses would be caught by code review (and are themselves discouraged by workspace style).

## 11. Smoke test pattern

Lives at `crates/editor-actions/tests/smoke_undo_byte_identical.rs`. The integration test proving the round-trip property: spawn → insert → modify → undo → undo recovers byte-identical pre-insert state.

```rust
#[test]
fn spawn_insert_modify_undo_undo_byte_identical() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();
    bus.submit(Box::new(InsertAction { entity, value: 42 }), &mut world).unwrap();
    bus.submit(Box::new(ModifyAction { entity, new_value: 99, old_value: 42 }), &mut world).unwrap();

    bus.undo(&mut world).unwrap();   // back to value 42
    assert_eq!(world.entity(entity).unwrap().get::<TestVal>(), Some(&TestVal(42)));

    bus.undo(&mut world).unwrap();   // component absent
    assert_eq!(world.entity(entity).unwrap().get::<TestVal>(), None);
}
```

The companion `redo_after_undo_restores_state` proves the symmetric property — undo then redo returns to the same byte-identical state. Together these cover PLAN §13.7's "undo/redo round-trip on each Action type" gate. The test fixtures (`SpawnAction`, `InsertAction`, `ModifyAction`, `TestVal`) live at `crates/editor-actions/tests/test_actions.rs` and are deliberately minimal — exhaustive Action-type coverage is the responsibility of each downstream consumer's own tests (cad-projection's parametric-edit canaries, gfx's render-side Action canaries when those land).

The other four integration test files cover the orthogonal axes:

- `coalesce_window.rs` (4 tests) — boundary timing + reset semantics
- `compound_atomic.rs` (3 tests) — rollback on inner failure
- `save_mark_dirty.rs` (3 tests) — `is_dirty()` semantics across save mark moves
- `audit_ledger_projection.rs` (5 tests) — ledger advance + truncate on undo/redo

Total: 23 unit tests across the modules + 17 integration tests = **40 tests**, matching PLAN §13.7's coverage requirement.

## 12. Failure class — snapshot-recoverable

Per the `//! Failure class: snapshot-recoverable` declaration at `crates/editor-actions/src/lib.rs` and PLAN §1.13. The choice of class is deliberate per the lib.rs doc-comment:

> `snapshot-recoverable`: bus-state corruption is recoverable via snapshot restore + audit-ledger replay. Plain `recoverable` would be too lax — losing the undo stack mid-session matters.

When an `Action::apply` fails, the bus emits a `Diagnostic::error` carrying `with_failure_class(FailureClass::SnapshotRecoverable)` (cross-ref `KERNEL_DIAGNOSTICS.md` §4 for the failure-class taxonomy). Callers handle the diagnostic per the `SnapshotRecoverable` policy: surface to user, offer rollback to last checkpoint, mark the stack as needing reconciliation if continued use is unsafe.

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `crates/editor-actions` does not appear in the failure-class exemptions table at `tools/architecture-lints/exemptions.toml`.

## 13. References

- **PLAN.md §6.16** — Command Bus + Undo/Redo (full design).
- **PLAN.md §6.16.7** — Coalescing (500 ms window, the rule).
- **PLAN.md §13.7** — Command Bus / undo gates (CI requirements).
- **PLAN.md §1.13** — failure-class taxonomy (snapshot-recoverable definition).
- **ADR-091** + **ADR-100** — Command Bus design rationale (PLAN §6.16 cites both).
- **`KERNEL_ECS_WORLD.md`** — sibling §18 doc; the `World` types every `Action::apply` mutates; mutation surface `kernel_ecs::lib.rs` re-exports the symbols the lint enforces.
- **`KERNEL_DIAGNOSTICS.md`** — sibling; the `Diagnostic` shape carrying `FailureClass::SnapshotRecoverable` for action-failure surfaces.
- **`PIE_SNAPSHOT.md`** — sibling; future participant for the bus-state recovery path (action stream + snapshot working together).
- **`crates/editor-actions/src/lib.rs`** — module roots + failure-class declaration.
- **`crates/editor-actions/src/action.rs`** — `Action` trait + `ActionId` + `ActionResult` + `MergeOutcome`.
- **`crates/editor-actions/src/bus.rs`** — `CommandBus` + `BusEntry` + `BusError` + the seven-step `submit` flow.
- **`crates/editor-actions/src/coalesce.rs`** — `CoalesceWindow` + boundary semantics.
- **`crates/editor-actions/src/compound.rs`** — `CompoundAction` + atomic rollback.
- **`crates/editor-actions/src/undo_stack.rs`** — `UndoStack` + `SaveMark` + `is_dirty()`.
- **`crates/editor-actions/tests/`** — six integration test files; 17 tests covering smoke / coalesce / compound / save-mark / audit-ledger paths.
- **`tools/architecture-lints/src/command_bus.rs`** — the `command-bus` lint; `FORBIDDEN_SYMBOLS` constant; `is_target_crate_file` path filter; `flatten_use_tree` rename detection.
- **`kernel/audit-ledger`** — the substrate the bus projects every Action onto for replay / telemetry.
