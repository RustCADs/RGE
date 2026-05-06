# KERNEL_ECS_WORLD

| Companion to | PLAN.md §6.13 (`SnapshotComponent` registry layer) + PLAN.md §1.5.2 (kernel ECS substrate) + PLAN.md §6.16 (Command Bus mutation surface) |
|---|---|
| Status | Stable; the `World` substrate backs the `world_bytes` layer of every PIE snapshot since 2026-05-08 |
| Audience | Subsystem authors building atop the kernel ECS — anyone calling `World::insert / remove / query / register_snapshot_component` from a Tier-2 crate or `editor-actions::Action::apply` |
| Sibling doc | `PIE_SNAPSHOT.md` — uses [`World::serialize_snapshot`] / [`restore_from_snapshot`] for the `world_bytes` layer of `PieSnapshot` |
| Reference impls | `kernel/ecs/src/world.rs` (root container) · `kernel/ecs/src/snapshot.rs` (registry + RGES envelope) · `kernel/ecs/src/storage/mod.rs` (relation storages) · `kernel/ecs/src/change_detection/mod.rs` (`Mut<T>` / `Changed<T>`) · `kernel/ecs/src/entity.rs` (`EntityId` / `EntityRef` / `EntityMut`) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the `kernel/ecs::World` substrate; subsystem-specific component conventions belong in their sibling §18 docs (e.g. `CAD_PROJECTION.md` for the `EntityCadMap` shape).

## 1. Why a substrate

Without a single ECS, every Tier-2 subsystem would invent its own entity-component-system / archetype-store / relation primitives. cad-projection, gfx, physics, audio, editor-ui would each ship their own incompatible `EntityId` and their own ad-hoc relation tables; the editor would have to thread a different mutation surface through every subsystem's `apply()` path. PLAN §1.5.2 commits to one canonical [`World`](https://docs.rs/...) shape; downstream `editor-actions::Action::apply` mutations + the plugin tick + the cad-projection tick all run against the same `World` instance.

The substrate's design goals (per the lib-level module-doc at `kernel/ecs/src/lib.rs`):

- **Safe Rust everywhere.** `#![forbid(unsafe_code)]` workspace-wide. The column-store implementation uses safe `Box<dyn Any + Send + Sync>` rather than the typed-pointer dance traditional ECS implementations rely on. Trade-off: cache linearity. Mitigation: future optimisation can swap to `unsafe` typed slabs behind a dedicated safety proof.
- **Deterministic iteration.** Snapshot order, query order, relation iteration — all stable across runs.
- **Plug into other substrates.** Snapshot via [`SnapshotComponent`] + per-component registry. Diagnostics via plugin-host. Mutation discipline enforced by the `command-bus` architecture lint.

## 2. `EntityId` — ULID-based, 128-bit

