# ADR-119: Real round fillet substrate — new operator beside chamfer `FilletOp`

| Status | Accepted 2026-05-12 (binding substrate decisions for the D-Fillet real round geometry chapter; implementation deferred to chapter sub-α onward) |
|---|---|
| Date | 2026-05-12 |
| Deciders | (RGE architecture review) |
| PLAN references | §7.1 (cad-core MVP — listed operators) at `IMPLEMENTATION.md:497` verbatim "Full operator library (Fillet, Loft, Sweep, Shell come later)"; §1.5.4 (cad-core architecture); §1.5.4.3 (topology lineage — fillet-survives-Split scenario at `PLAN.md:300` "if a fillet edge survives via Split into two, both inherit the constraint with `confidence` annotation"); §1.5.4 history-UI scenario at `PLAN.md:303` "this fillet came from face X which was the result of boolean Y…"; PLAN.md Months 7-12 roadmap at `PLAN.md:1086` "cad-core full operator set + Fillet + lineage + history" |
| §18 references | `CAD_CORE_MODEL.md` (operator-graph + tessellation + identity contracts); `CAD_TOPOLOGY_LINEAGE.md` (TopologyEvolution + topology-changing operators) |
| ADR references | ADR-098 (topology-lineage substrate — naming-by-shape vs naming-by-cut precedent); ADR-104 (capability surface — face-tag enum substrate precedent for cap-face identity if pressure surfaces); ADR-115 (graph-metrics substrate design — semantics-first ADR precedent); ADR-118 (frame-graph transient allocator — sibling design ADR for a Phase-6 substrate chapter; same "ADR pins policy, dispatches land code" cadence) |
| Doctrine refs | `docs/architecture/FILLET_OUTPUT_IDENTITY.md` — PARKED design note (pre-sub-ε.α/β); the open questions it documents are now ANSWERED for chamfer `FilletOp` (sub-ε.α landed face-inheritance + sub-ε.β landed filtered-edge-inheritance via the graph resolvers; sub-ε.γ halt-and-reframed direct `BRepProvider for FilletOp` impl until cap-face consumer pressure surfaces); this ADR-119 supersedes its open-questions section for the real round fillet case specifically. `docs/architecture/NON_GOALS.md` — round-fillet listed as deferred per §4. |
| Implementation phase | **This ADR: design-only.** Chapter sub-α (`Cuboid` round fillet — straight 2-endpoint edges) through later subs (Extrude / Revolve cap-side / Loft → multi-edge corner blending → circular-path Revolve via multi-segment spec) implement the policy pinned here. No source code lands in this ADR's dispatch. |

## Context

### What chamfer-style `FilletOp` is today

`FilletOp` ([`crates/cad-core/src/operators/fillet/mod.rs`](crates/cad-core/src/operators/fillet/mod.rs)) is a **chamfer-approximation** operator: per filleted edge, it adds **2 new vertices + 2 new triangles** that form a flat chamfer-cap connecting the original edge endpoints with inward-offset replicas. The per-edge data is carried by `ChamferSpec { vertex_a: u32, vertex_b: u32, inward_direction: [f32; 3] }` — hardcoded for 2-endpoint edges.

`FilletOp::evaluate` ([fillet/mod.rs:336-405](crates/cad-core/src/operators/fillet/mod.rs:336)) **clones upstream `positions` and `indices` verbatim and APPENDS the chamfer geometry**. This "append, never modify" structural property is **LOAD-BEARING** for:

- **Sub-ε.α** face-identity inheritance: every upstream face exists bit-identical in the output mesh; the graph resolver's `OperatorNode::Fillet(_)` arm at [`topology/resolve.rs::resolve_step`](crates/cad-core/src/topology/resolve.rs) recurses to the upstream and returns its `BRepFaceId`s unchanged. Chamfer caps are face-id-orphaned (intentional v0 simplification).
- **Sub-ε.β** edge-identity filtered inheritance: non-filleted upstream edges have bit-identical 2-endpoint geometry in the output (only chamfer-cap vertices are *added* near filleted-edge corners); the graph resolver's `OperatorNode::Fillet(op)` arm at [`topology/edge_resolve.rs::resolve_step`](crates/cad-core/src/topology/edge_resolve.rs) returns `upstream_edges \ op.edges()`. Filleted edges are excluded because their 2-endpoint identity is "absorbed" by the chamfer.

