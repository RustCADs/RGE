# EDITOR_STATE_MODEL

| Companion to | PLAN.md §1.15 (editor coordination state — coordination-not-authority) |
|---|---|
| Status | Stable v1; Selection / Hover / ActiveTool implemented; ModalState + DragDrop are intentional stubs deferred per IMPLEMENTATION.md Phase 5.2 + §0.6 freeze policy |
| Audience | editor-ui authors + tool-mode authors who need to coordinate selection / hover / tool state across panels; subsystem authors integrating with the editor's interaction model |
| Sibling doc | `PLUGIN_API.md` — canaries that read editor coordination state route through this substrate; `KERNEL_ECS_WORLD.md` — `Selection` / `Hover` reference `kernel/ecs::EntityId` (no upward import) |
| Reference impls | `crates/editor-state/src/lib.rs` (module roots) · `crates/editor-state/src/selection.rs` · `crates/editor-state/src/hover.rs` · `crates/editor-state/src/active_tool.rs` · `crates/editor-state/src/modal_state.rs` (stub) · `crates/editor-state/src/drag_drop.rs` (stub) · `tools/architecture-lints/src/editor_state_ownership.rs` (the lint enforcing both ownership and coordination-not-authority) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the `editor-state` coordination substrate; subsystem-specific consumer conventions belong in editor-ui / cad-projection / etc.'s sibling §18 docs.

## 1. The coordination-not-authority pattern

`editor-state` COORDINATES selection / hover / tool state across panels but does NOT own authoritative content. Authoritative content lives elsewhere:

- **Component bodies** — `kernel/ecs::Archetype` columns (cad-projection, gfx, physics ECS components).
- **CAD operator graphs** — `cad-core::CadGraph`.
- **Asset payloads** — `asset-store`.
- **Audit + transactions** — `editor-actions::Action` + the Command Bus (PLAN §6.16).

Per PLAN §1.15: editor-state may NOT import authoritative content types. The `editor-state-ownership` architecture lint enforces both halves of the rule (see §9). Coordination state is the cross-panel UI bookkeeping that makes the editor cohere — what's selected, what's hovered, which tool is active, modal-dialog state, drag-and-drop state. Each consumer panel reads from `editor-state` to render correctly; mutations flow through the Command Bus, not direct edits to coordination state from random call sites.

The five categories are **fixed** at v0.8 per architecture freeze §0.6. Adding a 6th requires an ADR + freeze-policy gate.

## 2. `Selection`

Lives at `crates/editor-state/src/selection.rs`. The set of entities the user has selected:

```rust
pub struct Selection {
    entities: BTreeSet<EntityId>,
}
```

Backed by `BTreeSet<EntityId>` for **deterministic iteration order** (required for inspector display + audit-log recording — same iteration order across runs makes recordings comparable byte-for-byte). `EntityId: Ord` reduces to `Ulid: Ord` reduces to `u128: Ord` — stable cross-process.

### Methods

```rust
impl Selection {
    pub fn new() -> Self;
    pub fn add(&mut self, entity: EntityId) -> bool;        // true if newly added
    pub fn remove(&mut self, entity: EntityId) -> bool;     // true if was present
    pub fn clear(&mut self);
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn contains(&self, entity: EntityId) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_;
    pub fn replace_with<I: IntoIterator<Item = EntityId>>(&mut self, entities: I);
    pub fn toggle(&mut self, entity: EntityId) -> bool;     // returns new membership
}
```

Notable behaviours:

- **Add returns presence delta** — `true` for "newly added", `false` for "already present". Useful for action-emitting callers that want to know whether the selection actually changed.
- **Iter yields ascending ULID order** — deterministic because `BTreeSet`. Inspector callers don't have to sort.
- **Toggle returns the new membership** — `true` after a no-op-if-absent `add`, `false` after a no-op-if-present `remove`. Single-call shape for shift-click style interactions.
- **`replace_with` clears + bulk-inserts in one call** — used by box-select drag operations that want to commit a whole new set at gesture-end.