Defined at `kernel/ecs/src/entity.rs`. Opaque, monotonically-increasing handle:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(Ulid);
```

Backed by [ULID](https://github.com/ulid/spec) — 48-bit millisecond timestamp + 80-bit random tail. Properties:

- **Unique across processes.** Two engines spawning entities at the same instant produce different IDs because the random-tail entropy makes collision astronomically unlikely.
- **Time-ordered.** Lexicographic `Ord` matches `u128::Ord` matches creation time (within timestamp resolution). Snapshot iteration in EntityId order is therefore stable + meaningful.
- **`Send + Sync + Copy`.** Cheap to thread through APIs; no clone bookkeeping.
- **Self-describing.** ULIDs have a Crockford-base32 `Display` impl, so debug logs are human-readable.

### Constructors

```rust
impl EntityId {
    pub fn new() -> Self;            // generates fresh ULID
    pub fn from_ulid(ulid: Ulid) -> Self;  // for snapshot-restore round-trip
    pub fn ulid(self) -> Ulid;
}
```

`new()` is the canonical entry point; `from_ulid` is reserved for [`World::restore_from_snapshot`] and serde-deserialization paths that round-trip the raw `u128`. The [`PieSnapshot::restore`] path (sibling: `PIE_SNAPSHOT.md` §7) uses this to preserve original IDs across a serialize → restore cycle.

## 3. Single-archetype dense column-store

The implementation choice landed differently from a textbook archetype-per-component-set ECS. The kernel uses **a single catch-all archetype** (`World::archetypes: Vec<Archetype>` with `len() == 1` in practice). Trade-offs documented in `World`'s module-doc:

- **Queries iterate the full entity list** even when most rows don't carry the target component (they return `None` from `get` and are skipped).
- **No archetype migration cost** when inserting / removing components — the row stays in place; only the column entry for that type is added or removed.
- **Future optimisation** can introduce real per-component-set buckets with migration; the `World` API stays identical.

Internal structure (visible to module-internals only):

```rust
pub struct World {
    archetypes: Vec<Archetype>,
    entity_map: HashMap<EntityId, ArchetypeLocation>,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    commands_buffer: Commands,
    change_tick: u64,
    last_tick: u64,
    relations: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    snapshot_fns: BTreeMap<TypeId, SnapshotFns>,
}
```

The `snapshot_fns` field is `BTreeMap` (not `HashMap`) precisely so iteration during [`serialize_snapshot`] is deterministic — see §8 below.

> **Source-truth flag:** the dispatch spec described entity-keyed `BTreeMap<EntityId, ArchetypeId>` + `BTreeMap<ArchetypeId, Archetype>`. The actual implementation uses `HashMap<EntityId, ArchetypeLocation>` + `Vec<Archetype>`. Hash-keyed lookup is fine for entity-by-id resolution; deterministic iteration over entities is achieved at snapshot time by sorting the keys. The behavioural surface is unchanged; this doc reflects the source-truth.

### `ArchetypeLocation`

A `pub(crate)` record:

```rust
pub(crate) struct ArchetypeLocation {
    pub(crate) archetype_index: usize,
    pub(crate) row: usize,
}
```

`despawn` uses `Archetype::swap_remove_entity` and updates the moved entity's `row` if a different entity was swapped into the freed slot — this is the only place row indices shift, and the bookkeeping is contained in `World::despawn`.

## 4. Relation storages

Three flavours, each tuned for a different topology. Lives at `kernel/ecs/src/storage/mod.rs`:

| Storage | Used by | Topology |
|---|---|---|
| [`TreeRelationStorage`] | `parent_of` (`ParentOf` tag) | Sparse tree; parent has few children |
| [`DenseLinearRelationStorage`] | `bone_of` (`BoneOf` tag) | Dense ordered list; insertion order matters (skeletons) |
| [`SparseRelationStorage`] | `lod_of` (`LodOf`), `template_of` (`TemplateOf`) | Sparse map; arbitrary density |

All three are `HashMap`-backed internally (NOT `BTreeMap` — confirmed in `storage/mod.rs`). Iteration order is therefore non-deterministic for these maps; subsystems that need deterministic ordering (e.g. inspector display) sort post-collection. The relation tags carry a `RelationTag::Storage` associated type:

```rust
pub trait RelationTag: 'static {
    type Storage: Default + Send + Sync + 'static;
}
```

[`World::relations_mut::<R>`] / [`World::relations::<R>`] is the lookup:

```rust
pub fn relations_mut<R: RelationTag>(&mut self) -> &mut R::Storage;
pub fn relations<R: RelationTag>(&self) -> Option<&R::Storage>;
```

Free-function helpers `parent_of(world, parent, child)` + `bone_of(world, source, target)` route to `world.relations_mut::<ParentOf>().link(...)` / `bone_of>().link(...)`. Each storage type's `link` method handles the "already linked elsewhere" case by removing the old link first — this is the **reparent** semantics, not cycle prevention.

> **Source-truth flag:** the dispatch spec listed "cycle detection on insert" for `TreeRelationStorage`. The actual implementation has no cycle detection — `TreeRelationStorage::link` only enforces single-parent-per-child by removing the prior link. Subsystems that care about acyclicity (cad-core operator graphs, scene-tree validators) check at their own level rather than relying on the substrate. This doc reflects the source-truth.

## 5. `Changed<T>` per-archetype tick

Change-detection without per-component versioning. The world holds a `change_tick: u64`; each component slot carries its own `change_tick` recorded when [`Mut<T>`] was last dropped. Lives at `kernel/ecs/src/change_detection/mod.rs`.

The `Mut<T>` guard:

```rust
pub struct Mut<'a, T: Component> {
    value: &'a mut T,
    tick: &'a mut u64,
    world_tick: u64,
}
```

`Drop` writes `*self.tick = self.world_tick;` — even a read-only use of the guard bumps the slot tick (conservative, matches Bevy semantics). The `Deref / DerefMut` impls hide the bookkeeping from callers.

The `Changed<T>` query filter is a zero-sized marker; the actual filtering happens inside [`World::query`]:

```rust
pub fn query<F: QueryFilter>(&self) -> Query<'_, F::Component>;
```

`F::filter_type_id()` returns `Some(TypeId::of::<T>())` for `Changed<T>` and `None` for raw component queries. The query iterator walks all archetypes, picks slots whose `change_tick > last_tick`, and yields `(EntityId, &T)` pairs. [`World::advance_tick`] bumps `change_tick` and snapshots the prior value as `last_tick` — subsequent `Changed<T>` queries observe only mutations after the advance.

## 6. Mutation surface

The list `editor-actions::Action::apply` + plugin ticks call against. The `command-bus` architecture lint blocks any other call site from importing these symbols (per PLAN §6.16). Direct mutation methods on `World`:

```rust
pub fn spawn(&mut self) -> EntityId;
pub fn spawn_with<C: Component>(&mut self, component: C) -> EntityId;
pub fn despawn(&mut self, entity: EntityId) -> bool;
pub fn insert<C: Component>(&mut self, entity: EntityId, component: C);
pub fn remove<C: Component>(&mut self, entity: EntityId) -> Option<C>;
pub fn replace<C: Component>(&mut self, entity: EntityId, component: C) -> Option<C>;
pub fn entity(&self, entity: EntityId) -> Option<EntityRef<'_>>;
pub fn entity_mut(&mut self, entity: EntityId) -> Option<EntityMut<'_>>;
pub fn commands(&mut self) -> &mut Commands;
pub fn flush_commands(&mut self);
```

Free-function aliases (re-exports from `kernel/ecs/src/lib.rs`) for the command-bus lint's grepability:

```rust
pub fn insert<C: Component>(world: &mut World, entity: EntityId, component: C);
pub fn remove<C: Component>(world: &mut World, entity: EntityId) -> Option<C>;
pub fn replace<C: Component>(world: &mut World, entity: EntityId, component: C) -> Option<C>;
pub fn insert_component<C: Component>(world: &mut World, entity: EntityId, component: C);
pub fn remove_component<C: Component>(world: &mut World, entity: EntityId) -> Option<C>;
pub fn despawn(world: &mut World, entity: EntityId) -> bool;
pub fn spawn_with<C: Component>(world: &mut World, component: C) -> EntityId;
```

Both surfaces are flagged by the `command-bus` lint when imported outside `crates/editor-actions/`. Plugins coordinate through the `PluginContext` resource registry (see `PLUGIN_API.md` §2) rather than calling these directly.

### Deferred mutation: `Commands`

`World::commands()` returns a `&mut Commands` buffer. Mutations are not visible until [`flush_commands`] is called:

```rust
pub fn flush_commands(&mut self) {
    let cmds = std::mem::take(&mut self.commands_buffer);
    for cmd in cmds.into_ops() {
        cmd.apply(self);
    }
}
```

`Commands::into_ops` drains the queued operations; each `apply(&mut World)` runs immediately. Callers that need transactional semantics use `Commands` + a manual `flush_commands` at frame boundaries. Plugins that mutate the world inside their own tick body use immediate mutation through the staged `&mut World` resource.

### `EntityMut`

Returned by `World::entity_mut(id)`. Carries `(id, &mut Archetype, row, world_tick)`. Methods:

```rust
pub fn id(&self) -> EntityId;
pub fn get<C: Component>(&self) -> Option<&C>;
pub fn get_mut<C: Component>(&mut self) -> Option<Mut<'_, C>>;
pub fn insert<C: Component>(&mut self, component: C);
pub fn remove<C: Component>(&mut self) -> Option<C>;
```

`get_mut` returns a `Mut<C>` guard with the `world_tick` stamped at `entity_mut` time. Drop-bumps the slot's `change_tick` to `world_tick` — see §5.

## 7. `SnapshotComponent` trait

Opt-in per-component capture/restore via byte payload. Lives at `kernel/ecs/src/snapshot.rs`. Required so [`PieSnapshot`] can serialize the World deterministically. Surface:

```rust
pub trait SnapshotComponent: Component + Serialize + DeserializeOwned {
    fn snapshot_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}
