# CAD_TOPOLOGY_LINEAGE

| Companion to | ADR-098 (topology lineage substrate); PLAN.md §1.5.4.3 |
|---|---|
| Status | v0 prototype (D-7.4 + D-7.4-followup metadata-passthrough + HIGH #3 unified-Tessellation collapse landed 2026-05-07 / 2026-05-08) |
| Audience | Operator authors who must reason about output-face identity; consumers of `LineageGraph`; anyone extending the lineage substrate toward Phase 7.2 PersistentFaceId or per-edge / per-vertex lineage |
| Sibling doc | `CAD_CORE_MODEL.md` — operator catalog + `Operator` trait + `effective_hash_and_label`; `GRAPH_FOUNDATION.md` — future `LineageGraph` backing |
| Reference impls | `crates/cad-core/src/topo_lineage/` (mod.rs / types.rs / plane.rs / infer/{mod,label_by_plane,infer_unlabeled,infer_labeled,tests}.rs — `infer` is a sub-module post-Phase-5-split) · `crates/cad-core/src/tessellation/mesh.rs` (`TopologyFaceId` + `Tessellation::face_labels`) · `crates/cad-core/src/operators/boolean/mod.rs` (csgrs metadata-passthrough producer; sub-module post-Phase-5-split) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. ADR-098 captures the *why* (design space, rejected alternatives, deferred fields per PLAN §1.5.4.3); this doc captures the *how* — using the substrate to query face lineage and to design new operators that participate in it.

**Elaborates**: REACTIVE_INVALIDATION.md §1 (Layer 2 — topology lineage emission inside Layer-1 transactions).

## 1. Quick concept

Every CAD operator's output faces have a lineage relation to its input faces — one of five evolutions:

- **`Preserved`** (1:1) — the input face's identity passes through unchanged.
- **`Split`** (1:N) — one input face becomes multiple disjoint output faces on the same plane.
- **`Merged`** (N:1) — multiple input faces collapse to one output face.
- **`Deleted`** (1:0) — the input face has no output coverage.
- **`Reinterpreted`** (0:1) — a newly-introduced output face with no matching input.

The `topo_lineage` substrate makes these queryable. The two key API entry points are `label_by_plane(tess, base_id) -> Tessellation` (assigns face ids to triangles by grouping on plane equation) and `infer_lineage(input, output, base_id) -> (Tessellation, LineageGraph)` (produces the lineage graph between a labeled input and an output, dispatching internally on whether the output already carries labels).

The substrate is *content-derived*: an operator does not need bespoke lineage machinery. After evaluating, the caller calls `infer_lineage` and gets back a labeled output plus a `LineageGraph`. Where csgrs metadata-passthrough is available (Boolean ops), `infer_lineage` takes the high-confidence label-tracking path; otherwise it falls back to the universal plane-equation heuristic. See ADR-098 §"Decision" for the rationale and §"Alternatives explicitly NOT chosen" for why we don't bind to one or the other path exclusively.

## 2. Public types

All in `crates/cad-core/src/topo_lineage/types.rs` except `TopologyFaceId` (which lives in `tessellation::mesh` and is re-exported from `topo_lineage::types` for back-compat — see ADR-098 §"Layering rationale").

### `TopologyFaceId(u64)`

Per-mesh face identity. Sequential within a single labeled `Tessellation`; not stable across rebuilds (Phase 7.2 PersistentFaceId is the future stabilisation dispatch — see §10).

```rust
pub struct TopologyFaceId(pub u64);
impl TopologyFaceId {
    pub const DEGENERATE: TopologyFaceId = TopologyFaceId(u64::MAX);
    pub fn is_degenerate(self) -> bool;
}
```

The `DEGENERATE = u64::MAX` sentinel labels triangles that could not be assigned a plane because they were degenerate (zero area) or had non-finite normals. Real-world CSG output (csgrs's BSP-tree triangulation in particular) routinely contains slivers and zero-area artifacts; the sentinel keeps labeling bounded and deterministic on adversarial input. Triangles labeled `DEGENERATE` are excluded from face counts and lineage edge inference.

### `TopologyEvolution`

```rust
pub enum TopologyEvolution {
    Preserved,
    Split,
    Merged,
    Deleted,
    Reinterpreted,
}
```

Discriminant-only at v0. PLAN §1.5.4.3 names `Split(Vec<PersistentFaceId>) / Merged(Vec<PersistentFaceId>)` inner data; the v0 represents these via multiple `LineageEdge` entries with a shared `from` (Split) or shared `to` (Merged). Deferred per ADR-098 §"v0 simplifications".

### `LineageEdge`

```rust
pub struct LineageEdge {
    pub from: Option<TopologyFaceId>,   // None for Reinterpreted
    pub to: Option<TopologyFaceId>,     // None for Deleted
    pub evolution: TopologyEvolution,
    pub confidence: f32,                // 1.0 = exact match, 0.5 = ambiguous, 0.0 = inferred
}
```

`confidence` is binary-ish at v0 (`1.0` for exact-plane / label-match, `0.5` for the ambiguous `Merged` case that the plane heuristic surfaces, `0.0` for purely-inferred edges). The PLAN §1.5.4.3 `SemanticScore` is deferred until a downstream consumer demands richer confidence — see §10.

### `LineageGraph`

```rust
pub struct LineageGraph { pub edges: Vec<LineageEdge> }

impl LineageGraph {
    pub fn new() -> Self;
    pub fn push(&mut self, edge: LineageEdge);
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn edges_from(&self, face_id: TopologyFaceId) -> impl Iterator<Item = &LineageEdge>;
    pub fn edges_to(&self, face_id: TopologyFaceId) -> impl Iterator<Item = &LineageEdge>;
    pub fn edges_by_evolution(&self, ev: TopologyEvolution) -> impl Iterator<Item = &LineageEdge>;
}
```

`Vec`-backed at v0. The PLAN-spec'd `kernel/graph-foundation::Graph` backing materialises when traversal queries (ancestor walks, descendant walks, multi-step lineage paths) become a real downstream consumer — currently no such consumer exists, so the simpler `Vec` ships. See `GRAPH_FOUNDATION.md` for the substrate it would migrate onto.

### `LineageError`

```rust
pub enum LineageError {
    LabelLengthMismatch { got: usize, expected: usize },
    InvalidInput(String),
    NonFiniteNormal { triangle_idx: usize },
    DegenerateTriangle { triangle_idx: usize },
}
```

`InvalidInput` is the variant `infer_lineage` returns when called with unlabeled input (the caller didn't run `label_by_plane` first). `DegenerateTriangle` / `NonFiniteNormal` are reachable through the private `QuantizedPlane::from_triangle` and exist as a future strict-mode hook — the public `label_by_plane` and `infer_lineage` paths *skip* degenerate triangles into the `DEGENERATE` sentinel rather than erroring.

## 3. The unified `Tessellation` type

Per HIGH #3 (2026-05-08 unified-mesh refactor — ADR-098 §"Decision" sub-decision 3), the substrate type is the existing `Tessellation` extended with a single optional field rather than a parallel `LabeledMesh` type:

```rust
pub struct Tessellation {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_labels: Option<Vec<TopologyFaceId>>,
}

impl Tessellation {
    pub fn new(positions, indices) -> Result<Self, TessellationError>;        // unlabeled
    pub fn with_labels(positions, indices, labels) -> Result<Self, _>;        // labeled
    pub fn is_labeled(&self) -> bool;                                          // face_labels.is_some()
    pub fn face_labels(&self) -> Option<&[TopologyFaceId]>;
    pub fn face_count(&self) -> Option<usize>;   // distinct labels excluding DEGENERATE
}
```

`face_labels.len() == indices.len() / 3` (one label per triangle) is enforced by `with_labels`. `is_labeled()` is the single dispatch axis: operators inspect their inputs' labeled-state and propagate it; lineage inference dispatches on the output's labeled-state (see §5).

The serde `skip_serializing_if` keeps the snapshot wire format minimal: an unlabeled `Tessellation` serializes byte-identically to pre-HIGH-#3 form. Labeled meshes carry the `face_labels: [...]` entry.

Trade-off recap from ADR-098 §"Alternatives explicitly NOT chosen": a parallel `LabeledMesh` type forced every operator to carry two `evaluate` signatures and every lineage path to carry two `infer_lineage` signatures. The combinatorial cost surfaced in HIGH #3; collapsing to `face_labels: Option<…>` cut every duplicated signature without changing runtime behaviour.

## 4. The hybrid lineage path

ADR-098 §"Decision" mandates a **hybrid** substrate: csgrs-metadata-passthrough where available, plane-equation matching as the universal fallback. The split below is what `infer_lineage` dispatches on:

### csgrs-metadata-passthrough (high-confidence path)

csgrs's `Mesh<S>` carries per-polygon metadata. The metadata round-trips through Union and Intersection cleanly: csgrs clones it through plane splits and through `clip_polygons`. For Difference, csgrs has a known quirk — it retags rhs-clipped polygons with lhs's metadata. `infer_lineage` accounts for this: rhs-derived faces in Difference outputs arrive labeled `TopologyFaceId::DEGENERATE` (the unmetadata sentinel from the boolean bridge), and `infer_lineage_with_labeled_output` surfaces them collectively as a single `Reinterpreted` edge with `to = Some(DEGENERATE)`.

When metadata is available, lineage classifications inherit the kernel's view of identity — no plane-equation reconstruction needed. Confidence is `1.0` across the board.

### Plane-equation matching (universal fallback)

Hand-rolled / non-csgrs operators have no metadata path. The fallback uses a private `QuantizedPlane` type that quantizes triangle normals + offsets to ~1e-4 precision and sign-canonicalizes opposite-facing normals so opposite-winding duplicates of the same plane hash equal:

```rust
struct QuantizedPlane { nx: i32, ny: i32, nz: i32, offset: i32 }

// Quantize: (component * 10_000).round() as i32 (saturating cast).
// Sign-canonicalize: ensure the first non-zero quantized component is positive
// so opposite-winding duplicates of the same plane hash identically.
```

Plane equation matching is a **heuristic**. Two distinct logical faces that happen to share a plane equation (e.g. coplanar caps after a Boolean) are indistinguishable to the v0 detector. Documented as a v0 limitation; resolution waits on connected-component analysis (deferred) or PersistentFaceId (Phase 7.2). See ADR-098 §"Negative / risks" and §"Followups".

Why hybrid rather than pure-csgrs or pure-plane: pure-csgrs would couple lineage to one kernel (truck — deferred per ADR-113 — would have a different identity story; any future hand-rolled operator wouldn't have metadata). Pure-plane shipped a real false-positive in D-7.4: surviving Difference faces classified as `Merged` because csgrs's BSP output has more triangles than the input plane after the difference cut. Hybrid is strictly better than pure-plane and strictly more portable than pure-csgrs.

## 5. `label_by_plane(tess, base_id) -> Tessellation`

```rust
pub fn label_by_plane(tess: &Tessellation, base_id: u64) -> Result<Tessellation, LineageError>;
```

Groups triangles by plane equation, returns a labeled `Tessellation` (always `is_labeled() == true`). Each distinct quantized plane gets a sequential face id starting at `base_id`. Triangles whose planes hash equally share the same face id. Degenerate / non-finite triangles are tagged `TopologyFaceId::DEGENERATE` and excluded from face counts; they are not an error condition.

Use this when an operator can't carry lineage metadata (hand-rolled / non-csgrs) and the caller needs labels for downstream lineage queries. Internally, `infer_lineage` calls it on unlabeled outputs.

The face-id assignment order is **input-traversal order** — the first triangle to touch a given plane gets the lowest id. Deterministic because the outer loop walks `tess.indices` in order and the only data structure used is a `HashMap<QuantizedPlane, TopologyFaceId>` whose iteration order does not affect output (the labels vector is populated in lock-step with the loop, not from the map).

`base_id` MUST satisfy `base_id + triangle_count < u64::MAX` so plane face ids cannot collide with the `DEGENERATE` sentinel. In practice `base_id` of `0`, `100`, or `1_000_000` is safely below the sentinel; the assertion in the body documents the invariant.

If the input tessellation already carries labels (`tess.is_labeled() == true`), they are *discarded and replaced* by plane-derived labels — the caller asked to relabel by plane, so we do.

## 6. `infer_lineage(input, output, base_id) -> (Tessellation, LineageGraph)`

```rust
pub fn infer_lineage(
    input: &Tessellation,
    output: &Tessellation,
    output_base_id: u64,
) -> Result<(Tessellation, LineageGraph), LineageError>;
```

Reconstructs lineage between a labeled input and an output. **Input must be labeled** — pass it through `label_by_plane` first if not. If `input.is_labeled() == false` returns `LineageError::InvalidInput`.

Dispatches on `output.is_labeled()`:

```text
match output.is_labeled() {
    true => infer_lineage_with_labeled_output(input, input_labels, output),
    false => {
        // Plane-equation heuristic: relabel the output, then match by plane.
        let labeled_output = label_by_plane(output, output_base_id)?;
        // Walk inputs in BTreeMap (deterministic) order:
        //   plane match + same triangle count -> Preserved (1.0)
        //   plane match + fewer outputs       -> Split     (1.0)
        //   plane match + more outputs        -> Merged    (0.5)
        //   no plane match                    -> Deleted   (1.0)
        // Output planes with no input match   -> Reinterpreted (1.0)
        // Returns (labeled_output, LineageGraph).
    }
}
```

Both paths return `(labeled_output, lineage_graph)` where the labeled output is the input's `output` upgraded to labeled form (cloned through when already labeled, relabeled by plane when not). The labeled-output return value is what downstream consumers thread into the next operator's input.

**The labeled path's central correctness property**: same-label-different-count is always `Split`, never `Merged`. Merged in the v0 lineage taxonomy means *multiple distinct input labels collapse to one output label* — that requires distinct input labels mapping to a single output label, which the per-input-label scan cannot observe directly. The labeled path therefore never emits `Merged`. This is the bug-fix that closed the D-7.4 plane-only false positive (surviving Difference faces classified as `Merged` because BSP retriangulation had more output triangles).

Test coverage: `infer/tests.rs` (post-Phase-5-split test sub-module) ships 14 unit tests covering identity-preservation, labeled-vs-unlabeled output dispatch, the labeled-path Split-not-Merged correctness gate, the Difference-degenerate `Reinterpreted` collapse, and lhs/rhs label distinguishability (per ADR-098 §"Test recipes").

## 7. Operator-trait label propagation

Phase 2 substrate per ADR-098 §"Operator trait extension". The `Operator` trait carries:

```rust
fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
    inputs_labeled.iter().any(|b| *b)   // default: any labeled input -> labeled output
}
```

Default propagates the labeled bit (any labeled input ⇒ labeled output). Operators that strip labels override to `false` (the canonical example is `TransformOp` per `cad-core/src/operators/transform.rs`). Operators that emit labels regardless of input would override to return `true`. See `CAD_CORE_MODEL.md` §3 for the full trait surface.

The contract: `output_is_labeled(...)` MUST match the actual `evaluate(...)` output's `Tessellation::is_labeled()` for the same inputs. If the prediction diverges from reality, the cache key (next section) becomes inconsistent and stale entries may surface.

## 8. `TessellationCache` labeled-state defense

Per ADR-098 §"Cache key extension". `OperatorGraph::evaluate` recursively computes an `effective_hash` per node by folding `(local_structural_hash, port_index, upstream_effective_hash)` for each upstream input — but post-HIGH-#3, an operator's output shape *also* depends on whether its inputs are labeled. Same operator, same parameters, same upstream `structural_hash`, but different output shape (labeled vs. unlabeled). Without a key extension, the cache would return a labeled tessellation when an unlabeled call expected the unlabeled fast path, or vice versa.

The fix: `effective_hash_and_label` folds an upstream-labeled-bitmap (1 bit per input port, packed into a `u32` modulo 32 for arity-32+ operators) into the BLAKE3 hash:

```rust
let upstream_labeled_bitmap: u32 = upstream_data
    .iter()
    .enumerate()
    .fold(0u32, |acc, (i, (_, labeled))| {
        if *labeled { acc | (1u32 << (i % 32)) } else { acc }
    });
hasher.update(&upstream_labeled_bitmap.to_le_bytes());
```

Two cache entries with the same operator + parameters but different upstream-labeled state hash distinctly. Hot-path cost: one extra BLAKE3 update per cache-key computation (4 bytes for any reasonable arity).

This is **defense in depth**. An operator implementer that forgets to fold a label-emitting parameter into `structural_hash` would otherwise produce cache-key collisions between labeled-input and unlabeled-input calls. The bitmap fold catches the case mechanically. Audit-2 finding A1.4 / A5.2 / Pairing N2 ("latent-but-explosive cache-collision bug") flagged the gap and this closure landed alongside it. See `CAD_CORE_MODEL.md` §5 for the broader cache-key recipe.

## 9. v0 simplifications vs PLAN §1.5.4.3 spec

ADR-098 §"v0 simplifications" enumerates these; re-cited here for completeness so consumers know what is and isn't on the v0 surface:

- **No `OperatorId` field on `LineageEdge`.** Depends on a stable operator-instance identity beyond `NodeId` (the operator-graph version-tracking dispatch). Defer.
- **No `SemanticScore` field on `LineageEdge`.** Depends on a richer semantic confidence model. Defer.
- **No `Split(Vec<PersistentFaceId>) / Merged(Vec<PersistentFaceId>)` inner data.** v0 represents these via multiple `LineageEdge` entries with a shared `from` (Split) or `to` (Merged); the discriminant-only enum keeps the API surface small.
- **No `PersistentFaceId`.** v0 uses sequential `TopologyFaceId` per-mesh; not stable across rebuilds. Phase 7.2 dispatch.
- **Face-only.** No edge / vertex lineage; B-Rep / mesh-topology bookkeeping budget didn't fit the prototype.
- **`Vec` backing on `LineageGraph::edges`.** Not yet `kernel/graph-foundation::Graph` (see `GRAPH_FOUNDATION.md`).

## 10. Future migrations

ADR-098 §"Followups / open questions" tracks these:

- **Phase 7.2 PersistentFaceId** — content-hash + lineage-path identifier so the editor can show "this face came from rev 12's Cuboid bottom-face → rev 17's Boolean cut" across saves. Tracked in HANDOFF.md.
- **`kernel/graph-foundation::Graph` backing** — materialises when traversal queries (ancestor walks etc.) become a real downstream consumer.
- **`OperatorId` field on `LineageEdge`** — depends on operator-graph version-tracking.
- **csgrs Difference rhs-retag special-casing** — documented in `infer_lineage_with_labeled_output` comments; if upstream csgrs changes Difference's metadata semantics, the special-case has to update.
- **Connected-component analysis for true `Split` detection** — v0's triangle-count comparison is a proxy; a real `Split` detector groups triangles by connected-edge adjacency on the matching plane and counts components.
- **`SemanticScore` field** — richer than the binary-ish v0 `confidence`. Defer until a UI confidence visualization demands it.

## 11. References

- **ADR-098** — topology lineage substrate; §"Decision" (hybrid path), §"Implementation guidance" (public API), §"v0 simplifications", §"Followups / open questions".
- **PLAN.md §1.5.4.3** — topology lineage; §1.5.4.2 (persistent topology IDs); §1.5.4.4 (kernel non-equivalence).
- **ADR-104** — capability surface; `output_labeled_when_input_labeled` field is the capability-surface form of the `Operator::output_is_labeled` invariant.
- **ADR-112** — cad-core Boolean CSG library; §"Phase 7.2 / 7.4 hook" and §"csgrs metadata-passthrough verification" describe the metadata-passthrough resolution this doc consumes.
- **`crates/cad-core/src/topo_lineage/mod.rs`** — module-level v0-simplifications doc (the canonical inline narrative).
- **`crates/cad-core/src/topo_lineage/types.rs`** — `LineageError`, `TopologyEvolution`, `LineageEdge`, `LineageGraph`.
- **`crates/cad-core/src/topo_lineage/plane.rs`** — `QuantizedPlane` (private; quantization + sign-canonicalization).
- **`crates/cad-core/src/topo_lineage/infer/{mod,label_by_plane,infer_unlabeled,infer_labeled,tests}.rs`** (sub-module post-Phase-5-split) — `mod.rs` carries the `infer_lineage` dispatcher + `pub use label_by_plane`; `label_by_plane.rs` holds the plane-grouping triangulation; `infer_unlabeled.rs` holds the plane-equation-matching heuristic path; `infer_labeled.rs` holds the csgrs-metadata-passthrough fast path; `tests.rs` carries the 14-test regression suite.
- **`crates/cad-core/src/tessellation/mesh.rs`** — `TopologyFaceId`, `Tessellation::face_labels`, the unified mesh substrate.
- **`crates/cad-core/src/operators/mod.rs`** — `Operator::output_is_labeled` trait method.
- **`crates/cad-core/src/operators/boolean/mod.rs`** — `BooleanOp::evaluate` (the canonical csgrs metadata-passthrough producer; sub-module post-Phase-5-split — see CAD_CORE_KERNEL_ADAPTERS.md for the full file layout).
- **`crates/cad-core/src/graph/operator_graph.rs`** — `effective_hash_and_label` cache-key extension.
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; `Operator` trait, operator catalog, `OperatorGraph::evaluate` recipe.
- **`GRAPH_FOUNDATION.md`** — sibling §18 doc; substrate the `LineageGraph` would migrate onto.