### Serde round-trip

`EntityId` doesn't impl `Serialize`/`Deserialize` directly (the kernel doesn't enable `ulid/serde`). `editor-state` enables `ulid/serde` in its own `Cargo.toml` and bridges via a private `EntityIdSerde(ulid::Ulid)` newtype that does. The `Selection` `Serialize / Deserialize` impls round-trip through `BTreeSet<EntityIdSerde>`, preserving ULID order.

## 3. `Hover`

Lives at `crates/editor-state/src/hover.rs`. Per-panel hover state — each panel can track its own hovered entity independently:

```rust
pub struct Hover {
    panels: BTreeMap<PanelId, EntityId>,
}
```

`BTreeMap<PanelId, EntityId>` for the same deterministic-iteration discipline. Each panel stores at most one hovered entity (or `None`). The viewport may hover entity A while the scene-tree hovers entity B; the inspector renders highlights for both.

### Methods

```rust
impl Hover {
    pub fn new() -> Self;
    pub fn set(&mut self, panel: PanelId, entity: EntityId);
    pub fn clear(&mut self, panel: &PanelId);
    pub fn clear_all(&mut self);
    pub fn get(&self, panel: &PanelId) -> Option<EntityId>;
    pub fn iter(&self) -> impl Iterator<Item = (&PanelId, EntityId)>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`set` is replace-semantics (a panel has at most one hover; setting overwrites). `clear` is per-panel; `clear_all` is the bulk reset (e.g. on selection-tool exit). `iter` yields `(&PanelId, EntityId)` pairs in `PanelId` ascending order.

Used by editor-ui's hover-highlight render passes + tooltip surfacing. Cross-panel highlighting (e.g. hovering an entity in the scene-tree highlights it in the viewport) is achieved by a single render pass that reads from all panels' hover entries.

### Serde round-trip

Same pattern as `Selection` — manual `Serialize / Deserialize` via `BTreeMap<PanelId, EntityIdSerde>`.

## 4. `PanelId`

Stable identifier for an editor panel:

```rust
pub struct PanelId(pub String);
```

String slug (`"scene-tree"`, `"inspector"`, `"viewport"`) for now. Future migration to a numeric handle is a Phase 6 concern (when panel registration becomes dynamic enough to justify the indirection). `Hash + Ord + Display` so it works as a `BTreeMap` key and renders in debug logs / audit output.

```rust
impl PanelId {
    pub fn new(s: impl Into<String>) -> Self;
    pub fn as_str(&self) -> &str;
}
```

The wrapped `pub String` is intentional — the surface is a thin newtype, not an opaque indirection. Callers that already have a `&str` literal use `PanelId::new("viewport")`.

## 5. `ActiveTool` enum

Lives at `crates/editor-state/src/active_tool.rs`. The "modal" tool selection that determines what mouse interactions mean:

```rust
pub enum ActiveTool {
    Select,    // default; cursor / select-mode (no gizmo)
    Translate, // translation gizmo (W key in Maya/Blender muscle memory)
    Rotate,    // rotation gizmo
    Scale,     // scale gizmo
    Brush,     // brush / sculpt tool
}
```

`Default = Select`. `Copy + PartialEq + Eq + Hash + Serialize + Deserialize` — cheap to thread through APIs and audit-log records. The enum just discriminates the tool *category*; tools may be parameterised via separate types (e.g. a future `BrushSettings` carrying brush radius / strength / falloff) — those parameters do NOT live on `ActiveTool` because adding them would force inspector-dropdown enumeration to grow with the parameter space.

### Methods

```rust
impl ActiveTool {
    pub const fn label(self) -> &'static str;       // "Select" / "Translate" / etc.
    pub fn all() -> &'static [ActiveTool];           // declaration order; for inspector dropdowns
}
```

`label()` returns a stable display string for the placeholder viewport overlay + audit-log records. `all()` returns a `&'static [ActiveTool]` in declaration order — single global state for now; per-viewport tool stacks are a Phase 6 concern (when multi-viewport layouts become a real workflow).