Per-upstream impls today: `Cuboid` (12 edges), `Extrude` (3n edges), `Revolve` Partial cap-side (2n of 3n edges; Full-mode + Partial side-side return `FilletError::UnsupportedEdgeGeometry` at construction time because they are **circular paths** through `segments` vertices — incompatible with `ChamferSpec`'s 2-endpoint structure), `Loft` (3n edges). All v0 chamfer impls use `ChamferSpec` with single-segment geometry.

### What is documented as out-of-scope today

The chamfer-style `FilletOp` module-doc enumerates the explicit NON-GOALS that this ADR addresses ([fillet/mod.rs:30-49](crates/cad-core/src/operators/fillet/mod.rs:30) verbatim):

> "Real round-fillet geometry (quarter-cylinder tessellation, face-strip removal, multi-edge corner blending, curvature continuity) is OUT OF SCOPE."
>
> "No multi-edge corner-sharing geometry. The chamfer is per-edge independent; if two filleted edges share a corner, the geometry may be visually weird, but the substrate-validation test does not exercise that case."
>
> "No support for circular-path Revolve edges in v0. Side-side adjacencies (Full and Partial) return `FilletError::UnsupportedEdgeGeometry` at construction time rather than fabricating geometry."

Three prior dispatches halt-and-reframed work into "real round fillet chapter" territory rather than extending the chamfer pattern:

- **D-Fillet sub-ε.γ** (2026-05-12) — inspection halt on direct `impl BRepProvider for FilletOp`. The three candidate paths (cache upstream face IDs in `FilletOp`; mint chamfer-cap face IDs with new `KIND_FILLET` bytestring; return empty Vec) all tripped halt-fast guardrails. The honest conclusion: the resolver-layer identity (sub-ε.α) IS the honest provider view for chamfer `FilletOp`; cap-face identity is pressure-deferred. Real round fillet's cap-face identity is reopened by this ADR.
- **D-Fillet circular-path Revolve sub** (2026-05-12) — inspection halt on lifting `FilletError::UnsupportedEdgeGeometry` for Revolve side-side edges. The geometry is a swept circular path through `segments` vertices; `ChamferSpec`'s 2-endpoint structure cannot represent it. Three candidate paths (multi-segment `ChamferSpec` reshape; trait reshape via new `SpecKind` enum; endpoint-only chamfer) all required either substrate reshape, trait reshape, or dishonest semantics. The honest conclusion: this belongs in the real round fillet chapter, not as a chamfer extension. Reopened by this ADR.
- **D-Fillet sub-ε.γ alternate** — `face_labels` propagation through `evaluate` would require either reusing `TopologyFaceId::DEGENERATE` for chamfer caps (semantic conflation), inventing a new sentinel (new tag), or borrowing an adjacent face's label (fake propagation). Honest conclusion: defer.

### Pressure for real round geometry

PLAN.md commits to `Fillet` in Months 7-12 (`PLAN.md:1086`): "cad-core full operator set + Fillet + lineage + history". The lineage scenario at `PLAN.md:300` requires fillet edges to participate in `TopologyEvolution::Split` — fillets that survive a Boolean split inherit constraints with confidence annotations. The history-UI scenario at `PLAN.md:303` describes "this fillet came from face X which was the result of boolean Y" — requires the fillet's filleted-edge identity to be addressable across operator-graph rebuilds.

Both PLAN-level commitments require **real round fillet geometry** with stable curved-edge identity. The chamfer-style `FilletOp` is honest about what it is (a fast preview) but cannot fulfill these commitments alone.

### Why this is ADR-territory, not MAY-list-territory

The substrate evolution required for real round fillet spans 8+ binary-or-larger decisions with cascading consequences:

1. New operator vs in-place evolution of `FilletOp`?
2. Curved-edge identity inheritance vs new ID minting?
3. Cap-face / corner-patch identity nameless vs named-with-new-`KIND_*`-bytestring?
4. Face-strip removal — original face retains `BRepFaceId` or new ID?
5. Substrate carrier — new struct vs generalized enum?
6. Per-upstream trait — new sibling vs extension of existing?
7. Chapter shape — which upstreams + when?
8. Multi-edge corner blending — same chapter vs separate?

Each is substrate-architectural. Code-first commitment without ADR would silently lock-in choices that subsequent dispatches inherit. ADR-118 (frame-graph allocator policy) set the precedent: pin policy in an ADR, land code in subsequent dispatches.

## Decision

### D1 — NEW operator (`RoundFilletOp`) beside chamfer `FilletOp`, NOT in-place evolution

Real round fillet ships as a **new operator type** (working name: `RoundFilletOp`) added alongside the existing chamfer `FilletOp`. The chamfer `FilletOp` is preserved verbatim — its module, struct, impls, `ChamferSpec`, `FilletUpstream` trait, per-upstream impls, and sub-ε.α/β resolver arms stay byte-identical.

**Rationale**:

- **Structural incompatibility**: `ChamferSpec` is hardcoded for 2-endpoint single-segment edges. Round fillet needs multi-segment path spec + cross-section geometry + corner-patch geometry. An enum-within-`ChamferSpec` would force the entire chamfer-style pattern (4 per-upstream impls + resolver arms + tests) to switch on the variant — high coupling cost, no isolation benefit.
- **`evaluate` semantics fundamentally differ**: chamfer APPENDS upstream geometry verbatim; round fillet MODIFIES upstream (face-strip removal). Mode-enum-in-`FilletOp::evaluate` would force a 2-case-split body that's larger than either case alone — and the test surface would multiplicatively explode.
- **Identity contracts differ**: chamfer's sub-ε.α/β resolver arms (face pass-through, edge filtered-pass-through) hold BECAUSE chamfer doesn't modify upstream geometry. Round fillet's identity contract is fundamentally different (curved-edge inheritance + face-strip-preserves-identity + cap-face pressure-driven) — needs its own resolver arms with different semantics.
- **Chamfer stays useful for fast preview / debugging**: chamfer's `+2 verts + 2 tris per edge` is constant-time per edge; round fillet's quarter-cylinder tessellation is O(tessellation_segments) per edge. Users authoring scenes can iterate on edge selection with chamfer FilletOp and switch to RoundFilletOp for final geometry. Two operators serve two distinct use cases.
- **`OpKind` variant additivity**: `OpKind::RoundFillet` is a new `#[non_exhaustive]` variant alongside `OpKind::Fillet`. Existing match-on-OpKind sites get the standard non-exhaustive wildcard treatment; no breaking change to pattern-match consumers.

**Rejected alternative**: mode-enum within `FilletOp`. Trips coupling-cost, test-surface explosion, and identity-contract conflation simultaneously. No isolation benefit. See §Alternatives.

### D2 — Filleted curved edges inherit upstream `BRepEdgeId`

When `RoundFilletOp` rounds an upstream edge, the resulting curved edge in the output **inherits the upstream's `BRepEdgeId` byte-identical**. The edge's geometric shape changes (from a 2-endpoint line segment to a swept curve); its **semantic identity stays the same** ("the +Z∩+X edge of the cube, now rounded").

**Rationale**:

- **Caller continuity**: editor selections, downstream constraint references, scripting-API handles all use `BRepEdgeId` as the addressable handle. Breaking that handle on rounding would force callers to re-select edges after every fillet edit — unusable.
- **Consistency with sub-7.2-ε identity-preserving-operators precedent**: `TransformOp` doesn't change topology and inherits upstream face/edge IDs verbatim. `RoundFilletOp` doesn't change topological-identity (the edge is still "the edge between face X and face Y") — only the edge's geometric shape changes. Same identity-preserving semantic class.
- **Distinct from D-Fillet sub-ε.β chamfer-edge-exclusion**: chamfer ABSORBS filleted edges (the 2-endpoint identity is replaced by the chamfer-cap geometry); the resolver excludes them. Round fillet PRESERVES filleted edges as curves — the resolver INCLUDES them with byte-identical upstream IDs. Different geometric semantics → different resolver behavior.
- **`BRepEdgeId::for_face_pair` derivation is shape-agnostic**: the `BRepEdgeId` bytes derive from `(owner, face_a_id, face_b_id)` only — not from edge geometry. Both straight and curved instantiations of "the edge between face X and face Y" produce the same `BRepEdgeId`. Substrate already supports this; no new derivation needed.

### D3 — Cap-face / corner-patch identity is PRESSURE-DRIVEN (nameless for v0)

The rolled quarter-cylinder cap-surface (one per filleted edge) and torus corner-patches (one per multi-edge corner) have **no `BRepFaceId` in v0**. They are nameless geometry — addressable by `TopologyFaceId` (per-tessellation sequential) but NOT by `BRepFaceId` (rebuild-stable).

**Rationale**:

- **No consumer pressure today**: there is no production code path that selects-per-cap or assigns-per-cap material. The chamfer caps in chamfer-style `FilletOp` have been nameless since D-Fillet sub-α landed and no consumer has surfaced demanding addressability.
- **Doctrine-as-substrate posture** (per workspace memory `rge_doctrine_as_substrate_posture.md`): inventing identity infrastructure ahead of consumer pressure trips the "validation-framework / centralization / taxonomy / extraction pressure" anti-pattern. Cap-face identity is a `KIND_FILLET_ROUND_CAP` BLAKE3 bytestring + a face-tag enum (analogous to `CuboidFaceTag` / `ExtrudeFaceTag`) + per-cap canonical-index ordering — substantial substrate for no current consumer.
- **Path to lift when pressure surfaces**: if/when a per-cap material assignment system or a cap-face selection-persistence requirement lands, the lift is well-scoped: add `BRepFaceId::KIND_FILLET_ROUND_CAP` + `RoundFilletCapFaceTag` + a `BRepProvider` impl on `RoundFilletOp` that returns cap-face IDs alongside upstream IDs. Documented as the lift path; not implemented in v0.
- **Caller workaround in v0**: callers needing to address cap surfaces can use `TopologyFaceId` from the output `Tessellation` (per-mesh sequential) — same workaround chamfer caps already use. Not as stable across operator-rebuild as `BRepFaceId`, but sufficient for transient consumers (e.g., per-frame visual highlighting).

### D4 — Original faces RETAIN `BRepFaceId` under face-strip removal

When `RoundFilletOp` removes triangles from an upstream face's tessellation (face-strip removal — the rolled surface eats into the adjacent face's boundary), the affected face **retains its upstream `BRepFaceId` byte-identical**. Identity = semantic surface ("the +Z face of the cube"), not mesh shape (triangle count, vertex list).

