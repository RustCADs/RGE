# CAD_CORE_KERNEL_ADAPTERS

| Companion to | PLAN.md §1.5.4.4 (CAD kernel non-equivalence + capability surface) + ADR-104 (capability surface — doc-comment-canonical until trigger) + ADR-113-deferred (truck cad-native backend; second-kernel trigger for ADR-104 materialisation) |
|---|---|
| Status | Stable v0; the capability-surface doc-comment on `BooleanOp` is canonical (no `KernelCapabilities` struct exists yet); materialises into a real `struct` + `Operator::capabilities()` trait method when ANY of the three trigger conditions in ADR-104 §"Decision sub-decision 2" fires |
| Audience | Future kernel-adapter authors (csgrs hardening, truck integration, parry-as-spatial-accelerator); editor-ui operator-picker authors needing capability filtering; future capability-aware tessellation cache authors |
| Sibling doc | `CAD_CORE_MODEL.md` — operator catalog + `Operator` trait + `effective_hash_and_label`; `CAD_TOPOLOGY_LINEAGE.md` — face-lineage substrate where the csgrs Difference rhs-retag quirk surfaces (per §6 of this doc) |
| Reference impls | `crates/cad-core/src/operators/boolean/mod.rs` (canonical capability-surface doc-block; the `# csgrs features / capability surface` block in the module-doc is the doc-comment-canonical declaration; sub-module post-Phase-5-split — see §10 for the full file layout) · `crates/cad-core/src/operators/mod.rs` (the `Operator` trait that future `capabilities()` method extends) · ADR-104 §"Decision sub-decision 1" + §"Initial field set" |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. ADR-104 captures the *why* (decision space, materialisation triggers, alternatives explicitly rejected); ADR-112 captures the BooleanOp-specific values + csgrs choice; this doc captures the *substrate-level adapter pattern* — how kernel non-equivalence shows up in the operator surface today (csgrs-only) and how it will materialise tomorrow (multi-kernel via `KernelCapabilities`).

## 1. Non-equivalence doctrine

PLAN §1.5.4.4 commits the workspace: **CAD kernels are NOT interchangeable**. The substrate must NOT pretend they are. Concretely:

