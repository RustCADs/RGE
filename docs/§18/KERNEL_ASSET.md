# KERNEL_ASSET

| Companion to | PLAN.md §1.6 (save file standards / content-addressed assets) + PLAN.md §10.1 (Tier-1 kernel crate list) + IMPLEMENTATION.md Phase 4.1 (kernel/asset exit criteria); Phase 4.2 (`crates/pak-format`) and Phase 4.3 (`crates/rge-data`) are sibling concerns covered in future §18 docs (`PAK_FORMAT.md` / `RGE_DATA.md` deferred) |
|---|---|
| Status | Stable v1; 53 tests passing (46 unit + 7 integration); Phase 4.1 done per Status.md 2026-05-09; canonical owner reconciliation closed 2026-05-06 (all 7 duplicate `AssetId` definitions migrated to `pub use rge_kernel_asset::AssetId;`); post-2026-05-09 migration to `Graph<AssetId, ()>` substrate per audit-1 followup |
| Audience | Authors loading / cooking assets through the registry; consumers of `Handle<T>` / `AssetId`; consumers wiring `DependencyGraph` invalidation (asset-pipeline, hot-reload-watcher); reviewers verifying the substrate-reuse migration (audit-1 graph-foundation Check 2 catch) |
| Sibling doc | `GRAPH_FOUNDATION.md` — substrate the `DependencyGraph` is now backed by; `RECOVERY_MODEL.md` — snapshot-recoverable failure-class story; future `PAK_FORMAT.md` (Phase 4.2) / `RGE_DATA.md` (Phase 4.3) for the disk-side codecs that consume `AssetId` |
| Reference impls | `kernel/asset/src/lib.rs` (28L) · `kernel/asset/src/id.rs` (354L; `AssetId` + 14 unit tests) · `kernel/asset/src/handle.rs` (213L; `Handle<T>` + 7 unit tests) · `kernel/asset/src/registry.rs` (460L; `Registry` + 14 unit tests) · `kernel/asset/src/dependency_graph.rs` (434L; `DependencyGraph` + 10 unit tests) · 4 integration test files (7 tests total) at `kernel/asset/tests/` |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the content-addressed asset substrate. Disk-side persistence (the `.rge-pak` cooked container; the imported source bytes' on-disk layout) lives in `crates/pak-format` + `crates/asset-store` and is covered by future §18 docs.

## 1. Why a substrate

Every cooked asset, every imported source file, every project-saved scene needs a stable identifier that survives across machines, toolchain bumps, and incremental recompiles. Without a substrate, each downstream system would invent its own id scheme: pak-format would reach for one BLAKE3 recipe, asset-pipeline would reach for another, rge-data would fork. Three months later, the workspace would carry 7+ `pub struct AssetId` definitions (the actual pre-reconciliation count per Status.md 2026-05-06).

PLAN §1.6 commits to **content-addressed assets** as a determinism + dedup primitive: same source bytes → same id, regardless of where in the workspace the cook ran. `kernel/asset` is the canonical home for that id and the runtime registry that owns the live payloads.

Three load-bearing properties:

- **`AssetId` is content-derived.** A 32-byte BLAKE3 hash of the asset's payload bytes, formatted as `"blake3:<64-hex-lowercase>"`. Same bytes on any machine produce the same id. Asset files written today round-trip against builds tomorrow (cross-machine determinism is regression-pinned by the `cross_machine_determinism_known_vectors` test against the empty-string and `"abc"` BLAKE3 vectors).
- **`Handle<T>` is type-erased on the registry side, type-safe on the consumer side.** `Registry` stores `Box<dyn Any + Send + Sync>` payloads keyed by `AssetId`; `Handle<T>` carries a `PhantomData<fn() -> T>` so the type parameter is preserved at the consumer surface without affecting `Send + Sync`. Safe `downcast_ref<T>` / `downcast_mut<T>` retrieve the typed payload; mismatched types surface as `RegistryError::TypeMismatch { id, stored, requested }`.
- **`DependencyGraph` reuses `kernel/graph-foundation`.** Post-2026-05-09 migration replaced the original `BTreeMap<AssetId, BTreeSet<AssetId>>` adjacency with `Graph<AssetId, ()>` per PLAN §1.14 substrate doctrine. The `graph-foundation` architecture lint (Check 2) caught kernel/asset reinventing the substrate during audit-1; the migration cleared the catch.

## 2. `AssetId` — content-addressed identifier

Lives at `kernel/asset/src/id.rs`. The 32-byte BLAKE3 digest formatted as `"blake3:<64-hex-lowercase>"`:

```rust
pub struct AssetId {
    bytes: [u8; 32],   // serialized via Display/FromStr
}

impl AssetId {
    pub fn from_bytes(bytes: &[u8]) -> Self;       // canonical: blake3::hash(bytes)
    pub const fn from_raw(raw: [u8; 32]) -> Self;  // bypass: raw digest already in hand
    pub const fn raw(&self) -> &[u8; 32];          // borrow underlying digest
    pub fn hex(&self) -> String;                   // 64-char lowercase hex (no prefix)
}
```

### Canonical owner reconciliation (2026-05-06)

The crate is the single canonical owner of `AssetId` per Status.md "AssetId canonical owner reconciliation" closure. Pre-reconciliation, 7 different crates (`asset-store`, `pak-format`, `rge-data`, `components-{animation,audio,identity,render}`) each carried their own `pub struct AssetId` with subtly different APIs. Post-reconciliation:

- All 7 use `pub use rge_kernel_asset::AssetId;` at the crate root.
- Per-crate call sites updated mechanically: `as_bytes()` → `raw()`; `from_content` / `from_digest` → `from_bytes` / `from_raw`; `display()` → `to_string()`; `AssetId::NULL` → per-crate `NULL_ASSET_ID` const wrapping `AssetId::from_raw([0u8; 32])`.
- 8 component structs got manual `Default` impls (kernel/asset deliberately omits `Default` because a default 32-byte digest would be meaningless).
- Workspace-wide grep confirms only one `pub struct AssetId` remains (the canonical one in `kernel/asset/src/id.rs`).

The cross-crate compatibility test `kernel/asset/tests/asset_id_compat_with_asset_store.rs` pins both the text-form match and the `cross_machine_determinism_known_vectors` empty-input + `"abc"` vectors against `asset-store`'s pre-reconciliation convention — so the migration is mechanical, no data conversion needed.

### String form: `"blake3:<hex>"`

The `Display` / `FromStr` / serde representations all use the prefix-discriminated string form rather than raw bytes. Per the lib.rs design rationale:

1. `.rge-scene` / `.rge-project` files are RON — humans benefit from grep-able asset references.
2. The `blake3:` prefix is a discriminator for future hash-family migration (ADR-077 escape-clause discipline). A future SHA3-256 algorithm bump would use `sha3:` prefix; the parser routes on prefix.
3. URL-safe and filename-safe — usable in HTTP-cooked asset URLs and cross-platform paths.

Parse errors are granular (so a corrupted `.rge-scene` line surfaces *which* part is malformed):

```rust
pub enum AssetIdParseError {
    MissingPrefix,                          // not "blake3:..."
    BadLength { expected: usize, got: usize },  // hex body wrong length
    BadHex,                                 // non-hex character in body
}
```

The parser accepts uppercase hex on read but `Display` always emits lowercase — round-trip discipline.

## 3. `Handle<T>` — typed ref-counted reference

Lives at `kernel/asset/src/handle.rs`. The cheap-to-clone reference:

```rust
pub struct Handle<T> {
    id: AssetId,
    rc: Arc<HandleStrong>,                  // strong-count token
    _marker: PhantomData<fn() -> T>,        // type tag
}
```

The handle does NOT own the asset payload — ownership lives in the `Registry`. The `Arc` ref-count signals to the registry when an asset has no live references and is eligible for GC via `Registry::sweep_orphans`.

### `unsafe_code = forbid` Send/Sync derivation

The crate's `lib.rs` carries `#![forbid(unsafe_code)]` (per workspace convention). `Handle<T>` is `Send + Sync` regardless of `T` because:

- `AssetId` is `Copy + Send + Sync` (it's just `[u8; 32]`).
- `Arc<HandleStrong>` is `Send + Sync` (`HandleStrong` is a unit struct with no `T` data).
- `PhantomData<fn() -> T>` is `Send + Sync` for all `T` — raw function pointer types are always `Send + Sync`, and `PhantomData` inherits that.

The compiler derives both bounds automatically — no `unsafe impl` needed. Using `fn() -> T` rather than `*const T` keeps `Send + Sync` regardless of `T`'s thread-safety bounds; if a `Handle<NotSendNotSync>` could exist, the consumer would still be free to send the handle across threads (the payload stays in the registry; the handle's own state is `Send + Sync` invariant).

### Equality + hashing by `AssetId`

```rust
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool { self.id == other.id }
}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
```

Two handles compare equal when they refer to the same `AssetId`, regardless of `Arc` pointer identity. This means `HashSet<Handle<T>>::insert` deduplicates by id (the `handle_is_hashable` unit test pins this).

`Debug` is hand-rolled to render the type name + id + strong count: `Handle { type: "alloc::string::String", id: blake3:..., strong_count: 2 }`.

## 4. `Registry` — the in-memory asset store

Lives at `kernel/asset/src/registry.rs`. Type-erased payload storage + integrated dependency graph:

```rust
pub struct Registry {
    payloads: HashMap<AssetId, RegistryEntry>,
    deps: DependencyGraph,
}

struct RegistryEntry {
    payload: Box<dyn Any + Send + Sync>,    // type-erased
    strong: Weak<HandleStrong>,             // weak ref to ref-count token
    type_name: &'static str,                // for error messages
}
```

### Insertion / retrieval surface

```rust
impl Registry {
    pub fn new() -> Self;
    pub fn insert<T: Send + Sync + 'static>(&mut self, id: AssetId, payload: T) -> Handle<T>;
    pub fn handle<T: Send + Sync + 'static>(&self, id: AssetId) -> Result<Option<Handle<T>>, RegistryError>;
    pub fn get<T: Send + Sync + 'static>(&self, id: AssetId) -> Result<Option<&T>, RegistryError>;
    pub fn get_mut<T: Send + Sync + 'static>(&mut self, id: AssetId) -> Result<Option<&mut T>, RegistryError>;
    pub fn remove(&mut self, id: AssetId) -> bool;
    pub fn sweep_orphans(&mut self) -> usize;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn ids(&self) -> impl Iterator<Item = AssetId> + '_;
    pub fn deps(&self) -> &DependencyGraph;
    pub fn deps_mut(&mut self) -> &mut DependencyGraph;
    pub fn serialize_deps(&self) -> Result<String, RegistryError>;
    pub fn restore_deps(&mut self, ron_text: &str) -> Result<(), RegistryError>;
}
```

`insert` always succeeds; if an asset already exists at `id`, the old payload is replaced and a `tracing::warn` is emitted (replacing is intentional API for hot-reload but unusual enough to log). `handle` resurrects a strong-count `Arc` if the previous one was dropped and swept; `get` / `get_mut` borrow the typed payload via safe `downcast_ref` / `downcast_mut`.

### `RegistryError`

```rust
pub enum RegistryError {
    NotFound(AssetId),
    TypeMismatch { id: AssetId, stored: &'static str, requested: &'static str },
    DiskError(String),
}
```

`NotFound` is currently unused at the public API level (`get` and `handle` return `Ok(None)` for missing ids; the variant is reserved for future fail-fast paths). `TypeMismatch` carries both the stored and requested type names — actionable for the caller to fix the dispatch. `DiskError` carries a `String` (forwarded from `ron::Error::to_string()`) for `serialize_deps` / `restore_deps` failures.

### Disk persistence: deps-only

Only the **dependency graph** is serialized to disk via `serialize_deps` / `restore_deps`. Asset payloads are stored separately (see `crates/asset-store` for the on-disk content-addressed cache layout). This keeps the registry thin and avoids a dependency on any particular asset format. The Registry deliberately does NOT serialize `payloads: HashMap<AssetId, RegistryEntry>` — type-erased `Box<dyn Any>` is not serializable.

## 5. `DependencyGraph` — substrate-backed dep tracking

Lives at `kernel/asset/src/dependency_graph.rs`. Post-2026-05-09 migration to `kernel/graph-foundation::Graph<AssetId, ()>` per PLAN §1.14 substrate doctrine. Edge `A → B` means "A depends on B".

### The `Graph<AssetId, ()>` substrate

```rust
pub struct DependencyGraph {
    inner: Graph<AssetId, ()>,
}
```

Each `AssetId` used as an edge endpoint is auto-promoted to a graph node, with its `NodeId` derived via `NodeId::from_bytes(asset_id.raw())` over the 32-byte BLAKE3 digest. The mapping is therefore **deterministic and reversible**: we recover the `AssetId` from a `NodeId` by looking up the node payload in the underlying `Graph`. Edge ids derive from `(src_node_id, dst_node_id)` so re-adding the same pair always produces the same `EdgeId`.

### Surface

```rust
impl DependencyGraph {
    pub fn new() -> Self;
    pub fn add_edge(&mut self, dependent: AssetId, dep: AssetId);
    pub fn remove_edge(&mut self, dependent: AssetId, dep: AssetId) -> bool;
    pub fn dependencies(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_;
    pub fn dependents(&self, id: AssetId) -> impl Iterator<Item = AssetId> + '_;
    pub fn transitive_dependents(&self, id: AssetId) -> Vec<AssetId>;
    pub fn remove_node(&mut self, id: AssetId);
    pub fn edge_count(&self) -> usize;
    pub fn node_count(&self) -> usize;
    pub fn detect_cycle(&self) -> Option<Vec<AssetId>>;
}
```

### Idempotent vs. graph-foundation's stricter contract

`Graph::insert_node` errors on duplicate ids; `Graph::insert_edge` errors on duplicate edge ids. The original `DependencyGraph::add_edge` semantics are idempotent — calling twice with the same pair has no extra effect — so the wrappers swallow `DuplicateNode` / `DuplicateEdge` rather than propagating. The `add_edge_is_idempotent` unit test pins this.

### Cycle detection (local DFS, substrate doesn't ship one)

`graph-foundation::Graph<N, E>` does NOT ship cycle detection (consistent with `cad-core::OperatorGraph`, which also implements its own; see `CAD_CORE_MODEL.md` §4 "Cycle detection"). `DependencyGraph::detect_cycle` implements DFS locally on top of the substrate's iteration API, operating over `AssetId` directly so the returned cycle path is in the domain type. The 3 cycle-detection unit tests pin two-cycle, three-cycle, and DAG cases.

The DFS entry point collects starts into a `BTreeSet<AssetId>` first so the traversal is reproducible regardless of the substrate's `NodeId`-keyed iteration order (`NodeId`s are u128-keyed, not directly `AssetId`-ordered).

### `transitive_dependents` BFS (deterministic order)

```text
1. Seed queue from direct dependents (BTreeSet ordering).
2. Pop front; for each parent in BTreeSet ordering, mark visited.
3. Continue until queue empty.
4. Return visited set in BTreeSet (sorted) order.
```

Used to compute "what gets invalidated if `id` changes". Result does NOT include `id` itself. The `transitive_dependents_returns_deterministic_order` unit test + `transitive_dependents_propagates_through_chain` integration test pin both the chain-walk semantics and the same-call-twice-returns-same-slice determinism property.

### `node_count` semantics post-migration

Per the post-2026-05-09 migration commentary in dependency_graph.rs: nodes are added on `add_edge` (both endpoints) and removed only by explicit `remove_node`. `remove_edge` does NOT prune zero-degree endpoint nodes (substrate invariant: node identity is content-derived from `AssetId` so the node remains discoverable as long as the `AssetId` has been mentioned). Mirrors `asset-store::DepGraph` behaviour — observable only via this internal accessor (no public API depends on auto-prune).

## 6. Sibling-mirror: `asset-store/src/dependency.rs` follows Option B template

The Tier-2 follower pattern. `crates/asset-store/src/dependency.rs` is the cooker-side dependency tracker (cooked-pak depends on imported glTF; build-pipeline routes invalidation through it). Per its module-doc (lines 22-26): *"Mirrors the migration applied to `kernel/asset::DependencyGraph` (audit-1 followup, 2026-05-09)."*

The two crates intentionally share the migration template:

- Both use `Graph<AssetId, ()>` substrate.
- Both auto-promote `AssetId` endpoints to `NodeId::from_bytes(asset_id.raw())`.
- Both swallow `DuplicateNode` / `DuplicateEdge` for idempotent semantics.
- Both implement DFS cycle detection locally (substrate doesn't ship one).
- Both return `BTreeSet`-sorted iteration for cross-platform determinism.

The differences are domain-shaped: `asset-store::DepGraph::add_edge` rejects self-edges with `DepError::SelfEdge`; `kernel/asset::DependencyGraph::add_edge` is `pub fn ... -> ()` (idempotent, no error path). `asset-store::DepGraph` carries a `transitive_closure` that returns `Result<_, DepError::Cycle>`; `kernel/asset::DependencyGraph::transitive_dependents` does NOT propagate cycle errors (the BFS visits each node at most once via the `BTreeSet<AssetId> visited` guard).

The intentional duplication is per PLAN §1.14 doctrine: **substrate primitives are shared; domain semantics aren't**. Both crates use the substrate identically; both apply their own domain-specific surface on top. Cross-ref `GRAPH_FOUNDATION.md` §6 for the substrate-reuse pattern.

## 7. The 53-test coverage breakdown

### Unit tests in `src/` (46 total)

- `id.rs` (15): `from_bytes_is_deterministic`, `from_bytes_is_blake3_of_input`, `different_bytes_produce_different_ids`, `display_is_blake3_prefixed_lowercase_64_hex_chars`, `hex_matches_blake3_to_hex_helper`, `round_trip_through_display_and_from_str`, `from_str_rejects_missing_prefix`, `from_str_rejects_wrong_length`, `from_str_rejects_non_hex_character`, `from_str_accepts_uppercase_hex`, `from_raw_round_trips_through_raw`, `cross_machine_determinism_known_vectors`, `hash_and_eq_use_underlying_bytes`, `ord_is_total_for_btreemap`, `serde_round_trips_via_string`.
- `handle.rs` (7): `handle_id_matches_construction_id`, `clone_increments_strong_count`, `drop_decrements_strong_count`, `equality_by_id`, `inequality_for_different_ids`, `handle_is_hashable`, `debug_contains_type_and_id`.
- `registry.rs` (14): `insert_returns_handle_with_matching_id`, `second_insert_at_same_id_replaces`, `get_returns_none_for_missing_id`, `get_errors_on_type_mismatch`, `handle_returns_none_for_missing_id`, `handle_errors_on_type_mismatch`, `get_mut_can_modify_payload`, `remove_returns_true_when_existed`, `remove_returns_false_when_not_present`, `len_and_is_empty`, `sweep_orphans_removes_entries_with_no_live_handles`, `sweep_orphans_keeps_entries_with_live_handles`, `dep_graph_accessible_via_registry`, `serialize_and_restore_deps_round_trips`.
- `dependency_graph.rs` (10): `add_edge_populates_both_forward_and_reverse`, `add_edge_is_idempotent`, `remove_edge_returns_true_when_existed`, `remove_edge_returns_false_when_not_present`, `transitive_dependents_returns_deterministic_order`, `detect_cycle_finds_two_cycle`, `detect_cycle_finds_three_cycle`, `detect_cycle_returns_none_for_dag`, `remove_node_cleans_both_directions`, `node_count_and_edge_count`.

### Integration tests in `tests/` (7 total)

- `asset_id_compat_with_asset_store.rs` (2): `asset_id_text_form_matches_asset_store_convention`, `known_vector_matches_asset_store_cross_machine_determinism`.
- `dependency_invalidation.rs` (2): `transitive_dependents_propagates_through_chain`, `transitive_dependents_after_partial_removal`.
- `handle_lifecycle.rs` (2): `handle_lifecycle_and_sweep`, `multiple_assets_independent_lifecycles`.
- `registry_round_trip.rs` (1): `dependency_graph_round_trips_via_ron`.

Together the 53 tests cover the cross-machine determinism contract, the typed-handle ref-count lifecycle, the type-erased registry's `TypeMismatch` discrimination, the substrate-backed dep graph's BFS + cycle-detection semantics, and the asset-store cross-crate compatibility.

## 8. Failure class — snapshot-recoverable

`kernel/asset/src/lib.rs` lines 1-10 declare:

```rust
//! `rge-kernel-asset` — canonical content-addressed asset substrate.
//!
//! Failure class: snapshot-recoverable
//!
//! Content-addressed asset substrate per IMPLEMENTATION.md Phase 4.1.
//!
//! Registry corruption (missing asset, dangling Handle) is recoverable by
//! re-loading payloads from disk and replaying the dependency graph. Plain
//! `recoverable` would imply "drop and continue" which loses the dep graph;
//! `snapshot-recoverable` is the precise class.
```

The lib-level explanation makes the choice explicit: registry corruption (a missing payload entry; a dangling `Handle` pointing at a swept slot) is recoverable VIA snapshot — the canonical recovery path is to re-load asset payloads from the on-disk content-addressed store and replay the persisted dependency graph (`serialize_deps` / `restore_deps`). Plain `recoverable` would imply "drop and continue", which would lose the dep graph and break invalidation propagation downstream. Snapshot-recoverable is the precise class.

The `architecture-lints` `failure-class` lint enforces the lib.rs declaration; `kernel/asset` does not appear in `tools/architecture-lints/exemptions.toml`.

## 9. Source / spec inconsistencies

- **Brief stated `unsafe_code = forbid` for handle Send/Sync derivation**; source-truth: the lib.rs does NOT carry an explicit `#![forbid(unsafe_code)]` attribute (only the workspace-level lints at `[lints] workspace = true`). The Send/Sync property IS achieved without `unsafe impl` (the compiler auto-derives both bounds because `AssetId: Send + Sync`, `Arc<HandleStrong>: Send + Sync`, `PhantomData<fn() -> T>: Send + Sync` for all `T`). The brief's framing is correct in spirit; the doc reflects the actual mechanism (compiler auto-derivation, no `unsafe` needed) without claiming the explicit attribute.
- **Brief stated NodeId derivation is via `NodeId::from_bytes(asset_id.raw())`**; source-truth confirmed: `dependency_graph.rs` line 285-287 uses exactly this. The mapping is documented as "deterministic and reversible" — node payload lookup recovers the `AssetId` from the `NodeId`. ✓
- **Brief stated 53-test breakdown as "46 unit + 7 integration"**; source-truth confirmed via `grep -c '#\[test\]'`: 46 unit tests across `src/` + 7 integration tests across `tests/` = 53. The breakdown matches Status.md line 24. ✓
- **Brief stated `Default` derivation on Registry but kernel/asset deliberately omits Default on AssetId**; source-truth: `Registry` derives `Default` (via `#[derive(Default)]` on the struct); `AssetId` does NOT derive `Default` and the lib.rs design rationale calls this out explicitly ("8 component structs got manual `Default` impls — kernel/asset deliberately omits Default" per Status.md line 27). The doc reflects both source-truths.
- **Brief stated DependencyGraph migration was caught by "graph-foundation lint Check 2"**; source-truth: the audit-1 followup is documented in the `dependency_graph.rs` module-doc lines 6-11 + 22-26. The graph-foundation lint exemption comment in `exemptions.toml` mentions only the editor-ui LayoutNodeId false-positive (no kernel/asset entry — meaning kernel/asset compiles cleanly against the lint post-migration). The brief's "Check 2 catch" framing aligns with the audit-1 finding rationale; the doc reflects this.

## 10. References

- **PLAN.md §1.6** — save file standards / content-addressed assets; the `AssetId` content-derivation contract.
- **PLAN.md §10.1** — Tier-1 kernel crate list; `kernel/asset` as the canonical content-addressed asset loader.
- **PLAN.md §1.13** — failure-class taxonomy; snapshot-recoverable rationale.
- **PLAN.md §1.14** — graph-foundation substrate doctrine; the migration target for `DependencyGraph`.
- **IMPLEMENTATION.md Phase 4.1** — `kernel/asset` exit criteria; the 53-test gate.
- **`GRAPH_FOUNDATION.md`** — sibling §18 doc; the `Graph<N, E>` substrate `DependencyGraph` is now backed by; `NodeId::from_bytes` derivation reference.
- **`RECOVERY_MODEL.md`** — sibling §18 doc; the snapshot-recoverable failure-class story.
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; precedent for "substrate doesn't ship cycle detection; consumer implements DFS locally".
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; uses BLAKE3 for `EventId` (cross-substrate identity discriminator); `AssetId` uses BLAKE3 over payload bytes (different domain).
- **`kernel/asset/src/lib.rs`** — module roots + failure-class declaration + recovery-model paragraph.
- **`kernel/asset/src/id.rs`** — `AssetId` + 15 unit tests + the BLAKE3 hash + `"blake3:<hex>"` text-form discipline.
- **`kernel/asset/src/handle.rs`** — `Handle<T>` + `HandleStrong` ref-count token + 7 unit tests.
- **`kernel/asset/src/registry.rs`** — `Registry` + `RegistryError` + 14 unit tests + `serialize_deps` / `restore_deps` disk persistence (deps-only).
- **`kernel/asset/src/dependency_graph.rs`** — `DependencyGraph` + post-migration `Graph<AssetId, ()>` substrate + 10 unit tests + DFS cycle detection.
- **`kernel/asset/tests/asset_id_compat_with_asset_store.rs`** — cross-crate compatibility regression (`AssetId` text form + cross-machine determinism vectors).
- **`kernel/asset/tests/dependency_invalidation.rs`** — `transitive_dependents` chain + partial-removal regression.
- **`kernel/asset/tests/handle_lifecycle.rs`** — ref-count + `sweep_orphans` regression.
- **`kernel/asset/tests/registry_round_trip.rs`** — dependency-graph RON round-trip regression.
- **`crates/asset-store/src/dependency.rs`** — sibling Tier-2 follower; same Option B substrate-migration template (mirrors `kernel/asset::DependencyGraph` per its 2026-05-09 module-doc).