**Rationale**:

- **`BRepFaceId` derivation is shape-agnostic**: identity derives from `(owner, kind_tag_bytes)` (per ADR-104's substrate). Triangle count + vertex list don't enter the derivation. A face's triangle count can change (face-strip removal, retessellation, LOD) without affecting its identity.
- **Caller continuity** (same rationale as D2 for edges): editor selections + downstream constraints + scripting-API handles must not break when a face's tessellation mutates.
- **Forward compatibility** with future ops that mutate face tessellation: `ShellOp` (offsets the entire surface; per-face tessellation changes), `BooleanOp` partial overlap (face-strip removal where the cut intersects), eventual `FilletOp` v1 with chamfer-on-face-edge support. All can adopt this identity contract verbatim — "identity = semantic surface, not mesh shape" is a substrate-wide commitment, not a `RoundFilletOp`-specific choice.
- **Consistency with sub-7.2-ζ.ε Transform precedent**: `TransformOp` changes vertex positions (placement-only) without changing face IDs — same identity-preserving semantic class.

### D5 — Spec / trait / resolver substrate is PARALLEL to chamfer's, not shared

The substrate for `RoundFilletOp` lives parallel to chamfer's:

- **`RoundFilletSpec`** (working name): new struct carrying per-filleted-edge data. Shape TBD by implementation (likely `{ edge_path: Vec<u32>, cross_section_radius: f32, ... }` — the exact field set is a sub-α decision, not pinned by this ADR). Distinct from `ChamferSpec`.
- **`RoundFilletUpstream`** (working name): new `pub(crate)` trait, sibling to `FilletUpstream` ([fillet/mod.rs:185-200](crates/cad-core/src/operators/fillet/mod.rs:185)). Per-upstream-operator method `resolve_round_spec(canonical_index: usize) -> Result<RoundFilletSpec, &'static str>`. Per-upstream impls live alongside chamfer's `FilletUpstream` impls.
- **Resolver arms**: new arm `OperatorNode::RoundFillet(_)` in `topology::resolve::brep_face_ids_for_node` AND in `topology::edge_resolve::brep_edge_ids_for_node`. The face-resolver arm handles face-strip-preserves-identity (D4); the edge-resolver arm handles curved-edge-inherits-upstream-ID (D2). NEITHER arm reuses chamfer's `OperatorNode::Fillet(_)` arm logic — different geometric semantics, different identity contracts.

**Rationale**:

- **No shared substrate**: chamfer's `ChamferSpec` + `FilletUpstream` + resolver arms are LOAD-BEARING for chamfer's sub-ε.α/β identity contract (which depends on "append, never modify"). Sharing substrate with round fillet (which modifies) would force the shared substrate to encode both semantics — coupling cost without benefit.
- **Independent evolution**: chamfer FilletOp can be deprecated, extended, or removed independently of `RoundFilletOp` evolution. Substrate isolation enables this.
- **Resolver arm independence**: chamfer's face-arm is `recurse_to_upstream(unchanged)`; round's face-arm is `recurse_to_upstream + face-strip-removal-acknowledgment` (identity unchanged but mutation logged for any downstream consumer that cares). Different bodies; clean separation.

### D6 — Existing chamfer `FilletOp` retains sub-ε.α/β identity behavior verbatim

The existing chamfer `FilletOp` — its module, struct, `ChamferSpec`, `FilletUpstream` trait, 4 per-upstream impls (Cuboid + Extrude + Revolve + Loft), `evaluate` body, sub-ε.α face-identity pass-through resolver arm, sub-ε.β filtered-edge-inheritance resolver arm, and all existing tests — is **PRESERVED verbatim**. No deprecation, no API change, no behavior change.

**Rationale**:

- **Two distinct use cases**: chamfer for fast preview (constant-time per edge; visual-debug-friendly; no face-strip mutation); round fillet for final geometry (curvature continuity; ProjectedMesh-quality output).
- **Sub-ε.α/β identity contract HELD**: chamfer's "append, never modify" property is structurally true and load-bearing for sub-ε.α/β. Removing chamfer would break those resolver arms; preserving chamfer preserves the contract.
- **No coupling cost**: chamfer and round fillet don't share substrate (per D5), so chamfer's preservation has zero cost to round fillet's design.
- **Pressure-driven deprecation**: if `RoundFilletOp` proves sufficient for all production use cases (no fast-preview consumer surfaces), chamfer `FilletOp` could be deprecated in a future dispatch. NOT this ADR's scope.

### D7 — Chapter shape: 4 straight-edge subs + multi-edge corner blending sub + circular-path Revolve sub

The implementation chapter for `RoundFilletOp` ships as 6 sub-dispatches:

| Sub | Scope | Upstream class | Approximate LoC |
|---|---|---|---|
| **sub-α** | `RoundFilletOp` substrate + `Cuboid` upstream impl | Cuboid (12 straight 2-endpoint edges) | ~500-700 LoC (new operator + new spec + new trait + Cuboid impl + tests; sets the precedent) |
| **sub-β** | `Extrude` upstream impl | Extrude (3n straight 2-endpoint edges) | ~250-350 LoC (additive per-upstream impl + tests) |
| **sub-γ** | `Revolve` cap-side upstream impl (Partial mode only — 2n cap-side edges) | Revolve Partial cap-side (2n straight 2-endpoint edges) | ~250-350 LoC |
| **sub-δ** | `Loft` upstream impl | Loft (3n straight 2-endpoint edges) | ~250-350 LoC |
| **sub-ε** | Multi-edge corner blending (torus-patch generation at corners where 2+ edges meet) | Cross-cutting (affects all 4 upstreams; per-upstream tests added) | ~400-600 LoC (algorithm + per-upstream tests; depends on substrate decisions in sub-α) |
| **sub-ζ** | Circular-path Revolve via multi-segment spec | Revolve Full-mode (n circular-path side-side edges) + Revolve Partial side-side (n circular-path side-side edges) | ~400-600 LoC (multi-segment spec evolution + Revolve impl + tests; the substrate sub-ε.β-of-D-Fillet-output-identity halted on) |

**Rationale**:

- **Sub-α through sub-δ are precedent + 3 mirrors**: sub-α sets the round-fillet substrate (spec + trait + resolver arms + evaluate body) plus the first upstream impl. Sub-β/γ/δ are near-mechanical mirrors at the same upstream-impl size as the chamfer sub-α/β/γ/δ chapter (~250-350 LoC each based on commit history).
- **Sub-ε (corner blending) is cross-cutting**: torus-patch generation at multi-edge corners requires the rolled-cylinder substrate from sub-α to exist. Cannot land before sub-α; can land after any of α/β/γ/δ.
- **Sub-ζ (circular-path Revolve) is independent of sub-ε**: multi-segment spec extension is orthogonal to corner-blending. Can ship in parallel with sub-ε or sequentially.
- **No commitment to sub-ordering** in this ADR. Each sub's halt-and-reframe boundary is preserved; user can pause / pivot between subs without breaking chapter invariants.

### D8 — Multi-edge corner blending + circular-path Revolve are EXPLICIT LATER SUBS, not sub-α

Per D7, sub-α targets `Cuboid` with **single-edge fillets only** (no corner blending) and **no circular-path edges**. The visibility of these constraints in sub-α is explicit:

- Sub-α `RoundFilletOp::new` for `CuboidOp` accepts a `Vec<BRepEdgeId>` (mirrors chamfer's API surface) but the per-edge geometry generation assumes ISOLATED edges. If two edges in the selection share a corner, sub-α produces **visually weird but topologically valid** geometry (the two rolled surfaces don't blend into a torus-patch at the corner — they overlap/gap depending on the corner angle). This is the SAME degeneracy chamfer `FilletOp` has at multi-edge-corner-sharing selections; sub-α matches the chamfer precedent.
- Sub-α rejects circular-path edges at construction time (via `FilletError::UnsupportedEdgeGeometry` — same error variant chamfer uses for the same case; substrate-honest signal). Sub-ζ later lifts this.

**Rationale**:

- **Honest scope per chapter sub**: each sub closes a bounded scope; sub-α is "round-fillet substrate + Cuboid isolated-edge case." Bundling corner blending or circular paths into sub-α would expand it to 1000+ LoC and force premature decisions on torus-patch parameterization or multi-segment spec shape.
- **Precedent from D-Fillet chamfer sub-α/β/γ/δ**: those subs shipped one upstream per dispatch with documented per-sub constraints (sub-γ Revolve has the "cap-side only" scope; circular-path side-side rejects at construction). Same scoping discipline carries to `RoundFilletOp` subs.
- **Sub-ε / sub-ζ are unblocked by sub-α**: they don't require sub-β/γ/δ to land first. User can reorder if pressure differs.

## Consequences

### What this ADR commits to (binding for subsequent dispatches)

- **New `RoundFilletOp` operator + new `OpKind::RoundFillet` variant** (D1, D5, D6)
- **New `RoundFilletSpec` substrate type** (D5)
- **New `pub(crate) trait RoundFilletUpstream`** (D5)
- **New `OperatorNode::RoundFillet(_)` resolver arms in both `topology::resolve` and `topology::edge_resolve`** (D5)
- **Curved-edge inheritance of upstream `BRepEdgeId`** (D2)
- **Face-strip-preserves-`BRepFaceId`** (D4)
- **Cap-face / corner-patch identity is nameless in v0** (D3 — lift path documented but unimplemented)
- **Existing chamfer `FilletOp` byte-identical** (D6)
- **Chapter shape: 6 sub-dispatches in flexible order** (D7, D8)

### What this ADR leaves open (deferred to sub-α onward)

- **`RoundFilletSpec` exact field set** — depends on sub-α's algorithm choice (likely `{ edge_path: Vec<u32>, cross_section_radius: f32, ... }` but the per-segment cross-section parameterization is sub-α's call)
- **Tessellation segments per quarter-cylinder** — sub-α-level knob; likely 8-16 default with a Tolerance-driven LoD path
- **Face-strip removal algorithm** — sub-α's call: triangle-by-triangle removal vs face-boundary-clipping vs hybrid
- **Variable-radius fillets** — out of v0 scope; if pressure surfaces, sub-η extension via spec evolution
- **Curvature continuity (G1/G2)** — v0 produces tessellation-level quarter-cylinder geometry; analytical curvature continuity is out of scope; if pressure surfaces, sub-θ work
- **Lineage propagation through `TopologyEvolution`** (per ADR-098) — `RoundFilletOp` is topology-preserving (face/edge IDs inherit per D2/D4), so the `Preserved` lineage variant applies. If multi-edge corner blending introduces a true topology change (e.g., a corner-patch ID), that case extends `TopologyEvolution::Split` semantics. Sub-ε's scope.
- **csgrs / kernel-geometry library backing** — v0 is hand-tessellated quarter-cylinders (no external dep). If a future CAD-kernel integration (ADR-112 csgrs precedent) lands, `RoundFilletOp` could route through it. Out of scope here.

