# GRAPH_FOUNDATION

| Companion to | PLAN.md §1.14 (graph systems substrate) |
|---|---|
| Status | Stable Tier-1 substrate; lint-enforced reuse across the workspace |
| Audience | Anyone building a graph-shaped subsystem (operator graphs, asset-dep graphs, ECS entity-relations, lineage graphs, editor-ui graph viewers, …) |
| Sibling doc | `CAD_CORE_MODEL.md` — first canonical consumer (`OperatorGraph` wraps `Graph<OperatorNode, EdgeKind>`); `PLUGIN_API.md` for layering convention |
| Reference impls | `kernel/graph-foundation/src/lib.rs` (47 tests) · `crates/cad-core/src/graph/operator_graph.rs` (`OperatorGraph` consumer) · future asset-dep graph / lineage graph backings (deferred per ADR-098) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. Each §18 doc carries the same five-row table; sections are numbered for ADR citation; code blocks stay <30 lines (link to the canonical `.rs` files for full impls).

## 1. Why a substrate

PLAN.md §1.14 names eight or more graph-shaped subsystems planned for the engine — the cad-core operator graph, the future asset-dependency graph, the future material/animation/script editor graph viewers, ECS entity-relations, the lineage graph (ADR-098), future editor layout reconcilers, and any cross-domain dependency tracker that needs invalidation propagation. Without a substrate, each one would invent its own `NodeId` / `EdgeId` / structural-hash story. Each invention would round-trip through different serialization shapes, diff differently, hash differently, and break interoperability between consumers that need to compose two graph-shaped views (e.g. ECS entity-relations × asset deps for hot-reload).

`kernel/graph-foundation` is the Tier-1 substrate that fixes this. It exposes content-derived 128-bit ids, a generic mutable graph container, an immutable Arc-shared snapshot type, a structural-diff type, a BFS invalidation router, and a viz-adapter trait — and **only** those. Domain-specific traversal, evaluation, and semantics live in each domain's own crate. Cross-domain semantic unification is explicitly out of scope (it would be the "god-substrate" anti-pattern).

The substrate's reuse is mechanically enforced. The `graph-foundation` architecture-lint scans every `.rs` file outside `kernel/graph-foundation/` and rejects any new top-level definition of `NodeId`, `EdgeId`, or `StableHash`. Adding one is a workspace-blocking lint failure; the only way past it is an exemption entry in `tools/architecture-lints/exemptions.toml` with a written justification (today there is exactly one — see §9).

## 2. Primitives — `NodeId`, `EdgeId`, `StableHash`

### `NodeId(u128)` and `EdgeId(u128)`

Both ids are 128-bit, BLAKE3-derived, deterministic across processes and platforms. The wire format is a 32-character lowercase hex string (`"0000000000000000000000000000000a"`) because RON — the workspace's snapshot format — does not natively support `u128`. Manual `Serialize` / `Deserialize` impls handle the hex encoding so callers don't see the constraint.

The constructor surface:

```rust
impl NodeId {
    pub fn from_bytes(bytes: &[u8]) -> Self;   // BLAKE3(bytes)[..16]
    pub const fn from_raw(raw: u128) -> Self;  // tests / migration
    pub fn to_hex(self) -> String;             // 32-char lowercase hex
}
```

`EdgeId` mirrors this and adds a convenience `from_endpoints(src: NodeId, dst: NodeId)` that mixes both endpoints into the hash so reversed `(src, dst)` pairs produce different ids.

The `Display` impl renders as `node:0x<hex>` / `edge:0x<hex>` for diagnostic output. `PartialOrd + Ord` use the underlying `u128` so iteration order via `BTreeMap` / `BTreeSet` is deterministic.

### `StableHash`

A 3-line trait that lets a domain feed its structural fields into a BLAKE3 hasher in deterministic (field-declaration) order:

```rust
pub trait StableHash {
    fn hash_into(&self, hasher: &mut blake3::Hasher);
}
```

Free functions `stable_node_id::<T: StableHash>(value: &T) -> NodeId` and `stable_edge_id::<T>(...) -> EdgeId` derive ids in one call. Blanket impls cover `u8 / u32 / u64 / u128 / str / String / [u8]` so most domain-side hash impls reduce to feeding their primitive fields.

The trait is considered sealed at v1 — graph systems should call the free functions rather than implementing `StableHash` directly until the API stabilises. The intent is that domain-side `StableHash` impls are the small minority; most consumers hash via `blake3::Hasher::new()` + `hasher.update(&bytes)` directly and feed the digest into `NodeId::from_bytes`. (Both `OperatorGraph::derive_node_id` and `derive_edge_id` use the direct-BLAKE3 path — see CAD_CORE_MODEL.md §5.)