```

The trait is a marker — `Component + Serialize + DeserializeOwned` does the work. `snapshot_name()` defaults to `std::any::type_name::<Self>()`; override for migration / cross-version compatibility. Components that don't impl this trait are **silently skipped** during snapshot — this is intentional per PLAN §6.13's "selective serialization" model.

> **Source-truth flag:** the dispatch spec described `SnapshotComponent` as having three methods (`snapshot_name` / `serialize` / `deserialize`). The actual surface is one method (`snapshot_name`); serialization is delegated to the inherited `Serialize + DeserializeOwned` bounds. The substrate carries postcard-encode/decode via type-erased `fn` pointers in `SnapshotFns`. This doc reflects the source-truth.

## 8. `World::register_snapshot_component` + the registry pattern

Per-frame the World holds a `BTreeMap<TypeId, SnapshotFns>` of registered components. `BTreeMap` is essential — iteration order during serialization must be deterministic, and `TypeId`'s `Ord` happens to satisfy us as a lookup key.

```rust
pub fn register_snapshot_component<C: SnapshotComponent>(&mut self) {
    self.snapshot_fns
        .entry(TypeId::of::<C>())
        .or_insert_with(|| SnapshotFns {
            serialize: make_serialize::<C>(),
            deserialize: make_deserialize::<C>(),
            name: C::snapshot_name(),
        });
}
```

The `SnapshotFns` bundle holds two `fn`-pointer monomorphisations and the component's stable name:

```rust
pub(crate) struct SnapshotFns {
    pub(crate) serialize: SnapshotSerializeFn,    // fn(&dyn Any) -> Result<Vec<u8>, _>
    pub(crate) deserialize: SnapshotDeserializeFn, // fn(&[u8]) -> Result<Box<dyn Any>, _>
    pub(crate) name: &'static str,
}
```

Calling `register_snapshot_component::<C>()` twice for the same `C` is idempotent (BTreeMap entry-or-insert).

### `serialize_snapshot` / `restore_from_snapshot`

```rust
pub fn serialize_snapshot(&self) -> Result<Vec<u8>, SnapshotError>;
pub fn restore_from_snapshot(&mut self, bytes: &[u8]) -> Result<(), SnapshotError>;
```

Per-frame iteration discipline (see `snapshot.rs` for the canonical impl):

1. **Sort entities by `EntityId.ulid().0` ascending** — `u128` compare; ULID's monotonic timestamp + random tail produces a stable cross-run order.
2. **Sort registered components by `snapshot_name()` ascending** — lexicographic on the static-string name. The TypeId-keyed BTreeMap of `SnapshotFns` is re-collected and re-sorted by name for the wire format because component-type identity (type_id) is process-local and not stable across runs.
3. **Walk entities × components, emit framed records.**

`restore_from_snapshot` is a clean-slate operation: despawn all current entities first, then re-spawn from the stream. Components in the stream whose type is not registered on this `World` are skipped with a `tracing::warn` for visibility (NOT a hard error — matches the `SnapshotError::UnknownComponent` doc-comment "only produced when the caller explicitly opts into strict mode"). The original `EntityId` is preserved via [`spawn_with_id`] so cross-call identity continues to resolve.

## 9. The `RGES` envelope format

Per the module-doc at `kernel/ecs/src/snapshot.rs`:

```text
magic:           [u8; 4]   = b"RGES"
version:         u16 LE    = 2
entity_count:    u32 LE
for each entity (sorted by EntityId / ULID u128 ascending):
  entity_id:     u128 LE
  comp_count:    u32 LE
  for each component (sorted by snapshot_name() ascending):
    name_len:    u32 LE
    name_bytes:  [u8; name_len]
    payload_len: u32 LE
    payload:     [u8; payload_len]   (postcard-encoded)