### What this ADR DOES NOT change

- **Chamfer `FilletOp`**: ADR-119 did not change it. Later ADR-120
  changed only tessellation face-label propagation for labeled input; the
  chamfer operator remains separate from `RoundFilletOp` and still does not
  mint stable B-Rep IDs for chamfer caps.
- **PLAN §5.6 / §7.1 targets**: unchanged; PLAN already commits to `Fillet` in Months 7-12.
- **`BRepEdgeId::for_face_pair` derivation**: unchanged; shape-agnostic identity derivation already supports curved-edge inheritance per D2.
- **`BRepFaceId` derivation**: unchanged; identity = semantic surface already supported per D4.
- **Topology lineage substrate (ADR-098)**: `TopologyEvolution` enum + the `topo_lineage` module — unchanged; `RoundFilletOp`'s topology-preserving nature fits the `Preserved` variant verbatim.
- **`docs/architecture/FILLET_OUTPUT_IDENTITY.md`**: was parked when this ADR
  landed. ADR-120 later updated it to record chamfer `FilletOp` face-label
  propagation and the still-deferred cap-face stable-ID question.

### Workspace + test-count impact projection (subsequent dispatches; not this ADR)

- Sub-α: workspace tests +12 to +18 (new operator + spec + trait + Cuboid impl + 8-12 unit tests + 2 resolver-arm tests + 1-2 evaluate-output tests)
- Sub-β/γ/δ: +6 to +10 each (additive per-upstream impl + tests)
- Sub-ε: +8 to +12 (corner blending + per-upstream corner tests)
- Sub-ζ: +6 to +10 (multi-segment spec + Revolve circular-path impl + tests)