## 3. `Graph<N, E>` — generic mutable container

Lives in `graph.rs`. Generic over node payload `N` and edge payload `E`; nodes keyed by `NodeId`, directed edges keyed by `EdgeId`. Storage is BTreeMap-backed:

```rust
pub struct Graph<N, E> {
    nodes: BTreeMap<NodeId, N>,
    edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    outgoing: BTreeMap<NodeId, BTreeSet<EdgeId>>,
    incoming: BTreeMap<NodeId, BTreeSet<EdgeId>>,
}
```

`BTreeMap` (not `HashMap`) so iteration order is deterministic — the same constraint that makes the workspace's snapshot serialization byte-identical across runs. The `outgoing` / `incoming` adjacency caches let consumers walk neighbours in O(degree) without scanning all edges.

The mutation surface returns `GraphError` for the standard mistakes:

- `DuplicateNode(NodeId)` — `insert_node` rejected because the id is already present.
- `DuplicateEdge(EdgeId)` — same for `insert_edge`.
- `DanglingEndpoint { src, dst }` — `insert_edge` rejected because `src` or `dst` doesn't exist as a node.
- `NodeNotFound(NodeId)` / `EdgeNotFound(EdgeId)` — lookups.

Removing a node cascades: every edge incident to it is removed first, then the node itself. This keeps the `incoming` / `outgoing` adjacency caches consistent without requiring callers to know the cascade order.

What `Graph<N, E>` deliberately does NOT provide: cycle detection, topological sort, evaluation, traversal. Domain-specific algorithms live in each domain's wrapper. `OperatorGraph` builds cycle detection on top via a `HashSet<NodeId>` ancestor stack inside its evaluator (see CAD_CORE_MODEL.md §5 and the Test 4 cycle-detection test in `operator_graph.rs`).

## 4. `GraphSnapshot<N, E>` — immutable Arc-shared

Lives in `snapshot.rs`. The capture-restore primitive that downstream `SnapshotParticipate` impls (`cad-core::CadGraph` per PLAN §13.2 — see CAD_CORE_MODEL.md §6) layer on top of:

```rust
pub struct GraphSnapshot<N, E> {
    nodes: Arc<BTreeMap<NodeId, N>>,
    edges: Arc<BTreeMap<EdgeId, EdgeRecord<E>>>,
}

impl<N: Clone, E: Clone> GraphSnapshot<N, E> {
    pub fn from_graph(graph: &Graph<N, E>) -> Self;
    pub fn to_graph(&self) -> Graph<N, E>;
    // ...
}
```

Cheap to clone (Arc-wrapped) so multiple subscribers can hold the same snapshot without duplicating heap storage.

Serde via the wire-format twin pattern: `Serialize` / `Deserialize` are hand-rolled to flatten the Arc through a private `SnapshotWire { nodes: BTreeMap<…>, edges: BTreeMap<…> }` struct so the standard derive works without enabling serde's `"rc"` feature. The wire-format is RON for human-inspectability; round-trip is byte-identical when iteration is deterministic (BTreeMap-backed).

`SnapshotError::Serialize(String) / Deserialize(String)` for round-trip failures. The `to_ron` / `from_ron` helpers wrap the `ron` calls so callers don't need a direct `ron` dep.

## 5. `GraphDiff<N, E>` — structural diff

Lives in `diff.rs`. Compares two snapshots and reports added / removed / changed nodes and edges:

```rust
pub struct GraphDiff<N, E> {
    pub added_nodes: BTreeMap<NodeId, N>,
    pub removed_nodes: BTreeMap<NodeId, N>,
    pub changed_nodes: BTreeMap<NodeId, (N, N)>,        // (old, new)
    pub added_edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    pub removed_edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    pub changed_edges: BTreeMap<EdgeId, (EdgeRecord<E>, EdgeRecord<E>)>,
}

impl<N: Clone + PartialEq, E: Clone + PartialEq> GraphDiff<N, E>
where EdgeRecord<E>: PartialEq
{
    pub fn between(old: &GraphSnapshot<N, E>, new: &GraphSnapshot<N, E>) -> Self;
}
```

`Default` is hand-rolled rather than derived — deriving `Default` would require `N: Default` / `E: Default` bounds even though the diff sets are empty.  The hand-roll constructs the empty BTreeMaps directly, freeing consumers from having to require `Default` on their payload types.

Used downstream by `Invalidation` propagation (a changed-node set is the natural input to `mark_dirty`), by snapshot replication (the differ produces a minimal patch), and by editor inspectors that want to highlight changed regions of a graph viewer.