```

All integers are little-endian. Per-component payload is **postcard** (compact binary serde) — this is v2; v1 used RON and v2 reduced payload size by ~5-10×. v1 snapshots are not readable by v2 (bump-only migration).

The framing macros (`read_bytes!` / `read_u16!` / `read_u32!` / `read_u128!`) bound-check before slicing, so OOB reads on a truncated buffer are impossible by construction; truncation surfaces as `SnapshotError::Truncated(offset)`. Cross-ref `PIE_SNAPSHOT.md` §"Format policy" for the participant-level envelope (`RGEP`) that wraps `world_bytes` plus participant payloads.

## 10. `SnapshotError`

```rust
pub enum SnapshotError {
    Serde(String),
    BadMagic([u8; 4]),
    BadVersion(u16),
    Truncated(usize),
    UnknownComponent(String),
}
```

`Serde` wraps postcard errors. `BadMagic` / `BadVersion` / `Truncated` are envelope-level deserialization errors. `UnknownComponent` is reserved for callers that opt into strict mode; the default restore path emits `tracing::warn` and skips. `PieSnapshot`'s `ParticipateError::World(SnapshotError)` wraps these for the higher-level orchestrator (sibling: `PIE_SNAPSHOT.md` §4).

## 11. Performance characteristics

Documented in W03 / Phase 5.3 benchmarks per HANDOFF.md:

- **100k spawn + 10k mutate + Changed query in 0.24s debug.** Single-archetype iteration is dominated by the linear scan; debug-build overhead is the main contributor.
- **10k-entity round-trip in 13.6ms `--release`.** vs the 500ms PLAN gate — 36× headroom. The round-trip includes `serialize_snapshot` + `restore_from_snapshot` for a 10k-entity scene with three registered components.

These numbers cover the "cad scene of meaningful size" envelope. Subsystem-specific perf (cad-core operator graph, cad-projection bridge bookkeeping) lives in their respective §18 docs.

## 12. Failure class

`kernel/ecs` declares `//! Failure class: recoverable` per PLAN §1.13 (see `kernel/ecs/src/lib.rs`). The substrate itself doesn't fail catastrophically. Component-deserialize failures surface via `SnapshotError::Serde`; restoration is best-effort per-component (caller decides whether one bad component fails the whole restore by inspecting the returned `Result`).