Total chapter projection: workspace +44 to +68 tests over 6 sub-dispatches.

## Alternatives considered

### Alt 1 — In-place evolution of `FilletOp` with a mode enum

```rust
enum FilletGeometry {
    Chamfer(ChamferSpec),
    Round(RoundFilletSpec),
}

struct FilletOp {
    edges: Vec<BRepEdgeId>,
    specs: Vec<FilletGeometry>,
    radius: f32,
    owner: BRepOwnerId,
}
```

`FilletOp::evaluate` switches on the spec variant per-edge.

**Rejected because**:

- Forces `evaluate` body to handle both append-only (chamfer) and modify-upstream (round) semantics in one function — case-split body larger than either case alone
- Forces ALL 4 per-upstream `FilletUpstream` impls to handle both `ChamferSpec` and `RoundFilletSpec` returns — per-upstream code doubles
- Forces sub-ε.α/β resolver arms (which depend on "append, never modify") to either work-or-break depending on which spec variant is in the FilletOp — identity contract becomes per-instance-conditional rather than per-operator-type
- Test surface multiplies: every chamfer test needs a round-mode counterpart in the same test file
- Mode-aware API for callers: `FilletOp::new` becomes `FilletOp::new_chamfer` / `FilletOp::new_round` or carries a mode flag — same surface size as two operators, with worse separation