## 6. `Invalidation` — BFS dirty-bit propagation

Lives in `invalidation.rs`. Routes dirty-bit signals to registered listeners and walks the dependency DAG transitively:

```rust
pub trait InvalidationListener: Send + 'static {
    fn on_invalidated(&mut self, node: NodeId);
}

pub struct Invalidation { /* ... */ }

impl Invalidation {
    pub fn register(&mut self, listener: Box<dyn InvalidationListener>) -> ListenerHandle;
    pub fn unregister(&mut self, handle: ListenerHandle) -> bool;
    pub fn mark_dirty<F>(&mut self, root: NodeId, dependents_of: F)
    where F: Fn(NodeId) -> Vec<NodeId>;
}
```

`mark_dirty` does BFS from `root`, calls each registered listener once per dirtied node, and dedupes via a `BTreeSet<NodeId>` visited set. The 4-node DAG and diamond-dedup tests in `kernel/graph-foundation/tests/` (and the unit tests inline in `invalidation.rs`) gate the invariant that diamond-shaped dependency graphs deliver `on_invalidated` exactly once per node, not once per arrival path.

The `dependents_of` closure is supplied by the caller because `Invalidation` itself doesn't own a `Graph<N, E>` — domain code passes a closure that walks its own graph's edges in the inverse direction (`A → B` means "B depends on A" so `dependents_of(A) = [B]`). This keeps `Invalidation` decoupled from any specific `N` / `E` payload shape.

`ListenerHandle` is opaque (private `u64` newtype) so subscribers can drop registrations without exposing handle internals to the host's API.

## 7. `VizAdapter`, `NodeView`, `EdgeView` — editor surface

Lives in `viz_adapter.rs`. The trait surface for editor graph-viewer widgets:

```rust
pub struct NodeView<'a> { pub id: NodeId, pub display_name: &'a str, pub kind: &'a str }
pub struct EdgeView<'a> { pub id: EdgeId, pub src: NodeId, pub dst: NodeId, pub label: &'a str }

pub trait VizAdapter {
    fn node_count(&self) -> usize;
    fn edge_count(&self) -> usize;
    fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_>;
    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_>;
}
```

Each domain (material / animation / script / CAD) implements `VizAdapter` on its own graph wrapper; the future editor's `widgets/node_graph.rs` consumes the trait without coupling to any domain's concrete `N` / `E` types. The view types borrow zero-copy from the underlying graph (`&'a str` for display strings).

This surface is stable but not yet consumed — no editor widget has landed. It exists so that when the first editor graph viewer dispatch fires, the substrate is already in place and the viewer doesn't need to re-derive a domain-neutral surface from scratch.

## 8. `EdgeRecord<E>` — edge payload carrier

```rust
pub struct EdgeRecord<E> {
    pub src: NodeId,
    pub dst: NodeId,
    pub data: E,
}
```

Returned by `Graph::edge(EdgeId)` and `GraphSnapshot` iteration. The split between `EdgeId` (identity) and `EdgeRecord<E>` (endpoints + payload) keeps the adjacency caches' edge-id lookups O(log n) without forcing every adjacency walk to clone the full payload.

## 9. The `graph-foundation` architecture-lint

Lives at `tools/architecture-lints/src/graph_foundation.rs` and is one of the nine workspace-gating lints (`cargo run -p rge-tool-architecture-lints -- all` exits 0 only when all nine pass). The lint runs **two checks**, both rooted in the same substrate doctrine.

### Check 1 — forbidden-name redefinition

What it enforces: no `pub struct NodeId`, `pub struct EdgeId`, `pub trait StableHash`, or any `enum / type / trait` definition with those exact names anywhere in the workspace except inside `kernel/graph-foundation/`. The implementation walks every `.rs` file via `iter_rust_files`, parses it with `syn`, and visits `ItemStruct / ItemEnum / ItemType / ItemTrait` nodes whose identifier matches the forbidden set.

### Check 2 — adjacency-map reinvention (added 2026-05-09)

What it enforces: no struct field of shape `BTreeMap<K, BTreeSet<K>>` or `HashMap<K, HashSet<K>>` where the outer-key type equals the inner-set's element type may be defined outside `kernel/graph-foundation/`. That shape is the canonical "I'm reinventing graph storage" pattern (an adjacency map). The proper substrate is `Graph<N, E>` (per §3 of this doc).

