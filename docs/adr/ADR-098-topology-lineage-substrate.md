# ADR-098: Topology lineage substrate

| Status | Accepted 2026-05-08 (D-7.4 + D-7.4-followup metadata-passthrough + HIGH #3 unified-Tessellation refactor all landed) |
|---|---|
| Date | 2026-05-08 |
| Deciders | (RGE architecture review) |
| PLAN references | §1.5.4.3 (topology lineage), §1.5.4.2 (persistent topology IDs), §1.5.4.4 (CAD kernel non-equivalence + capability surface), §1.6.8 (determinism modes), §1.13 (failure classes), §13.6 (CAD validation gates) |
| ADR references | ADR-097 (cad-projection split), ADR-104 (capability surface), ADR-112 (cad-core Boolean CSG library) |
| Implementation phase | Phase 7.4 — Topology lineage prototype (under the Phase 7 "many architectures die here" risk umbrella) |

## Context

PLAN §1.5.4.3 commits the workspace to a topology-lineage substrate: every CAD operator must record how each output face relates to the input faces it descended from — `Preserved`, `Split`, `Merged`, `Deleted`, or `Reinterpreted` (newly-introduced). Without lineage the persistent-ID story collapses to "remap and hope"; with lineage, identity becomes traceable across rebuilds, replication, and history-walk UIs. Phase 7.4 is the first dispatch where this substrate has to ship as code rather than as a paragraph in PLAN.md.

The dispatch landed in three waves. **D-7.4** (2026-05-07) shipped the v0 prototype: `crates/cad-core/src/topo_lineage/` with plane-equation-matching as the only lineage signal. **D-7.4-followup** (2026-05-07) integrated csgrs's per-polygon `Mesh<S>` metadata as a stronger signal for csgrs-backed Boolean operators, fixing the v0 false-positive where surviving Difference faces were classified as Merged. **HIGH #3** (2026-05-08) collapsed the `LabeledMesh` / `Tessellation` substrate duplication that the followup had introduced: a single `Tessellation` carries optional `face_labels: Option<Vec<TopologyFaceId>>`, and the `BooleanOp::evaluate` / `infer_lineage` paths dispatch on the labeled-state of their inputs rather than on a separate type.

The substrate is a *prototype*. PLAN §1.5.4.3 names a handful of fields and structures (per-edge / per-vertex lineage, `OperatorId` on `LineageEdge`, `SemanticScore`, `PersistentFaceId` content-hashing, `kernel/graph-foundation::Graph` backing) that v0 deliberately defers — Phase 7.2 (persistent IDs) and the `kernel/graph-foundation::Graph` choice are later dispatches that need v0's API surface to harden first. This ADR documents the v0 design space and the trail of follow-ups so future maintainers don't have to reverse-engineer it from code.

## Decision

**The lineage substrate is a hybrid: csgrs-metadata-passthrough where available, plane-equation-matching as the universal fallback. The substrate type is the existing `Tessellation`, extended with a single optional `face_labels` field rather than introducing a parallel `LabeledMesh` type.**

Three sub-decisions follow from this.

1. **Plane-equation matching is the universal substrate.** The private `QuantizedPlane` type quantizes triangle normals + offsets to ~1e-4 precision (via `(component * 10_000).round() as i32`) and sign-canonicalizes opposite-facing normals so opposite-winding duplicates of the same plane hash equal. `label_by_plane(tess, base_id)` assigns one `TopologyFaceId` per distinct quantized plane. `infer_lineage(input, output, base_id)` matches output planes against input planes; surviving planes with same triangle counts → `Preserved`, fewer-output → `Split`, more-output → `Merged`, no-match → `Deleted`, output-plane-with-no-input-match → `Reinterpreted`. This path works for ANY operator (csgrs-backed or hand-rolled) because every operator produces triangle output.

2. **csgrs-metadata-passthrough is layered on top where it exists.** csgrs's `Mesh<S>` carries per-polygon metadata that round-trips through Union and Intersection cleanly (clones through plane splits + `clip_polygons`). Difference is special-cased: csgrs retags rhs-clipped polygons with lhs's metadata, so `infer_lineage_labeled` knows to treat rhs lineage in Difference outputs as the canonical case rather than a metadata bug. When metadata is available, lineage classifications inherit the kernel's view of identity (no plane-equation reconstruction needed). When metadata is *not* available — non-csgrs operators or unlabeled inputs — the plane-equation fallback handles it.

3. **`face_labels` is an optional field on `Tessellation`, not a parallel type.** Per HIGH #3 (2026-05-08): `face_labels: Option<Vec<TopologyFaceId>>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. Unlabeled meshes serialize bit-identically to pre-HIGH-#3. Operators dispatch on `output.is_labeled()` rather than on type identity, collapsing the `BooleanOp::evaluate` / `evaluate_labeled` and `infer_lineage` / `infer_lineage_labeled` duplication to a single signature each.

## Consequences

### Positive

- **One substrate, one path through the operator graph.** Operators don't fork on labeled-vs-unlabeled at compile time; the dispatch is at runtime, on a single `Option<Vec<…>>`. Cache integration sees one `Tessellation` shape.
- **Plane-equation fallback is universal.** Hand-rolled operators (Cuboid / Extrude / Revolve) and any future kernel-agnostic operators get lineage "for free" without hand-rolling identity tracking.
- **csgrs-metadata-passthrough is a strict refinement.** Where csgrs provides identity, we use it; where it doesn't, the plane fallback covers. No correctness regression vs. plane-only.
- **Sentinel-based degenerate handling stays robust.** Real-world CSG output (csgrs's BSP-tree triangulation in particular) routinely contains slivers and zero-area artifacts. `TopologyFaceId::DEGENERATE = u64::MAX` collects them under one face id rather than each producing a fresh one — labeling stays bounded and deterministic on adversarial input.

### Negative / risks

- **Plane-equation matching is heuristic.** Two distinct logical faces that happen to share a plane equation (e.g. coplanar caps after a Boolean) are indistinguishable to the v0 detector. Documented in `topo_lineage/mod.rs` as a v0 limitation; resolution waits on connected-component analysis (deferred) or PersistentFaceId (Phase 7.2).
- **Difference rhs-retag is a csgrs quirk we depend on.** csgrs preserves lhs's metadata on the surviving polygons of `lhs - rhs`; if upstream csgrs ever changes that, our `infer_lineage_labeled` Difference path breaks. Watched in the dependency-audit cadence and fixed in `infer_lineage_labeled` documentation comments.
- **Confidence is binary in v0.** `LineageEdge.confidence` is `1.0` for exact-plane matches, `0.5` for ambiguous matches, `0.0` for purely inferred. The `SemanticScore` from PLAN §1.5.4.3 (richer semantic confidence) is deferred — v0 is enough to test the API shape against real Boolean output, not to feed a downstream UI.

### Mitigations

- **Determinism gate.** `cad-core/tests/cad_lineage_*.rs` runs lineage on the §13.6 fixture set and asserts BLAKE3 byte-identity across iterations. Plane quantization is deterministic by construction; csgrs-metadata-passthrough is deterministic given deterministic input ordering (already gated by `OperatorGraph::structural_hash`).
- **Degenerate-handling test coverage.** Unit tests in `plane.rs` cover: collinear-points triangle (zero area), NaN-input triangles, coincident-vertex triangles, opposite-winding identity. Sentinel `TopologyFaceId::DEGENERATE` is tested via `label_by_plane` integration; strict-mode `LineageError::DegenerateTriangle` / `NonFiniteNormal` reachable through the private `QuantizedPlane::from_triangle` for future strict-mode hooks.
- **Layering invariant.** `TopologyFaceId` lives in `tessellation::mesh` (so `Tessellation` can own `face_labels` without a `tessellation → topo_lineage` reverse import). It is re-exported from `topo_lineage::types` for back-compat with code that imports it from the older path. Architecture-lints enforce no reverse imports from kernel layers.

## Alternatives explicitly NOT chosen and why

**Pure csgrs metadata, no plane fallback.** csgrs is one of N future operator backends; truck (deferred per ADR-113 placeholder) ships its own face-identity story, and any hand-rolled operator wouldn't have csgrs's metadata at all. Committing to "kernel-provided identity is the only signal" makes lineage *kernel-coupled*, which violates the non-equivalence doctrine of §1.5.4.4: kernels are not interchangeable, but a substrate that inherits *all* its identity from one kernel can't be ported across them. Plane-equation matching is the kernel-neutral substrate.

**Pure plane-equation matching, no csgrs metadata.** This was the v0 (D-7.4) shape and it shipped a real false-positive: the surviving lhs face in `cube – sphere` was classified `Merged` (because csgrs's BSP output has more triangles than the input plane after the difference cut). Fixing the false-positive without metadata requires connected-component analysis on the output triangles AND the input plane's triangles, which is structurally harder than just using metadata where it exists. Hybrid is strictly better than plane-only.

**Parallel `LabeledMesh` type.** This was the post-D-7.4-followup shape (2026-05-07) and it forced every operator to carry two `evaluate` signatures (`evaluate` for unlabeled, `evaluate_labeled` for labeled) and every lineage path to carry two `infer_lineage` signatures. The combinatorial-proliferation cost surfaced in HIGH #3 of the 2026-05-08 audit; collapsing to `face_labels: Option<…>` on `Tessellation` cut every duplicated signature and cache key without changing any runtime behaviour.

**Per-edge / per-vertex lineage from day one.** PLAN §1.5.4.3 names edge and vertex lineage. v0 ships face-only because edge / vertex identity require either B-Rep input (truck) or much more sophisticated mesh-topology bookkeeping than the prototype budget admitted. Deferred to a follow-up ADR after the face-only path's real-world coverage is understood.

## Implementation guidance

### Public API (post-2026-05-08)

```rust
// crates/cad-core/src/tessellation/mesh.rs
pub struct TopologyFaceId(pub u64);
impl TopologyFaceId {
    pub const DEGENERATE: TopologyFaceId = TopologyFaceId(u64::MAX);
}

pub struct Tessellation {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_labels: Option<Vec<TopologyFaceId>>, // one label per triangle
}
impl Tessellation {
    pub fn is_labeled(&self) -> bool { self.face_labels.is_some() }
    // ...
}

// crates/cad-core/src/topo_lineage/types.rs
pub enum TopologyEvolution { Preserved, Split, Merged, Deleted, Reinterpreted }
pub struct LineageEdge {
    pub from: Option<TopologyFaceId>, // None for Reinterpreted
    pub to: Option<TopologyFaceId>,   // None for Deleted
    pub evolution: TopologyEvolution,
    pub confidence: f32,
}
pub struct LineageGraph { pub edges: Vec<LineageEdge> }
pub enum LineageError { LabelLengthMismatch{...}, InvalidInput(String), NonFiniteNormal{...}, DegenerateTriangle{...} }

// crates/cad-core/src/topo_lineage/infer.rs
pub fn label_by_plane(tess: &Tessellation, base_id: u64) -> Result<Tessellation, LineageError>;
// Returns the input mesh with `face_labels` populated (one TopologyFaceId per
// triangle, grouped by quantized plane equation).
pub fn infer_lineage(input: &Tessellation, output: &Tessellation, output_base_id: u64) -> Result<(Tessellation, LineageGraph), LineageError>;
// `input` MUST be labeled (carries `face_labels`); `output_base_id` is the
// monotonic-base used to assign new face ids on the output side. Returns
// `(labeled_output, lineage)` where `labeled_output` is the same triangle
// mesh as `output` but with `face_labels` populated and `lineage` records
// per-face evolution edges.
```

### Operator trait extension (Phase 2)

```rust
// crates/cad-core/src/operators/mod.rs (Operator trait)
fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
    inputs_labeled.iter().any(|b| *b)
}
```

The default propagates the labeled bit (any labeled input ⇒ labeled output); operators that strip labels (e.g. Transform) override to `false`. The cache key extension folds an upstream-labeled-bitmap (1 bit per port modulo 32) into BLAKE3 via `effective_hash_and_label`, so a `Tessellation` from `[unlabeled, labeled]` inputs hashes distinctly from `[labeled, unlabeled]`. Defends the cache against label-state collision when otherwise-identical operators differ only in upstream labeling.

### Layering rationale: why `TopologyFaceId` lives in `tessellation::mesh`

The natural place for `TopologyFaceId` is `topo_lineage::types` — it's a lineage-substrate type, after all. But `Tessellation` (in `tessellation::mesh`) carries `face_labels: Option<Vec<TopologyFaceId>>`. If `TopologyFaceId` lived in `topo_lineage::types`, then `tessellation::mesh` would `use crate::topo_lineage::types::TopologyFaceId` — a `tessellation → topo_lineage` reverse import that violates the layered-import architecture-lint (the lineage substrate is logically *above* the tessellation substrate; tessellation knows nothing about lineage other than the optional `face_labels` slot).

The fix is to put `TopologyFaceId` at the lower layer (`tessellation::mesh`) and re-export from the upper layer (`topo_lineage::types`) for back-compat. Code that imports `TopologyFaceId` from either path resolves to the same type. The architecture-lint passes because all imports flow upward (tessellation does not depend on topo_lineage).

### Cache key extension: defending against label-state collision

Pre-HIGH-#3, the tessellation cache keyed on `structural_hash + Tolerance`. After collapsing `LabeledMesh` into `Tessellation::face_labels`, an operator's output shape now depends on whether its inputs are labeled — same operator, same parameters, same upstream `structural_hash`, but DIFFERENT output shape (labeled vs. unlabeled `Tessellation`). Without a key extension, the cache would return a labeled tessellation when an unlabeled call expected the unlabeled fast path, or vice versa.

The fix: `effective_hash_and_label` folds the upstream-labeled-bitmap (1 bit per input port, packed into a `u32` modulo 32 for arity-32+ operators) into the BLAKE3 hash. Two cache entries with the same operator + parameters but different upstream-labeled state hash distinctly. The bitmap is small (4 bytes for any reasonable arity) and the BLAKE3 fold is constant-time. Hot-path cost: one extra BLAKE3 update per cache-key computation.

### Test recipes (mandatory before declaring D-7.4 follow-ons done)

1. `infer_lineage` on `cube` → identity-transformed `cube`: 6 `Preserved` edges, no `Reinterpreted`.
2. `infer_lineage` on `cube – sphere`: surviving lhs faces classified `Preserved` (using csgrs metadata-passthrough), new spherical-cap face classified `Reinterpreted`.
3. `infer_lineage` on a deliberately-coplanar two-input fixture: both faces collapse to one `Preserved` ↔ one `Merged` (multi-edge, shared `to`).
4. `label_by_plane` on a degenerate-triangle-laden mesh: every degenerate triangle assigned `TopologyFaceId::DEGENERATE`; no panic; `face_labels.len() == triangle_count`.
5. Determinism gate: same fixture × N iterations → BLAKE3-identical `LineageGraph` serialization across runs.

## Followups / open questions

- **PersistentFaceId (Phase 7.2 dispatch).** v0's per-mesh sequential `TopologyFaceId` is not stable across rebuilds. Phase 7.2 must add a content-hash + lineage-path identifier so the editor can show "this face came from rev 12's Cuboid bottom-face → rev 17's Boolean cut" across saves. Tracked in HANDOFF.md.
- **`kernel/graph-foundation::Graph` backing.** v0 stores `LineageGraph::edges` as a `Vec<LineageEdge>`. The PLAN-spec'd graph-foundation backing materializes when traversal queries (ancestors / descendants / multi-step lineage walks) become a real downstream consumer — currently no such consumer exists, so the simpler `Vec` ships.
- **`OperatorId` field on `LineageEdge`.** Depends on a stable operator-instance identity beyond `NodeId` (the operator-graph version-tracking dispatch). Defer until the operator-graph itself version-tracks individual operator instances.
- **csgrs Difference rhs-retag special-casing.** Documented in `infer_lineage_labeled` comments; if upstream csgrs changes Difference's metadata semantics, the special-case has to update. Watched in the dependency-audit cadence.
- **Connected-component analysis for true Split detection.** v0's triangle-count comparison is a proxy. A real Split detector groups triangles by connected-edge adjacency on the matching plane and counts components. Defer until the simpler heuristic surfaces a real downstream blocker.
- **`SemanticScore` field on `LineageEdge`.** Richer than the binary-ish v0 `confidence`. Defer until a downstream consumer (e.g. UI confidence visualization) demands it.

## References

- PLAN.md §1.5.4.3 (topology lineage), §1.5.4.2 (persistent topology IDs), §1.5.4.4 (kernel non-equivalence)
- IMPLEMENTATION.md §7 (Phase 7 — CAD Spike risk profile), §7.4 (topology-lineage gate)
- ADR-112 §"Phase 7.2 / 7.4 hook" (the truck-deferred lineage hook this ADR ships)
- ADR-104 (capability surface — `output_labeled_when_input_labeled` field)
- change.md entries for D-7.4 (2026-05-07), D-7.4-followup (2026-05-07), HIGH #3 (2026-05-08)
- `crates/cad-core/src/topo_lineage/mod.rs` (module-level v0-simplifications doc)
- `crates/cad-core/src/topo_lineage/plane.rs` (`QuantizedPlane` quantization + sign canonicalization)
- `crates/cad-core/src/topo_lineage/infer.rs` (`label_by_plane`, `infer_lineage`)
- `crates/cad-core/src/tessellation/mesh.rs` (`TopologyFaceId`, `Tessellation::face_labels`)