The mode-enum approach has all the surface-area cost of two operators with none of the isolation benefit. Strictly inferior.

### Alt 2 — Skip ADR; write sub-α directly with implicit decisions

Ship sub-α (Cuboid round fillet) and let the decisions in D1-D8 emerge implicitly from the implementation choices.

**Rejected because**:

- Per workspace memory `rge_audit_layer_classification.md`: undocumented architectural commitments accumulate "Layer-2 governance-surface drift" — substrate evolves without explicit reasoning, and subsequent dispatches inherit unstated invariants.
- Per ADR-118 precedent: substrate-defining decisions get an ADR; implementation-detail decisions don't. Real round fillet is substrate-defining (new operator + new spec + new trait + new resolver arms).
- Sub-α's design choices (`RoundFilletSpec` field set, face-strip algorithm, identity inheritance) cascade into sub-β/γ/δ/ε/ζ. Implicit decisions in sub-α get re-litigated at each subsequent sub — high cost across the chapter.
- The 8 decisions D1-D8 are NOT implementation details (e.g., "use a Vec or a HashMap"). They're architectural commitments to identity contracts, substrate isolation, and chapter shape. ADR is the right venue.

### Alt 3 — Defer the entire chapter; chamfer FilletOp is "good enough" for v0

Don't write `RoundFilletOp` at all. Chamfer FilletOp serves all v0 fillet use cases.