`despawn` of a missing entity emits a `tracing::warn` and returns `false`; `insert` of a missing entity is a `tracing::warn` no-op. These are recoverable invariant violations — the caller can choose to handle or ignore them. The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/ecs` does not appear in the failure-class exemptions table.

## 13. References

- **PLAN.md §1.5.2** — kernel ECS substrate definition (the `World` slot in the kernel-tier diagram).
- **PLAN.md §6.13** — `SnapshotComponent` registry layer; `world_bytes` ⊂ `PieSnapshot`.
- **PLAN.md §6.16** — Command Bus mutation discipline; the `command-bus` lint enforces direct-mutation imports outside `crates/editor-actions/`.
- **PLAN.md §1.13** — failure-class taxonomy.
- **`PIE_SNAPSHOT.md`** — sibling §18 doc; uses `World::serialize_snapshot` / `restore_from_snapshot` for the `world_bytes` layer of `PieSnapshot`.
- **`kernel/ecs/src/lib.rs`** — module roots + free-function mutation aliases.
- **`kernel/ecs/src/world.rs`** — `World` root container; spawn / despawn / insert / remove / replace / query / commands / relations / resources.
- **`kernel/ecs/src/snapshot.rs`** — `SnapshotComponent` trait, `SnapshotFns` registry, `serialize_snapshot` / `restore_from_snapshot`, `RGES` envelope.
- **`kernel/ecs/src/storage/mod.rs`** — `TreeRelationStorage`, `DenseLinearRelationStorage`, `SparseRelationStorage`.
- **`kernel/ecs/src/relations/mod.rs`** — `RelationTag` trait, `ParentOf` / `BoneOf` / `LodOf` / `TemplateOf` markers, `parent_of` / `bone_of` free-function helpers.
- **`kernel/ecs/src/change_detection/mod.rs`** — `Mut<T>` guard, `Changed<T>` filter, `QueryFilter` sealed trait.
- **`kernel/ecs/src/entity.rs`** — `EntityId`, `EntityRef`, `EntityMut`.