- **csgrs** (today's only Boolean backend, per ADR-112) is BSP-tree triangle-mesh CSG without exact arithmetic. Output is a polygon set with per-polygon plane equations and shared-vertex tracking — triangle-soup-equivalent for the workspace's `Tessellation` shape. T-junction handling is on the csgrs upstream TODO list. Robustness on near-degenerate / non-watertight meshes is best-effort.
- **truck** (deferred per ADR-113-deferred) is exact-arithmetic NURBS-based B-Rep. Output is a `Solid` with persistent face / edge / vertex IDs. Handles NURBS surfaces, sews / heals, robust under tolerance. Misaligned with today's `Tessellation` triangle-soup contract — adopting truck means rewriting upstream operators (`Cuboid` / `Extrude` / `Revolve`) to produce `Solid` instead of `Tessellation`, OR adding a Solid → Tessellation conversion shim.
- **parry3d** (transitive via rapier3d 0.32) is a collision-detection / spatial-query layer; it does NOT do mesh boolean ops. It is listed only because it sometimes shows up in Boolean-on-mesh conversations as a spatial accelerator (csgrs integrates with it optionally). Not a standalone CSG path.
- **Hand-rolled BSP** (~1500 LoC budget per ADR-112 Option 4) is the option that would let the workspace own the BSP code; rejected per ADR-112's Decision because the maintenance burden outweighs the dependency footprint of csgrs at v0.

The capability-surface substrate exists because these kernels' capability profiles differ in ways the operator-picker, the cache, and the editor-ui workflow must care about. PLAN §1.5.4.4 calls out "boolean robustness under tolerance" as the canonical kernel-distinguishing capability; ADR-104 generalises that into a typed surface.

## 2. `KernelCapabilities` — doc-comment-canonical surface

Per ADR-104 §"Decision sub-decision 1", today's canonical form is **doc-comments on operator modules**, NOT a real struct. The doc-comment-canonical form is acceptable until ANY of the three trigger conditions in §3 fires.

### Canonical fields (per ADR-104 §"Initial field set")

The struct, when materialised, takes the shape:

```rust
pub struct KernelCapabilities {
    pub boolean_robust_under_tolerance: bool,
    pub deterministic_triangulation: bool,
    pub t_junction_handling: bool,
    pub concave_input_supported: bool,
    pub arity: u32,
    pub output_labeled_when_input_labeled: bool,
}
```

Field semantics (from ADR-104 §"Initial field set"):

- **`boolean_robust_under_tolerance: bool`** — true iff the operator is robust under tolerance (e.g. exact-arithmetic CSG). `BooleanOp = false` (csgrs is BSP without exact arithmetic).
- **`deterministic_triangulation: bool`** — true iff the operator's triangulation is bit-deterministic given deterministic input ordering. All current operators = true (verified via the 200-iter Union+Difference soak per CI gate).
- **`t_junction_handling: bool`** — true iff the operator handles T-junctions in its output. `BooleanOp = false` (csgrs upstream TODO; documented in ADR-112).
- **`concave_input_supported: bool`** — true iff the operator accepts concave input polygons / surfaces. Extrude = false, Revolve (partial) = false, Revolve (full) = true, Cuboid = N/A.
- **`arity: u32`** — number of upstream tessellations the operator's `evaluate` expects. Already exposed via `Operator::arity()`; included in the struct for completeness.
- **`output_labeled_when_input_labeled: bool`** — true iff the operator preserves face labels when any input is labeled. Default (`inputs.iter().any(|b| *b)`) for most operators; `TransformOp = false` (strips labels). The capability-surface form of `Operator::output_is_labeled` per ADR-098 §"Operator trait extension" / `CAD_TOPOLOGY_LINEAGE.md` §7.

### Canonical existing entry — `BooleanOp`

The capability-surface doc-block in `crates/cad-core/src/operators/boolean/mod.rs` (the `# csgrs features / capability surface` section of the module-doc):

```text
# csgrs features / capability surface

csgrs 0.20.1 default-features = false + ["f64", "earcut"] — f64 avoids the
rapier3d 0.24/0.32 conflict (workspace pins 0.32 in crates/physics); the
bridge converts f32 ↔ f64 at the boundary. Per ADR-112/104 capability
surface: boolean_robust_under_tolerance: false, healing_strategies: none,
deterministic_triangulation: true (gated by the per-CI 200-iter Union+
Difference × 100 soak). T-junction handling deferred per csgrs upstream TODO.
```

Cuboid / Extrude / Revolve / Transform inherit the trivial defaults (no doc-block needed); operators with non-trivial capabilities MUST include the block per ADR-104 §"Implementation guidance / Doc-comment template".

## 3. Materialisation triggers

Per ADR-104 §"Decision sub-decision 2", the doc-comment-only form materialises into a real `struct` + `Operator::capabilities()` trait method when ANY of:

### Trigger 1 — Editor-UI operator-picker

When `editor-ui` (or any future caller) needs to programmatically filter the operator list by capability — for example: "show only operators with `boolean_robust_under_tolerance: true`", "show only operators with `deterministic_triangulation: true`", "filter by arity matching the current selection's port count" — the doc-comment-only form is no longer sufficient. Reading doc-comments at runtime is impossible (rustdoc strips them); the struct + trait method becomes load-bearing.

### Trigger 2 — Second CAD kernel (truck per ADR-113-deferred)

When truck lands as the `cad-native` backend, the workspace has TWO Boolean implementations with different capability profiles. Operator dispatch then needs to choose: does the user want exact-arithmetic robustness (truck)? or BSP triangle-soup speed (csgrs)? Without a struct, the operator-picker (and any cache-keying consumer) has to pattern-match on `OpKind` — `Boolean(BooleanOp { backend: Csgrs })` vs `Boolean(BooleanOp { backend: Truck })` — which is exactly what the capability surface was supposed to abstract per PLAN §1.5.4.4. The struct must materialise to keep the abstraction intact.

### Trigger 3 — Capability-aware tessellation cache

The cache today keys on `(structural_hash, Tolerance, labeled-state)` per `CAD_CORE_MODEL.md` §8. A future requirement to key on capability — for example "this cached entry was produced by a non-robust operator; invalidate when the user enables strict-tolerance mode" — fires materialisation. The cache key would extend to `(structural_hash, Tolerance, labeled-state, capability_fingerprint)` and the fingerprint derives from `KernelCapabilities`.

Until any of these fires, the doc-comment-only form is the canonical source per ADR-104. The materialisation recipe is mechanical (ADR-104 §"Materialisation recipe (when trigger fires)") so when the trigger does fire, the dispatch is bounded.

## 4. csgrs adapter (today's only backend)

The current cad-core → csgrs path is **direct** — there is NO `KernelAdapter` trait between them. `BooleanOp::evaluate` calls csgrs operations directly via the `csgrs_bridge` sub-module. From `crates/cad-core/src/operators/boolean/csgrs_bridge.rs`:

```rust
fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
    // ... arity check ...
    let lhs = inputs[0];
    let rhs = inputs[1];
    if lhs.is_labeled() || rhs.is_labeled() {
        self.evaluate_with_labels(lhs, rhs)
    } else {
        self.evaluate_unlabeled(lhs, rhs)
    }
}
```

Both paths convert to `csgrs::Mesh<S>` (where `S = ()` for unlabeled, `S = TopologyFaceId` for labeled), call `lhs_mesh.union(&rhs_mesh)` / `.intersection(...)` / `.difference(...)` per the `BooleanMode`, and convert back to `Tessellation`.

### Boundary conversions

- **f32 ↔ f64.** csgrs uses `nalgebra::Point3<f64>` internally; cad-core's `Tessellation::positions` is `Vec<[f32; 3]>`. The conversion happens at the polygon-construction boundary (`tessellation_to_csgrs`) and at the result-extraction boundary (`csgrs_to_tessellation`). The `f64` feature of csgrs is enabled because csgrs's `f32` feature conflicts with rapier3d 0.24 / 0.32 (workspace pins 0.32 in `crates/physics`).
- **Triangle soup ↔ csgrs polygons.** Each input triangle becomes a 3-vertex `csgrs::Polygon` with the right-hand-rule face normal. Each output polygon (csgrs may produce N-vertex coplanar polygons) gets fan-triangulated from `vertex[0]` so the output is back to triangle-soup. Vertex de-dup uses exact f32 bit equality (12-byte LE-byte key) for BLAKE3-stable determinism.
- **Degenerate filtering.** Triangles whose face-normal is zero (zero-area / coincident vertices) are filtered before reaching csgrs's BSP — csgrs panics on degenerate planes.

### catch_unwind shield

csgrs's BSP can panic on pathological input that survives the degenerate-filter (very-near-degenerate triangles, all-coincident vertices that pass the zero-norm check by an epsilon, etc.). The boolean dispatch wraps the operation in `std::panic::catch_unwind(AssertUnwindSafe(...))` and surfaces panics as `OpError::InvalidParameter("boolean failed: csgrs panicked on pathological input")`:

```rust
std::panic::catch_unwind(AssertUnwindSafe(|| match mode {
    BooleanMode::Union        => lhs_mesh.union(rhs_mesh),
    BooleanMode::Intersection => lhs_mesh.intersection(rhs_mesh),
    BooleanMode::Difference   => lhs_mesh.difference(rhs_mesh),
}))
.map_err(|_| OpError::InvalidParameter(...))
```

This routes panics → recoverable error per the snapshot-recoverable failure class (cad-core's `//! Failure class: snapshot-recoverable` declaration). The two regression tests `near_degenerate_input_handled_gracefully` and `boolean_returns_diagnostic_not_panic_on_pathological_input` pin the no-panic guarantee.

### csgrs Difference rhs-retag quirk

Documented in `BooleanOp`'s module-doc + `CAD_TOPOLOGY_LINEAGE.md` §4: csgrs preserves polygon metadata cleanly through Union and Intersection (clones it through plane splits / `clip_polygons`), but **Difference retags rhs's clipped polygons with `Mesh::metadata`** (which the bridge passes as `None`). So rhs-derived faces in Difference outputs arrive labeled `TopologyFaceId::DEGENERATE` (the unmetadata sentinel). The labeled regression test `boolean_evaluate_difference_retags_rhs_as_lhs_per_csgrs_quirk` pins this: lhs label survives; rhs label does NOT survive the Difference (it shows up as `DEGENERATE` or is absent).

This quirk is **kernel-specific** — truck would have a different identity story. The non-equivalence doctrine says: do not paper over this. `infer_lineage` accounts for the quirk; downstream consumers (lineage graph, persistent face IDs in Phase 7.2) treat the DEGENERATE label as a Reinterpreted edge rather than misclassifying it.

## 5. Future truck adapter (per ADR-113-deferred)

When truck lands, the adapter pattern materialises. The substrate-level shape:

```rust
pub trait KernelAdapter: Send + Sync {
    fn boolean(
        &self, mode: BooleanMode,
        lhs: &Tessellation, rhs: &Tessellation,
        tolerance: Tolerance,
    ) -> Result<Tessellation, OpError>;
    // ... future per-kernel methods (extrude, revolve, fillet, ...) ...
}

pub struct CsgrsAdapter;
impl KernelAdapter for CsgrsAdapter { /* today's evaluate code */ }

pub struct TruckAdapter;
impl KernelAdapter for TruckAdapter { /* truck-shapeops-backed impl */ }
```

> **Source-truth flag:** the dispatch spec described this trait. Source-truth: NO such trait exists today. csgrs is called directly from `BooleanOp::evaluate`. The trait materialises **only when the second-kernel trigger fires** per ADR-104. This doc reflects the design intent, NOT a current API.

Operator dispatch then chooses the adapter via capability query:

```rust
// pseudocode for the future operator-picker
let adapter: &dyn KernelAdapter = if user_request.requires_tolerance_robustness {
    &TRUCK_ADAPTER  // boolean_robust_under_tolerance: true
} else {
    &CSGRS_ADAPTER  // faster but BSP-without-exact-arithmetic
};
adapter.boolean(mode, lhs, rhs, tolerance)
```

The csgrs Difference rhs-retag quirk applies only to `CsgrsAdapter`. `TruckAdapter` would have its own quirk profile (sewing tolerances, tessellation density on free-form surfaces, NURBS-to-mesh conversion artifacts) — each documented in its capability-surface doc-block.

This dispatch is documenting the **design space**, NOT shipping the adapter. The adapter materialises when truck actually lands per ADR-113-deferred. Until then, `BooleanOp::evaluate`'s direct-csgrs-call is the canonical pattern.

## 6. `kernel_id` discriminant on `KernelCapabilities`

When the struct materialises (per §3 trigger 2 — second CAD kernel), it gains a `kernel_id: KernelId` discriminant field:

```rust
pub enum KernelId {
    Csgrs,
    Truck,
    // Future: Native, Hybrid, ...
}

pub struct KernelCapabilities {
    pub kernel_id: KernelId,
    pub boolean_robust_under_tolerance: bool,
    // ... other six canonical fields from §2 ...
}
```

> **Source-truth flag:** ADR-104 §"Initial field set" lists six canonical fields and does NOT include `kernel_id`. The `kernel_id` field is a forward-looking extension this doc names for the multi-kernel trigger; ADR-104 should be amended at materialisation time (or the discriminant lives on the adapter rather than the capability struct, depending on the chosen factoring). Flagging here so the materialisation dispatch picks one.

The editor-ui operator-picker uses `kernel_id` for filters like "show only operators backed by csgrs" or "show only operators backed by truck". The capability fields (`boolean_robust_under_tolerance` etc.) cover the cross-cutting "what does this operator promise" axis; `kernel_id` covers the orthogonal "which implementation provides it" axis. Both are needed for a usable picker.

## 7. Failure-class boundary

Kernel adapters route panics from underlying kernels through `catch_unwind` (csgrs example: §4 above). The pattern surfaces in the operator's failure class, not the adapter's:

- `cad-core` declares `//! Failure class: snapshot-recoverable` per PLAN §1.13. Every operator inherits this — including the csgrs-backed `BooleanOp`. A kernel panic surfaces as `OpError::InvalidParameter`; the snapshot-recoverable class means the caller can `cad.rollback()` instead of `commit()` and the editor-level undo is correct.
- Future kernels with stable error returns (truck's `truck-shapeops` operations return `Result` rather than panicking) won't need `catch_unwind`. The adapter wraps the underlying error type and routes to `OpError::InvalidParameter` directly.
- Cross-ref `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §7 ("Auto-emit policy") — the plugin-host's `catch_unwind` shield is the analogous pattern at the plugin boundary; the operator-level shield is the analogous pattern at the kernel boundary. Both surface to the unified `Diagnostic` stream.

The `architecture-lints` `failure-class` lint enforces the cad-core `//! Failure class: snapshot-recoverable` declaration; future per-adapter modules inherit by being inside `crates/cad-core/`.

## 8. Capability-aware caching (deferred until trigger 3)

The cache today keys on `(structural_hash, Tolerance, labeled-state)`. When trigger 3 fires (capability-aware cache), the key extends:

```rust
pub struct CacheKey {
    pub structural_hash: [u8; 32],
    pub tolerance: Tolerance,
    pub capability_fingerprint: [u8; 16],   // NEW; BLAKE3 over kernel-id + capability bools
}
```

The fingerprint folds the operator's `KernelCapabilities` into the cache key so that swapping a non-robust operator for a robust one (csgrs → truck) invalidates the cache. Without this, a cached `BooleanOp` result produced by csgrs could be served against a truck-routed query, masking the kernel difference the non-equivalence doctrine forbids.

Today the cache is correct because there is only one kernel (csgrs). Trigger 3 fires the moment a second kernel can route to the same operator; the dispatch dispatch updates the cache key and the operator-graph evaluator's `effective_hash_and_label` recipe in lockstep.

## 9. References

- **PLAN.md §1.5.4.4** — CAD kernel non-equivalence doctrine; "boolean robustness under tolerance" as the canonical kernel-distinguishing capability.
- **ADR-104** — capability surface; doc-comment-canonical until trigger; §"Decision sub-decision 1" (doc-comment-only form), §"Decision sub-decision 2" (three trigger conditions), §"Initial field set" (six canonical fields), §"Materialisation recipe (when trigger fires)" (mechanical post-trigger steps).
- **ADR-112** — cad-core Boolean CSG library; §"Capability surface entry" (the first concrete capability-surface declaration; csgrs-backed Boolean values); §"Decision" (csgrs over parry / truck / hand-rolled).
- **ADR-113-deferred** — truck cad-native backend; second-kernel trigger for ADR-104 materialisation. Currently in the architectural-debt registry's "Deferred (defensible until trigger fires)" list.
- **ADR-098** — topology lineage substrate; `output_is_labeled` invariant the `output_labeled_when_input_labeled` capability field reflects.
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; operator catalog (5 v0 operators); `Operator` trait the future `capabilities()` method extends; `effective_hash_and_label` recipe the capability-aware cache key extends.
- **`CAD_TOPOLOGY_LINEAGE.md`** — sibling §18 doc; csgrs Difference rhs-retag quirk surfaces (per §4 of this doc) as the labeled-path's `DEGENERATE`-then-Reinterpreted edge in `infer_lineage`.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; the `OpError → Diagnostic` boundary; `FailureClass::SnapshotRecoverable` for kernel-panic surfaces.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; the analogous `catch_unwind` shield at the plugin boundary; auto-emit policy template the per-adapter dispatch may want to mirror.
- **`crates/cad-core/src/operators/boolean/{mod,csgrs_bridge,labeled_path,unlabeled_path,tests}.rs`** (post-Phase-5-split sub-module): `mod.rs` carries the canonical capability-surface doc-block in the `# csgrs features / capability surface` section + `BooleanOp` + `impl Operator`; `csgrs_bridge.rs` holds `tessellation_to_csgrs` + `csgrs_to_tessellation` + `run_boolean` + `catch_unwind` shield; `labeled_path.rs` holds `evaluate_with_labels` + per-triangle `TopologyFaceId` synthesis; `unlabeled_path.rs` holds the simpler `evaluate_unlabeled` path; `tests.rs` carries the 22-test regression suite (arity / hashing / non-commutative Difference / near-degenerate / pathological / labeled-passthrough / Difference-rhs-retag-quirk).
- **`crates/cad-core/src/operators/mod.rs`** — `Operator` trait surface (the future `capabilities()` method extends); `OpKind` discriminant the post-trigger picker MUST move past per the non-equivalence doctrine.
- **`crates/cad-core/src/lib.rs`** — failure-class declaration the operator + future adapter modules inherit.
