# CAD_CORE_MODEL

| Companion to | PLAN.md §1.5.4 (cad-core); ADR-104 (capability surface); ADR-112 (Boolean CSG library) |
|---|---|
| Status | Stable v0 (Phase 7.1 D-prime + D-Extrude + D-Revolve + D-Boolean shipped; PIE `SnapshotParticipate` closure landed 2026-05-07) |
| Audience | Operator authors; cad-projection consumers; future editor-ui operator-picker authors; future second-CAD-kernel authors (per ADR-113-deferred) |
| Sibling doc | `CAD_TOPOLOGY_LINEAGE.md` — face-lineage substrate that operators participate in via `output_is_labeled`; `GRAPH_FOUNDATION.md` — the `Graph<N, E>` primitive `OperatorGraph` wraps |
| Reference impls | `crates/cad-core/src/operators/` (5 v0 operators) · `crates/cad-core/src/graph/operator_graph.rs` (DAG + evaluator) · `crates/cad-core/src/checkpoints/` (transactional history + `SnapshotParticipate`) · `crates/cad-core/src/tessellation/` (output mesh + memoization cache) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. ADR-104 captures the capability-surface design (doc-comment-canonical until trigger fires); ADR-112 captures the Boolean CSG library choice; ADR-098 captures the topology lineage substrate (sibling doc). This doc captures the shape of the runtime model that downstream consumers build on.

**Elaborates**: REACTIVE_INVALIDATION.md §1 (Layer 1 — graph mutations / authoritative origin of every reactive ripple).

## 1. Three-layer model

```text
┌──────────────────────────────────────────────────────────────┐
│ TessellationCache  (HashMap<CacheKey, Arc<Tessellation>>)    │  ← memoization
├──────────────────────────────────────────────────────────────┤
│ CheckpointHistory  (BTreeMap<CheckpointId, GraphSnapshot>)   │  ← transactional history
├──────────────────────────────────────────────────────────────┤
│ OperatorGraph      (wraps Graph<OperatorNode, EdgeKind>)     │  ← the operator DAG
└──────────────────────────────────────────────────────────────┘
                             │
                             ▼
              kernel/graph-foundation::Graph<N, E>
```

Each layer is independent: `OperatorGraph`s operate without checkpoints (used directly by tests); `CheckpointHistory` captures and restores graphs without depending on the cache; the cache memoizes evaluation outputs without needing checkpoint context.

The integration point is `CadGraph` — a wrapper that owns both an `OperatorGraph` and a `CheckpointHistory`, threading the transactional discipline (begin / commit / rollback) over the underlying DAG so callers can roll back on operator-evaluation failure (per `//! Failure class: snapshot-recoverable`, see §11).

## 2. The `Operator` trait

Lives in `crates/cad-core/src/operators/mod.rs`. Uniform contract every operator implements:

```rust
pub trait Operator: std::fmt::Debug + Send + Sync {
    fn op_kind(&self) -> OpKind;
    fn arity(&self) -> usize;
    fn structural_hash(&self) -> [u8; 32];           // local hash, NOT recursive
    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError>;

    /// Predict whether the output Tessellation will carry face_labels given
    /// the labeled-state of each input. Default: any labeled input ⇒ labeled
    /// output. See CAD_TOPOLOGY_LINEAGE.md §7.
    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        inputs_labeled.iter().any(|b| *b)
    }
}
```

`OpKind` is the discriminant tag (cheap dispatch in inspectors without matching on the full payload-bearing enum):

```rust
pub enum OpKind { Boolean, Cuboid, Extrude, Revolve, Transform }
```