**Rejected because**:

- PLAN.md commits to `Fillet` in Months 7-12 (`PLAN.md:1086`) and to fillet-edge-survives-Split lineage at `PLAN.md:300`. Chamfer FilletOp's `+2 verts + 2 tris per edge` is not what those PLAN commitments envision.
- The user-facing UI scenario at `PLAN.md:303` ("this fillet came from face X which was the result of boolean Y") requires fillet-edge identity to be addressable across rebuilds. Chamfer FilletOp absorbs filleted-edge identity (sub-ε.β resolver excludes them); cannot fulfill the scenario.
- "Good enough for v0" is a defensible deferral framing IF no PLAN commitment exists. Here, PLAN explicitly commits to `Fillet` as a "full operator library" item. Defer would be a PLAN re-target, not a v0-scoping decision.
- Halt-entirely is still a USER choice (see "If this ADR is accepted but chapter sub-α is not green-lit, the chapter stays parked"). This ADR doesn't commit to writing sub-α; it commits to the substrate decisions IF/WHEN sub-α lands.

### Alt 4 — Cap-face identity NAMED for v0 (Decision D3 reverse)

Mint `BRepFaceId::KIND_FILLET_ROUND_CAP` + `RoundFilletCapFaceTag` substrate at sub-α; cap surfaces have addressable identity from day 1.