`Display` writes the label via `f.write_str(self.label())`.

## 6. `ModalState` (stub per IMPLEMENTATION.md)

Lives at `crates/editor-state/src/modal_state.rs`. **Intentionally empty stub** per the file's module-doc:

```text
//! `editor_state::modal_state` — deferred per IMPLEMENTATION.md Phase 5.2.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15). Implemented
//! when an actual feature demonstrates demand (modal dialogs / blocking flows).
//! Promote only on demonstrated 2-subsystem pressure (§0.6 freeze policy).
```

The category is reserved — modal popups, dialogs, wizard flows. The substrate is stubbed until the first concrete consumer demands it; at that point the substrate gets a real `ModalState` type, the consumer migrates to use it, and the stub becomes a load-bearing module. Per PLAN §0.6 freeze policy, "promote on demonstrated 2-subsystem pressure" — meaning two independent consumer use cases need to coalesce on one shape before the substrate ships.

## 7. `DragDrop` (stub per IMPLEMENTATION.md)

Lives at `crates/editor-state/src/drag_drop.rs`. **Intentionally empty stub** per the same policy as ModalState:

```text
//! `editor_state::drag_drop` — deferred per IMPLEMENTATION.md Phase 5.2.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15). Implemented
//! when an actual feature demonstrates demand (drag-and-drop interactions).
//! Promote only on demonstrated 2-subsystem pressure (§0.6 freeze policy).
```

Drag-and-drop state machine across panels. Same deferred-stub policy: implement when the first 2 subsystems converge on a use shape.

## 8. The `editor-state-ownership` architecture lint

Lives at `tools/architecture-lints/src/editor_state_ownership.rs`. Two enforcement halves:

### Part A — ownership

The five forbidden type names (`FORBIDDEN_TYPE_NAMES` constant in the lint source):

```rust
const FORBIDDEN_TYPE_NAMES: &[&str] =
    &["Selection", "Hover", "ActiveTool", "ModalState", "DragDrop"];
```

Any `struct`, `enum`, or `type` alias with one of those names found in another crate is a violation. `use … ::Selection` (re-import) is explicitly NOT flagged — that's the correct usage pattern.

The lint walks the syntax tree via `syn::visit::Visit` looking for `ItemStruct` / `ItemEnum` / `ItemType` whose `ident` matches a forbidden name and the file is outside `crates/editor-state/`. Files inside `tools/architecture-lints/` are skipped entirely so test fixtures can use the names freely.

### Part B — coordination-not-authority

`crates/editor-state/` may NOT import authoritative content from a Tier-2 crate family. The lint's `FORBIDDEN_IMPORT_PREFIXES` constant enumerates ~30 forbidden crate names (with `_` for `-`, matching Rust path syntax):

```text
cad_core, cad_native, cad_occt,
components_animation, components_audio, components_editor, components_identity,
components_interaction, components_lifecycle, components_networking,
components_physics, components_render, components_spatial, components_visibility,
material_graph, material_runtime,
anim_clip, anim_graph, anim_ik,
asset_store, pak_format,
io_gltf, io_image, io_step, io_stl, io_obj, io_audio,
physics, audio, input
```

Any `use` whose leading path segment matches one of these is a violation. **Exception:** `kernel/*` crates (paths starting with `kernel_`) are freely importable — they only expose IDs and primitive handles. Concretely, `use rge_kernel_ecs::EntityId` is allowed; `use rge_cad_core::CadGraph` is not.

The lint walks `ItemUse` nodes via `syn::visit::Visit` and flags violations with line numbers recovered by source-text scanning (the `proc_macro2::Span::start()` route requires the `span-locations` feature which is not activated workspace-wide).

### Current status

Per the dispatch reading, the lint reports **0 violations / 0 exemptions** as of 2026-05-06 (the W03 editor-shell exemption was cleared with the editor-shell migration to `editor-state`). The lint exits 0; the architecture gate is clean.