`EdgeKind` is the edge payload — `Input(port)` says "this edge feeds the downstream operator's `port`-th declared input". Future operators with multiple ordered inputs (e.g. Boolean's `lhs=0` / `rhs=1`) reuse the same variant:

```rust
pub enum EdgeKind { Input(u8) }
```

`OperatorNode` is the tagged-union enum the operator graph stores:

```rust
#[serde(tag = "kind")]
pub enum OperatorNode {
    Boolean(BooleanOp),
    Cuboid(CuboidOp),
    Extrude(ExtrudeOp),
    Revolve(RevolveOp),
    Transform(TransformOp),
}
```

`#[serde(tag = "kind")]` produces a stable wire representation (`{ "kind": "Cuboid", "width": 1.0, ... }`) that is forward-compatible when new variants are added. `OperatorNode::as_operator(&self) -> &dyn Operator` reborrows for uniform dispatch; the trait `impl Operator for OperatorNode` itself just delegates each method through `as_operator()`.

`OpError` is the common evaluation error:

```rust
pub enum OpError {
    WrongArity { expected: usize, got: usize },
    EmptyResult,
    InvalidParameter(String),
}
```

## 3. Operator catalog (5 v0 operators)

Each operator's source file is the canonical reference for its parameter set + evaluation algorithm. One-line each:

- **`CuboidOp`** (`operators/cuboid.rs`) — arity 0; origin-centered axis-aligned box primitive with `width / height / depth` parameters; closed-form generative.
- **`TransformOp`** (`operators/transform.rs`) — arity 1; affine TRS (translation / rotation as quaternion / scale) applied to one upstream tessellation. Overrides `output_is_labeled` to `false` (strips labels).
- **`ExtrudeOp`** (`operators/extrude.rs`) — arity 0; sweeps a 2D convex polygon profile along `+Z` to height `h`. Carries a `Polygon2D` profile (with its own `Polygon2DError` validation).
- **`RevolveOp`** (`operators/revolve/mod.rs` — sub-module post-Phase-5-split with `mod.rs` + `full_path.rs` + `partial_path.rs` + `tests.rs`) — arity 0; rotates a 2D profile around the Y-axis through 2π with `n` segments.
- **`BooleanOp`** (`operators/boolean/mod.rs` — sub-module post-Phase-5-split with `mod.rs` + `csgrs_bridge.rs` + `labeled_path.rs` + `unlabeled_path.rs` + `tests.rs`) — arity 2; `Union | Intersection | Difference` of two upstream tessellations via the csgrs CSG library (per ADR-112). lhs = `inputs[0]`, rhs = `inputs[1]`. The first cad-core operator with a Tier-3 dependency; the bridge wraps csgrs panics in `catch_unwind` and surfaces them as `OpError::InvalidParameter` per the snapshot-recoverable failure class.

`BooleanOp` is also the canonical lineage producer: it propagates labels through csgrs's polygon metadata and downstream `infer_lineage` consumes the labeled output via the labeled-path. See `CAD_TOPOLOGY_LINEAGE.md` §4 for the metadata-passthrough mechanics.

## 4. `OperatorGraph` — the DAG

Lives at `crates/cad-core/src/graph/operator_graph.rs`. Wraps `kernel/graph-foundation::Graph<OperatorNode, EdgeKind>` so snapshots from the substrate work without an intermediate wrapper:

```rust
pub struct OperatorGraph {
    graph: Graph<OperatorNode, EdgeKind>,
    root: Option<NodeId>,
}

impl OperatorGraph {
    pub fn new() -> Self;
    pub fn add_operator(&mut self, op: OperatorNode) -> Result<NodeId, GraphBuildError>;
    pub fn connect(&mut self, src: NodeId, dst: NodeId, port: u8) -> Result<EdgeId, GraphBuildError>;
    pub fn set_root(&mut self, node: NodeId) -> Result<(), GraphBuildError>;
    pub fn evaluate(&self, target: NodeId, cache: &mut TessellationCache, tolerance: Tolerance)
        -> Result<Arc<Tessellation>, EvalError>;
    // ...
}
```

### Content-derived `NodeId` (deduplication)

`add_operator` derives the `NodeId` from the serialized `OperatorNode` content via BLAKE3:

```text
NodeId = BLAKE3("cad-op:" || ron::to_string(operator_node))
```

So two `add_operator` calls with identical payloads collide (surface as `GraphError::DuplicateNode`). RON is the serialization format because graph-foundation already pulls it for snapshot serialization and the result is stable across builds (no nondeterministic ordering — operator structs serialize in field-declaration order). See `GRAPH_FOUNDATION.md` §2 for the `NodeId` substrate.

`EdgeId`s derive from `(src, dst, port)` so duplicate-edge errors trigger `GraphError::DuplicateEdge`.

### Cycle detection

`graph-foundation::Graph<N, E>` does NOT detect cycles itself (it accepts `A → B → A`). `OperatorGraph::evaluate` does, via a `HashSet<NodeId>` ancestor stack passed through the recursive evaluator: re-entering a node already on the stack returns `EvalError::Cycle`. See Test 4 in `operator_graph.rs` for the canonical regression test.

### `evaluate` recipe (recursive, memoizing)

```rust
fn eval_node(&self, node_id, cache, tolerance, stack) -> Result<Arc<Tessellation>, EvalError> {
    let (effective_hash, _output_labeled) = self.effective_hash_and_label(node_id, stack)?;
    let key = CacheKey { structural_hash: effective_hash, tolerance };
    if let Some(hit) = cache.get(&key) { return Ok(hit); }
    // Cache miss: recurse on inputs, evaluate, insert.
    let node = self.graph.node(node_id).ok_or(EvalError::NodeNotFound(node_id))?;
    let by_port = self.collect_incoming_by_port(node_id, node.arity())?;
    let upstream: Vec<Arc<Tessellation>> = by_port
        .iter()
        .map(|(_, src)| self.eval_node(*src, cache, tolerance, stack))
        .collect::<Result<_, _>>()?;
    let inputs: Vec<&Tessellation> = upstream.iter().map(AsRef::as_ref).collect();
    let tess = node.evaluate(&inputs)?;
    Ok(cache.insert(key, tess))
}
```

`collect_incoming_by_port` validates that incoming edges cover `0..arity` exactly once; surfaces violations as `EvalError::PortMismatch { expected_arity, got }`. Test 5 in `operator_graph.rs` covers this.

### `effective_hash_and_label` — the cache-key recipe

`effective_hash_and_label` is the **central correctness primitive**. It recursively combines `(local_structural_hash, port_index, upstream_effective_hash)` plus the upstream-labeled-bitmap to produce a 32-byte BLAKE3 digest that fully identifies a sub-tree's evaluation:

```text
hasher.update(node.structural_hash())
for (port, upstream_hash) in upstream_data.iter().enumerate() {
    hasher.update(&[port])
    hasher.update(upstream_hash)
}
hasher.update(&upstream_labeled_bitmap.to_le_bytes())   // CAD_TOPOLOGY_LINEAGE.md §8
effective_hash = hasher.finalize().as_bytes()
```

The bitmap fold is defense-in-depth against operators that forget to reflect label-emitting parameters in their local `structural_hash` (audit-2 finding A1.4 / A5.2 / Pairing N2). See `CAD_TOPOLOGY_LINEAGE.md` §8 for the rationale and Test 7 in `operator_graph.rs` for the canonical correctness regression ("change upstream parameter → downstream cache miss → vertex positions differ").

`EvalError`:

```rust
pub enum EvalError {
    Op(OpError),
    NodeNotFound(NodeId),
    RootNotFound,
    Cycle,
    PortMismatch { node: NodeId, expected_arity: usize, got: usize },
}
```

## 5. `CadGraph` + `CheckpointHistory` — transactional model

Lives at `crates/cad-core/src/checkpoints/mod.rs`. `CadGraph` is the integration point:

```rust
pub struct CadGraph {
    graph: OperatorGraph,
    history: CheckpointHistory,
}

impl CadGraph {
    pub fn graph(&self) -> &OperatorGraph;
    pub fn graph_mut(&mut self) -> Result<&mut OperatorGraph, CheckpointError>;  // gated
    pub fn begin_operation(&mut self) -> Result<(), CheckpointError>;
    pub fn commit(&mut self, label: impl Into<String>) -> Result<CheckpointId, CheckpointError>;
    pub fn rollback(&mut self) -> Result<(), CheckpointError>;
    pub fn restore_to(&mut self, id: CheckpointId) -> Result<(), CheckpointError>;
    pub fn head(&self) -> CheckpointId;
}
```

All mutations must occur inside a `begin_operation` / (`commit` | `rollback`) bracket. `graph_mut()` outside an open operation returns `CheckpointError::MutationOutsideOperation`. Reads (via `graph()`) are always allowed.

### The lifecycle

- `begin_operation` eagerly captures a `GraphSnapshot` of the current graph (cheap — Arc-wrapped per `GRAPH_FOUNDATION.md` §4) plus the current root, into a private `InProgress` record.
- `commit(label)` advances HEAD: drops the `InProgress` record, captures a fresh snapshot from the current graph, and stores it as a new `Checkpoint` keyed by a monotonic `CheckpointId(u64)` (root is `CheckpointId(0)`, subsequent commits are `1, 2, …`).
- `rollback()` restores: replaces the inner `Graph<N, E>` with the `InProgress` snapshot's `to_graph()` and re-`set_root`s. `head` does not move (the rolled-back operation never committed).
- `restore_to(id)` replays a historical snapshot: requires no in-progress transaction (else `CheckpointError::InProgressMustBeResolved`); looks up the checkpoint, replaces the graph + root, and moves `head` to the target id.

`Checkpoint` shape:

```rust
pub struct Checkpoint {
    pub id: CheckpointId,
    pub snapshot: GraphSnapshot<OperatorNode, EdgeKind>,
    pub root_at_checkpoint: Option<NodeId>,
    pub parent: Option<CheckpointId>,
    pub label: String,
}
```

`CheckpointHistory` is `BTreeMap<CheckpointId, Checkpoint>`-backed for deterministic iteration (same convention as `GRAPH_FOUNDATION.md` §3 `Graph` storage and `kernel/ecs` per PLAN §1.6.8).

### `SnapshotParticipate` impl (CRITICAL #1 closure 2026-05-07)

Per PLAN §13.2 (all stateful Tier-2 has `SnapshotParticipate`). Lives at `crates/cad-core/src/checkpoints/participate.rs` (414 LoC including its test suite). The `impl SnapshotParticipate for CadGraph` round-trips the entire `CadGraph` (graph + history + in-progress) via RON:

```rust
impl SnapshotParticipate for CadGraph {
    fn participant_id(&self) -> ParticipantId {
        ParticipantId::new("cad-core.cad-graph")
    }
    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        ron::to_string(self).map(|s| s.into_bytes()) // ...
    }
    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let restored: CadGraph = ron::from_str(std::str::from_utf8(bytes)?)?;
        *self = restored; Ok(())
    }
}
```

Why RON not postcard: `OperatorNode` derives `#[serde(tag = "kind")]` for forward-compat across new variants, and postcard does not support internally-tagged enum deserialization (it's a non-self-describing format). RON is self-describing and round-trips the tagged enum cleanly. `kernel/graph-foundation`'s `GraphSnapshot::to_ron` uses the same serialization choice (see `GRAPH_FOUNDATION.md` §4).

Convention: callers should register `CadGraph` and `CadProjection` together in the same `PieSnapshot::capture / restore` call. After restoring, callers should invoke `CadProjection::validate_handles` (in `rge-cad-projection`) with the restored cad-graph to detect any orphan `BRepHandle.cad_node` references — orphans indicate a divergent-state PIE payload (graph and projection captured at different times). The closure documents the silent-inconsistency window this PIE-participation closes.

## 6. `Tessellation` — output mesh

Lives at `crates/cad-core/src/tessellation/mesh.rs`. The output shape every `Operator::evaluate` produces:

```rust
pub struct Tessellation {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_labels: Option<Vec<TopologyFaceId>>,
}
```

Per HIGH #3 (2026-05-08) the unified collapse — a single mesh type carrying optional labels rather than parallel `LabeledMesh` / `Tessellation` types. See `CAD_TOPOLOGY_LINEAGE.md` §3 for the labeled-state design rationale.

Index-validity invariants are enforced at construction time:

- `indices.len() % 3 == 0` (triangle list).
- Every `idx in indices` satisfies `(idx as usize) < positions.len()`.
- If `face_labels = Some(labels)`, then `labels.len() == indices.len() / 3`.

`TessellationError::IncompleteTriangle / IndexOutOfBounds / LabelLengthMismatch` surface violations.

Accessors: `vertex_count() / triangle_count() / is_labeled() / face_labels() / face_count()`. `face_count()` returns `Some(n)` for labeled tessellations counting distinct non-`DEGENERATE` ids; `None` for unlabeled.

## 7. `Tolerance` — quantized cache key fragment

Lives at `crates/cad-core/src/tessellation/cache.rs`. Newtype around a positive finite `f32`:

```rust
pub struct Tolerance(pub f32);

impl Tolerance {
    pub fn new(t: f32) -> Result<Self, ToleranceError>;  // validates finite > 0
    pub fn value(self) -> f32;
}
```

`Hash` / `Eq` quantize the inner `f32` via `(value * 1e9) as u64` so tolerances that agree to ~1 nanometer (when interpreted in meters) hash and compare equal. This is the floating-point-stability guarantee that lets the cache key match across float-drift between two structurally-identical evaluations.

## 8. `TessellationCache` — memoization

Lives at `crates/cad-core/src/tessellation/cache.rs`:

```rust
pub struct CacheKey {
    pub structural_hash: [u8; 32],   // effective_hash from §4
    pub tolerance: Tolerance,
}

pub struct TessellationCache { /* HashMap<CacheKey, Arc<Tessellation>>, hit/miss tracking */ }

impl TessellationCache {
    pub fn new() -> Self;
    pub fn get(&self, key: &CacheKey) -> Option<Arc<Tessellation>>;
    pub fn insert(&mut self, key: CacheKey, tess: Tessellation) -> Arc<Tessellation>;
    pub fn record_hit(&mut self);   // bumped by OperatorGraph::evaluate
    pub fn record_miss(&mut self);
    pub fn hits(&self) -> u64;
    pub fn misses(&self) -> u64;
}
```

`HashMap` (not `BTreeMap`) intentionally — determinism is not a requirement here (the cache key fully encodes the inputs and the value is always recomputable on miss) and we want hashing speed. Hits and misses are tracked so the editor can surface cache-effectiveness metrics.

The cache key extension folds the upstream-labeled-bitmap into `structural_hash` per `CAD_TOPOLOGY_LINEAGE.md` §8 — defense in depth against operators that emit labels without reflecting label-emitting parameters in their local `structural_hash`.

## 9. Capability surface (per ADR-104)

ADR-104 declares a `KernelCapabilities` struct as the canonical capability surface, but materialises only when one of three trigger conditions fires (editor-ui operator-picker filtering / second CAD kernel landing per ADR-113-deferred / capability-aware tessellation cache). Until then the canonical form is the operator's module-level doc-comment block.

The canonical existing entry is `BooleanOp`'s capability block (per ADR-112 §"Capability surface entry"):

```text
boolean_robust_under_tolerance: false   (csgrs is BSP without exact arithmetic)
healing_strategies: none
deterministic_triangulation: true       (gated by 200-iter soak)
t_junction_handling: false              (csgrs upstream TODO)
```

`Cuboid / Extrude / Revolve / Transform` inherit the trivial defaults and document only their `output_labeled_when_input_labeled` value (the doc-comment form of `Operator::output_is_labeled`). When the trigger fires, the materialisation recipe is mechanical — see ADR-104 §"Materialisation recipe (when trigger fires)". Cross-link to ADR-104 for the canonical field set + materialisation triggers.

## 10. `GraphBuildError` — graph-construction errors

```rust
pub enum GraphBuildError {
    Graph(GraphError),                                                    // wraps DuplicateNode etc.
    RootNotFound(NodeId),                                                 // set_root on missing node
    PortMismatch { node: NodeId, expected_arity: usize, got: usize },     // construction-time arity check
}
```

`GraphError` is the substrate-level error from `kernel/graph-foundation`; `GraphBuildError` adds the cad-core-level concerns (root must exist; port-mismatch surface at construction not just at evaluation).

## 11. Failure class — snapshot-recoverable

Per PLAN §1.13 and the `//! Failure class: snapshot-recoverable` declaration on `crates/cad-core/src/lib.rs`. Every cad-core sub-module inherits the class.

Operator failures during `evaluate()` surface as `OpError`. Cache failures (csgrs panics on degenerate input etc.) are wrapped via `std::panic::catch_unwind` in `BooleanOp::evaluate` → `OpError::InvalidParameter`. Snapshot rollback is the canonical recovery path: a caller running `cad.begin_operation(); evaluate(...); commit(...)` who hits an `OpError` calls `cad.rollback()` instead, restoring the pre-operation graph state. The transactional bracket ensures editor-level undo is correct even when an operator panics partway through evaluation.

The `architecture-lints` workspace tool's `failure-class` lint enforces the declaration on every Tier-1 + Tier-2 crate; cad-core has the declaration so it does not appear in the failure-class exemptions table.

## 12. References

- **PLAN.md §1.5.4** — cad-core; §1.5.4.2 (persistent topology IDs); §1.5.4.3 (topology lineage); §1.5.4.4 (kernel non-equivalence + capability surface); §1.6.8 (determinism modes); §1.13 (failure classes); §13.2 / §13.6 (CAD validation gates).
- **ADR-098** — topology lineage substrate (sibling §18 doc `CAD_TOPOLOGY_LINEAGE.md`).
- **ADR-104** — capability surface; doc-comment-canonical until trigger fires; `output_labeled_when_input_labeled` field is the capability-surface form of `Operator::output_is_labeled`.
- **ADR-112** — Boolean CSG library; csgrs choice; capability-surface entry for `BooleanOp`.
- **ADR-113** (deferred) — truck cad-native backend; second-kernel trigger for ADR-104 materialisation.
- **`crates/cad-core/src/lib.rs`** — module roots + failure-class declaration.
- **`crates/cad-core/src/operators/mod.rs`** — `Operator` trait, `OpKind` / `EdgeKind` / `OperatorNode`, `OpError`.
- **`crates/cad-core/src/operators/{cuboid,transform,extrude,revolve,boolean}.rs`** — the five v0 operators.
- **`crates/cad-core/src/graph/operator_graph.rs`** — `OperatorGraph::evaluate` + `effective_hash_and_label`.
- **`crates/cad-core/src/checkpoints/mod.rs`** — `CadGraph` + `CheckpointHistory` + `Checkpoint` + the begin/commit/rollback/restore_to bracket.
- **`crates/cad-core/src/checkpoints/participate.rs`** — `SnapshotParticipate` impl (PLAN §13.2 closure).
- **`crates/cad-core/src/tessellation/mesh.rs`** — `Tessellation` + `TopologyFaceId` (also home of the unified `face_labels` substrate).
- **`crates/cad-core/src/tessellation/cache.rs`** — `Tolerance` + `CacheKey` + `TessellationCache`.
- **`CAD_TOPOLOGY_LINEAGE.md`** — sibling §18 doc; `infer_lineage` consumer of operator outputs.
- **`GRAPH_FOUNDATION.md`** — sibling §18 doc; the `Graph<N, E>` substrate `OperatorGraph` wraps and the `GraphSnapshot` substrate `CheckpointHistory` builds on.
- **`PLUGIN_API.md`** / **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 docs; convention origin.