**Rejected because**:

- No production consumer pressure today. Per workspace memory `rge_doctrine_as_substrate_posture.md`, inventing identity infrastructure ahead of pressure trips the "validation-framework / centralization / taxonomy / extraction pressure" anti-pattern.
- Substrate cost: new `KIND_*` bytestring + new face-tag enum + per-upstream cap-face-tag ordering canonicalization + `BRepProvider` impl on `RoundFilletOp` + tests for the named-identity contract. ~150-250 LoC of substrate work for zero current consumer.
- Path to lift well-scoped: if pressure surfaces (per-cap material assignment; cap-face selection-persistence; per-cap LoD), the lift is additive (mint the identity at the operator + add the provider impl) — no cascading substrate changes. Better to wait for pressure.
- Symmetry with chamfer caps: chamfer `FilletOp` has kept caps nameless since D-Fillet sub-α (2026-05-08) and no consumer has demanded otherwise. Round fillet matches.

### Alt 5 — Filleted-edge identity NEW ID (Decision D2 reverse)

When `RoundFilletOp` rounds an upstream edge, mint a NEW `BRepEdgeId` for the resulting curved edge (e.g., `BRepEdgeId::for_round_fillet(owner, upstream_edge_id)` BLAKE3 of the upstream-edge-id under a `KIND_FILLET_ROUND_EDGE` domain separator).

**Rejected because**:

- Breaks caller handle continuity: editor selections, scripting handles, downstream constraint references all break on rounding. Every fillet operation forces every consumer to re-select edges.
- No semantic justification: the edge IS the same edge (between the same two faces); only the geometric shape changes. Identity = semantic, not geometric (per D4's rationale and the existing `BRepEdgeId::for_face_pair` shape-agnostic derivation).
- Cascading complexity: `TopologyEvolution::Preserved` (ADR-098) variant doesn't apply if IDs change; would force a `TopologyEvolution::Reinterpreted` variant for "same edge, new shape" — confusing semantic.
- No precedent in the workspace: `TransformOp` doesn't change IDs (placement, not topology). `RoundFilletOp` is topology-preserving in the same sense — only geometric shape changes.

## Notes on doctrine + workspace memory alignment

This ADR aligns with the workspace's accumulated decision-making posture per the agent memory:

- **`rge_doctrine_as_substrate_posture.md`** — "doctrine-as-substrate validated 2026-05-11 via Loft op dispatch: orthogonal substrate work absorbs governance doctrine without surfacing expansion pressure; 'no ADR / no lint / no exemption' is success; resist validation-framework / centralization / taxonomy / extraction pressure during routine work." This ADR is NOT routine work — it's substrate-defining for a new operator chapter. ADR is warranted.
- **`rge_dispatch_pattern.md`** — "bounded scope, explicit MAY/MUST-NOT file lists, inline API spec, canned verification commands; commit + push origin main once per dispatch (do not batch)." This ADR is the design-pin dispatch; sub-α onward are the implementation dispatches. ADR + 6 implementation subs = 7 dispatches total for the chapter.
- **`rge_audit_layer_classification.md`** — "never retroactively author missing ADRs to fix phantom references." This ADR is authored AT decision-time (before sub-α), not retroactively.
- **`synthesis_seven_axis_decomposition.md`** — substrate decisions span the mechanization / doctrine / topology / authority axes; ADR captures all four for cross-axis coherence.

## Decision deciders

(RGE architecture review)

---

**Implementation status**: This ADR is design-only. Sub-α through sub-ζ are UNCOMMITTED — the user (or future architecture review) decides whether and when to open chapter sub-α. This ADR commits to the SUBSTRATE DECISIONS for the chapter IF AND WHEN it opens.