## 9. The W03 → editor-state migration history

Early Wave-03 lived in `editor-shell`'s `coord.rs` — proto-`Selection` and proto-`ActiveTool` types lived alongside editor-shell's own UI plumbing. The migration to canonical `editor-state::Selection / ActiveTool` re-export pattern landed during Phase 5.2 alongside the kernel/ecs `EntityId` migration (per Status.md "editor-shell migration to editor-state + kernel/ecs::EntityId" entry).

The migration steps:

1. **Mint canonical types.** `Selection` / `Hover` / `ActiveTool` shipped in `crates/editor-state/`.
2. **Convert editor-shell to re-export.** editor-shell's old proto types were replaced by `pub use rge_editor_state::{Selection, ActiveTool, Hover};` re-exports, allowing pre-existing call sites to continue compiling unchanged.
3. **Migrate fields.** Each consumer's `Selection` field-type reference was retargeted at the canonical location; the editor-shell re-export remained the call-site shim.
4. **Update the lint exemption.** The W03 editor-shell exemption was cleared from `tools/architecture-lints/exemptions.toml` once all `Selection` / `Hover` / `ActiveTool` definitions outside `crates/editor-state/` had been removed.

Readers extending coordination state going forward shouldn't need to dig through Wave docs; the substrate shape is settled at v1.

## 10. Failure class

`crates/editor-state/src/lib.rs` declares `//! Failure class: recoverable` per PLAN §1.13. State changes are diff-coordinated (no PIE-state at this level); failures (e.g. invalid `Selection` contents from corrupted load) are handled in-place by the consumer.

The failure modes:

- **Corrupted serde input.** `Selection::deserialize` returns the underlying serde error; the caller decides whether to surface a diagnostic, fall back to empty selection, or abort.
- **`PanelId` collisions.** `BTreeMap` semantics — `Hover::set` on an existing panel replaces the prior value; `Selection::add` on an existing entity is a no-op return-`false`. Neither collision is a failure.
- **`EntityId` referring to a despawned entity.** Coordination state holds IDs; if the entity is despawned without clearing the selection, the next render-pass query against the `World` returns `None` and the consumer renders nothing for that ID. The substrate doesn't auto-prune — the consumer (or the action that despawned the entity) is responsible for clearing the selection / hover.

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `crates/editor-state` does not appear in the failure-class exemptions table.

## 11. References

- **PLAN.md §1.15** — editor coordination state; the five-category fix; coordination-not-authority rule.
- **PLAN.md §0.6** — architecture freeze policy; "promote on demonstrated 2-subsystem pressure" governs ModalState / DragDrop substrate landing.
- **PLAN.md §1.13** — failure-class taxonomy.
- **`PLUGIN_API.md`** — sibling §18 doc; canary plugins that read editor coordination state thread it through `PluginContext` as a staged resource (`Selection` / `Hover` are stagable types).
- **`KERNEL_ECS_WORLD.md`** — sibling §18 doc; `Selection` / `Hover` reference `kernel/ecs::EntityId`; no upward import per the lint's Part B.
- **`crates/editor-state/src/lib.rs`** — module roots + failure-class declaration + the five-category architectural commitment.
- **`crates/editor-state/src/selection.rs`** — `Selection` + `EntityIdSerde` private serde bridge.
- **`crates/editor-state/src/hover.rs`** — `Hover` + `PanelId` + per-panel hover bookkeeping.
- **`crates/editor-state/src/active_tool.rs`** — `ActiveTool` enum + `label()` + `all()` enumeration.
- **`crates/editor-state/src/modal_state.rs`** — intentional stub.
- **`crates/editor-state/src/drag_drop.rs`** — intentional stub.
- **`tools/architecture-lints/src/editor_state_ownership.rs`** — Part A (ownership) + Part B (coordination-not-authority) lint impl.
- **`tools/architecture-lints/exemptions.toml`** — exemption registry; editor-state-ownership lint shows 0 exemptions as of 2026-05-06.