Why this check exists: audit-1 (deep audit 2026-05-09 round 1) found that `kernel/asset::DependencyGraph` had silently rolled its own graph via `BTreeMap<AssetId, BTreeSet<AssetId>>`. Check 1 didn't catch it because no `NodeId / EdgeId / StableHash` redefinition was involved. Check 2 was added per the audit followup; it immediately surfaced a bonus catch in `crates/asset-store/src/dependency.rs` (also `BTreeMap<AssetId, BTreeSet<AssetId>>` × 2, forward + reverse) which audit-1 missed entirely. Both crates were migrated to `Graph<AssetId, ()>` with content-derived `NodeId` via `NodeId::from_bytes(asset_id.raw())` mirroring the cad-core::OperatorGraph precedent.

How it works: extends the syn AST visitor's `visit_item_struct` to call a `detect_adjacency_map(&Type)` helper on each field type. The helper:

1. Outer type's last path segment must be `BTreeMap` or `HashMap`.
2. Type must have exactly 2 generic args.
3. Second arg must be a `BTreeSet` or `HashSet` with exactly 1 generic arg.
4. The outer key type and the inner set's element type must compare equal via `syn::Type`'s native `PartialEq` (enabled by syn's `extra-traits` feature).

False-positive guard: `BTreeMap<UserId, BTreeSet<Permission>>` (different key/element types) does NOT trigger — only K==V pairs do. Permission maps, capability sets, and tag-collection-by-category are all unaffected.

v1 scope: struct fields only. Function args / return types / type aliases / enum variants are NOT checked (workspace currently has zero such non-struct-field shapes; future-extension scope if a new pattern emerges).

### Current exemption

There is exactly one entry in `tools/architecture-lints/exemptions.toml`:

```toml
[[exemption]]
lint = "graph-foundation"
file = "crates/editor-ui/src/layout/node.rs"
reason = """Layout-tree NodeId is a string-based reconciler ID..."""
```

The layout reconciler's `NodeId` is conceptually distinct from the graph-substrate id — it's a string-based stable identifier used to reconcile editor panes across hot-reload, not a handle into a graph storage. The follow-up plan (recorded in the exemption's `reason` field) is to rename it to `LayoutNodeId` in a future cleanup pass to remove the false positive entirely.

### When to consult the lint while authoring a new graph subsystem

The mechanical test is `cargo run -p rge-tool-architecture-lints -- graph-foundation`. If your new subsystem fails this lint, you've defined a `NodeId / EdgeId / StableHash` rather than reusing the substrate's. The fix is not an exemption (those are reserved for the small set of conceptually-distinct reconciler-ID cases); the fix is to import `rge_kernel_graph_foundation::{NodeId, EdgeId, StableHash}` and either use them directly or wrap them in your own newtype that internally holds a `NodeId`.

## 10. References

- **PLAN.md §1.14** — graph systems substrate decision; lists the eight+ graph-shaped subsystems planned for the engine.
- **ADR-098** — topology lineage substrate; v0 stores `LineageGraph::edges` as a `Vec<LineageEdge>`, with `kernel/graph-foundation::Graph` migration deferred until traversal queries materialise (see ADR-098 §"Followups / open questions"). See sibling `CAD_TOPOLOGY_LINEAGE.md`.
- **`kernel/graph-foundation/src/lib.rs`** — the substrate root (47 tests).
- **`kernel/graph-foundation/src/id.rs`** — `NodeId` / `EdgeId` BLAKE3 + hex serde.
- **`kernel/graph-foundation/src/graph.rs`** — `Graph<N, E>` + `EdgeRecord<E>` + `GraphError`.
- **`kernel/graph-foundation/src/snapshot.rs`** — `GraphSnapshot<N, E>` + `SnapshotWire` + RON round-trip.
- **`kernel/graph-foundation/src/diff.rs`** — `GraphDiff<N, E>::between`.
- **`kernel/graph-foundation/src/invalidation.rs`** — `Invalidation` + `InvalidationListener`.
- **`kernel/graph-foundation/src/viz_adapter.rs`** — `VizAdapter` + `NodeView` / `EdgeView`.
- **`kernel/graph-foundation/src/stable_hash.rs`** — `StableHash` trait + free fns + primitive blanket impls.
- **`tools/architecture-lints/src/graph_foundation.rs`** — lint implementation.
- **`tools/architecture-lints/exemptions.toml`** — the single existing exemption.
- **`crates/cad-core/src/graph/operator_graph.rs`** — canonical consumer (`OperatorGraph` wraps `Graph<OperatorNode, EdgeKind>`); see sibling `CAD_CORE_MODEL.md` §5.
- **`crates/cad-core/src/checkpoints/mod.rs`** — uses `GraphSnapshot` for transactional capture / restore via `CheckpointHistory`.
- **`PLUGIN_API.md`** / **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 docs; convention origin.
