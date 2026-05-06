# ADR-112: cad-core Boolean CSG library

| Status | Accepted 2026-05-06 (D-Boolean shipped + D-7.4-followup metadata-passthrough integrated) |
|---|---|
| Date | 2026-05-06 |
| Deciders | (RGE architecture review) |
| PLAN references | §1.5.4 (cad-core), §1.5.4.2 (persistent topology IDs), §1.5.4.3 (topology lineage), §1.5.4.4 (CAD kernel non-equivalence + capability surface), §1.6.8 (determinism modes), §1.13 (failure classes), §13.2 / §13.6 (CAD validation gates) |
| ADR references | ADR-097 (cad-projection split), ADR-098 (topology lineage), ADR-101 (graph-foundation), ADR-104 (CAD kernel non-equivalence + capability surface) |
| Implementation phase | Phase 7 — CAD Spike (HIGHEST SECONDARY RISK; "Many architectures die here" per IMPLEMENTATION.md §7) |

## Context

The Phase 7.1 cad-core operator catalog now contains four operators — `Cuboid`, `Transform`, `Extrude`, `Revolve` — all hand-rolled and producing the workspace's `Tessellation { positions: Vec<[f32; 3]>, indices: Vec<u32> }` triangle-soup output. Boolean (`Union | Intersection | Difference`) is the next operator on the Phase 7 path per IMPLEMENTATION.md §7.1, and it is qualitatively different from everything that came before it. Cuboid/Extrude/Revolve are *generative* — they build vertices from parameters with closed-form formulas. Boolean is a *combinator* — it consumes two upstream tessellations and must compute a topologically valid output that respects the spatial intersection of two arbitrary input meshes. This requires a CSG (Constructive Solid Geometry) algorithm; classic implementations are BSP-tree-based (Naylor 1990, Thibault & Naylor 1987) or use exact-arithmetic plane arrangements.

The choice is high-blast-radius. Phase 7.4 (`TopologyEvolution { Preserved, Split, Merged, Deleted, Reinterpreted }`) explicitly tests itself against Boolean operations because Boolean is where topology splits and merges most aggressively. Whatever data we get out of Boolean — triangle soup vs. labeled B-Rep faces — sets the upper bound on what Phase 7.2 (persistent topology IDs) and Phase 7.4 (lineage) can prove. PLAN §1.5.4.4 also commits us to a "CAD kernel non-equivalence doctrine": kernels are not interchangeable, and capability differences (notably "boolean robustness under tolerance") are surfaced explicitly rather than papered over. The Boolean dispatch is the first time this doctrine bites real implementation choices.

The four candidate paths are: (1) the `csgrs` pure-Rust CSG kernel, (2) the existing transitive `parry3d` dependency, (3) committing to the `truck` B-Rep CAD kernel for `cad-native`, or (4) hand-rolling ~1500 LoC of BSP-on-triangle-soup CSG inside `cad-core`. Each one trades a different combination of correctness, dependency footprint, topology-richness, and maintenance burden, and the ergonomics of the choice cascade into Phase 7.2 / 7.4 in ways that are hard to reverse later.

## Options considered

### Option 1 — `csgrs` crate (pure-Rust BSP-tree triangle-mesh CSG)

`csgrs` (`timschmidt/csgrs`, MIT) is a "multi-modal constructive solid geometry kernel in Rust" that performs union / difference / intersection / xor on polygon sets stored in BSP trees. It targets exactly the use case at hand — boolean ops on triangle / polygon meshes — and explicitly integrates with the Dimforge ecosystem (nalgebra, parry, rapier) which we already depend on transitively. Its `Mesh::triangulate()` exposes a triangle-soup output path that lines up with the workspace's `Tessellation` shape.

| Dimension | Assessment |
|---|---|
| Correctness | BSP-based union/diff/intersect/xor with documented coplanar-overlap handling. README explicitly lists "T-junction detection" in the TODO section — *not implemented yet* — which is a known fragility class for downstream rendering and Phase 7.4 lineage. Robustness on near-degenerate / non-watertight meshes is best-effort, typical of unbounded-arithmetic BSP. |
| Perf @ 1k tris each side | Interactive (single-digit ms class) — well within an editor frame. |
| Perf @ 10k tris each side | Comfortable for non-realtime authoring; not free, but well under the §13.2 "100 random parametric edits on 10 B-Rep entities" interactive budget. |
| Topology output | Polygon set with per-polygon plane equations and shared-vertex tracking; not B-Rep faces. Triangle-soup-equivalent for our purposes. Phase 7.4 face/edge identity must be reconstructed externally (e.g. plane-equality + lineage labels we attach pre-Boolean). |
| License | MIT. |
| Transitive deps | `nalgebra 0.34`, `geo 0.29`, `hashbrown 0.15`, `robust 1.1`, `thiserror 2.0`, `either 1.15`. Optional: `parry3d`, `rapier3d`, `earcutr`, `spade`, `chull`, `boolmesh`. nalgebra is *not* currently a direct workspace dep (rapier3d 0.32 migrated to glam math types per `versions.md`), so this re-introduces nalgebra to the workspace. |
| `forbid(unsafe_code)` | No explicit `forbid(unsafe_code)` declaration in the crate. README notes "Dependencies are 100% rust" but the crate itself does not pledge unsafe-forbidden. nalgebra and hashbrown both contain audited `unsafe`. Workspace policy is `unsafe_code = "forbid"` at the workspace root; pulling csgrs is fine (the lint applies only to *our* crates), but we cannot extend our forbid-pledge to its tree. |
| Last release | v0.20.1 (July 2025), 9 releases total, 969 commits on main, active development through 2025–2026. |
| Maintainer profile | Single maintainer (Timothy Schmidt / @timschmidt). High bus factor — typical of pure-Rust geometry crates. |
| API fit with cad-core `Tessellation` | Good. Build a csgrs `Mesh` from `(positions, indices)`, run boolean, call `.triangulate()`, harvest `(positions, indices)` back. One marshal in / one marshal out per operator evaluation. No nalgebra leakage in the cad-core surface API if we wrap it. |
| Determinism | BSP construction order depends on insertion order; csgrs uses `hashbrown` (deterministic seed-zero default). Output is deterministic *given identical input ordering*. Workspace `structural_hash` already drives input ordering deterministically via the operator graph, so this is reachable but requires test coverage to gate. Float epsilons inside csgrs are not configurable from outside. |

### Option 2 — `parry3d` (rapier physics-primitive library)

`parry3d` is the collision-detection / spatial-query layer under `rapier3d`. We depend on `rapier3d 0.32` directly per `Cargo.toml` and on `parry3d` transitively. It exposes shape primitives, distance queries, contact generation, convex-hull, approximate convex decomposition, and trimesh intersection *queries* — but not boolean *operations* on triangle meshes. The Slicer and meshlib threads in the literature confirm this limitation; the only reason `parry` shows up in Boolean conversations at all is because `csgrs` integrates with it for spatial acceleration, not because parry itself does the boolean.

| Dimension | Assessment |
|---|---|
| Correctness | Not applicable — parry does not perform mesh boolean operations. Building one on top of parry's BVH + intersection queries is essentially writing Option 4 (roll-our-own) using parry as a spatial accelerator. |
| Perf | n/a |
| Topology output | n/a |
| License | Apache 2.0 / MIT. |
| Transitive deps | Already transitive (rapier3d 0.32). No new dep weight if we used it. |
| `forbid(unsafe_code)` | No. Performance-critical math uses `unsafe` internally. |
| Last release | parry3d tracks rapier3d's release train; rapier3d 0.32 was Jan 2026. |
| Maintainer profile | Dimforge org (parry / rapier / nalgebra). Multi-maintainer, well-funded by physics-engine demand. |
| API fit with cad-core `Tessellation` | Indirect. Would require building a CSG layer on top — see Option 4. |
| Determinism | Documented determinism story for collision queries; CSG layer determinism would inherit our roll-our-own's choices. |

This option is effectively "Option 4 with parry as a spatial accelerator" and is not a standalone path. Listed for completeness because the dispatch brief asked.

### Option 3 — `truck` (B-Rep CAD kernel; PLAN-endorsed `cad-native` backend)

`truck` (`ricosjp/truck`, Apache 2.0) is the B-Rep CAD kernel PLAN §1.5.4 already names as `cad-native`'s implementation. It re-implements classical B-Rep + NURBS in Rust. The `truck-shapeops` sub-crate provides solid boolean operators (`and`, `or`, plus negation via complementation) on the kernel's `Solid` type, with topological healing, defeaturing, and shape repair. This is the highest-fidelity option: outputs are real B-Rep faces / edges / vertices, not triangle soup.

| Dimension | Assessment |
|---|---|
| Correctness | B-Rep boolean against B-Rep solids — preserves face identity natively, handles NURBS surfaces, robust under sewing/healing. The PLAN explicitly identifies "boolean robustness under tolerance" as a kernel-distinguishing capability (§1.5.4.4) and truck is positioned as the primary kernel choice. |
| Perf @ 1k tris each side | Tris is the wrong unit — truck operates on B-Rep solids (Faces × Edges × Vertices). For CAD-typical solids (~10s–100s of faces) it is interactive. Tessellation happens *after* boolean, so a Boolean→Tessellation pipeline is the right shape. |
| Perf @ 10k tris each side | Same — face count, not tri count, is the meaningful axis. |
| Topology output | Native B-Rep `Solid` with persistent face/edge/vertex IDs. Phase 7.2 / 7.4 hooks are *natural* here; lineage edges fall out of the kernel's history. This is structurally the best fit for the architecture's topology-lineage thesis. |
| License | Apache 2.0. |
| Transitive deps | `truck-shapeops` pulls `truck-base`, `truck-geometry`, `truck-topology`, `truck-meshalgo`, `truck-geotrait`, `derive_more 2.1`, `rustc-hash 2.1`, `itertools 0.14`. **No nalgebra, no glam** — uses `cgmath` internally via `truck-base`. Substantial new dep weight (5+ truck-* crates), but all from one org with one license. |
| `forbid(unsafe_code)` | Truck README emphasizes "safe implementation using Rust to eliminate core dumped"; not all sub-crates declare `forbid(unsafe_code)` explicitly, but the project's stated stance aligns. Typical truck deps (rustc-hash, itertools) contain audited unsafe. |
| Last release | truck-shapeops 0.4.x train, tracks the broader truck workspace; recent activity throughout 2025 per the repo's CHANGELOG. 73 git tags total — steady multi-year cadence. |
| Maintainer profile | `ricosjp` — Research Institute for Computational Science / RICOS Co. Ltd. (Japan). Commercial backing; not a single-maintainer hobby project. |
| API fit with cad-core `Tessellation` | **Misaligned at this exact moment.** Boolean *input* is `Solid`, not `Tessellation`. To make this work today, every upstream operator (`Cuboid`/`Extrude`/`Revolve`) would have to be rewritten to produce truck `Solid`s, OR we'd need a triangle-mesh→Solid conversion path that is itself non-trivial and lossy. PLAN §1.5.4 commits us to this eventually — but committing right now requires a Phase 7 pivot, not a Boolean dispatch. |
| Determinism | cgmath-based math; truck's CHANGELOG notes ongoing tolerance / healing tuning. Likely deterministic in practice but the workspace would need to gate it with golden hashes — same as csgrs. |

### Option 4 — Roll our own BSP-tree triangle-soup CSG (~1500 LoC budget)

A direct port / adaptation of the Naylor 1990 BSP CSG algorithm. The reference implementation, evanw/csg.js, is **538 LoC of JavaScript** total, with **~200–220 LoC** of pure BSP algorithm (`build`, `clipPolygons`, `clipTo`, `invert`, `allPolygons`, plus the `union`/`subtract`/`intersect` orchestration). Translating to idiomatic Rust with `glam` math, error types, and a workspace-style test suite typically inflates by 2.5–3.5×: ~600–750 LoC of algorithm, ~400–600 LoC of tests, ~200 LoC of plumbing/types. The 1500 LoC budget in the dispatch brief is a credible upper bound.

| Dimension | Assessment |
|---|---|
| Correctness | csg.js handles coplanar-overlap correctly per its README; T-junction handling and near-degenerate triangles remain hand-holding territory. In Rust we'd inherit the same algorithmic frontier as csgrs but with worse coverage at the start (csgrs has 5+ years of accumulated edge-case fixes; we'd have zero). Robustness gains would need to be earned, not inherited. |
| Perf @ 1k tris each side | Same algorithmic class as csgrs; likely 1.5–3× slower until tuned. |
| Perf @ 10k tris each side | Acceptable but unpolished. |
| Topology output | Triangle soup with per-polygon plane equations. Same as csgrs, no better. |
| License | Ours. |
| Transitive deps | Zero new — `glam` already in `gfx`, would be promoted to workspace dep (low cost). |
| `forbid(unsafe_code)` | Yes — fully under the workspace pledge. |
| Last release | n/a (we ship and own it). |
| Maintainer profile | RGE itself. |
| API fit with cad-core `Tessellation` | Perfect — we design directly against `Tessellation`. No marshaling. |
| Determinism | Perfect — every float epsilon, hash, sort key is ours; bit-exact reproducibility is achievable from day one. |

## Decision

**Choose Option 1 (`csgrs`).**

Two reasons drive this:

1. **Phase 7's risk is correctness and topology-lineage hooks, not dependency footprint.** PLAN §1.5.4.4 / IMPLEMENTATION.md §7 are explicit that the CAD pillar is the highest-secondary-risk surface and that "many architectures die here." Rolling our own CSG (Option 4) means burning the Phase 7 budget on re-deriving 5+ years of csgrs's accumulated edge-case fixes for coplanar polygons, near-degenerate triangles, and BSP construction tuning — fixes csgrs already ships. The brief's stated workspace bias of "fewer deps, more determinism" is real, but determinism is reachable with csgrs (its Cargo features cleanly separate the optional bevy/wasm/parry integrations from the BSP core; we depend on the minimum surface), and the dep cost is one direct crate, not five. Option 4 is correct on dependency footprint and wrong on opportunity cost.

2. **Truck (Option 3) is the right *eventual* answer, not the right *now* answer.** PLAN §1.5.4 commits us to truck for `cad-native` — eventually. But Phase 7.1 has already shipped four triangle-soup operators (Cuboid / Transform / Extrude / Revolve), and Phase 7.3's `cad-projection` is wired against `Tessellation`, not B-Rep solids. Switching to truck *as the Boolean dispatch* implicitly forces all four existing operators to migrate to truck `Solid` outputs in the same dispatch, plus a Tessellation-from-Solid conversion at the projection boundary. That is a Phase 7 pivot, not a Boolean implementation, and it would inflate the dispatch's blast radius from "one operator" to "the entire CAD substrate." We adopt truck deliberately later (own ADR, own dispatch, planned migration order) and let Boolean ship on the substrate that already exists.

What we are giving up by choosing csgrs:
- **Native B-Rep face/edge identity from Boolean output.** csgrs produces triangle soup; Phase 7.4's `TopologyEvolution` lineage must be reconstructed from plane-equality + pre-operator labels we attach in cad-core, not lifted from the kernel. This is an explicit Phase 7.4 design constraint that we accept now and will revisit when truck adoption lands.
- **Workspace `forbid(unsafe_code)` purity for the cad-core dep tree.** csgrs and its `nalgebra` dependency contain audited unsafe. Our forbid-pledge applies to crates *we author*, so this is consistent with policy, but it's a real signal — surfaced via `cargo-deny` and a periodic dep audit.
- **Marginal dep weight.** One extra direct dep, plus nalgebra coming back into the tree. Mitigated by csgrs's optional-feature flags (we disable `parallel`, `bevymesh`, `wasm`, `metaballs`, `sdf`, `offset`, all the format-IO features, etc., and pull only `mesh` + `f32`).

## Consequences

### Positive

- Boolean dispatch ships in one bounded sub-dispatch (operator + tests + structural_hash + cache integration) instead of a Phase 7 pivot.
- csgrs's existing edge-case coverage (coplanar overlaps, BSP-construction tuning) buys us correctness we'd pay months to re-derive.
- Triangle-soup output keeps the cad-projection / Tessellation contract unchanged — Phase 7.3 is unaffected.
- Per-feature dependency activation lets us pull a *minimal* csgrs surface (BSP + mesh, nothing else) — keeps build-time impact bounded.
- Determinism is reachable with deterministic input ordering, which we already have via `OperatorGraph` structural hashing.

### Negative / risks

- **Topology lineage hooks are weaker** than truck's would be. Phase 7.4 must label faces/edges in cad-core *before* feeding csgrs and reconstruct identity from plane equations on output. This is a real engineering tax for the lineage prototype.
- **Single-maintainer bus factor** on csgrs (Timothy Schmidt). Mitigated by MIT license (we can hard-fork) and by csgrs being well-bounded (~one crate, no plugin ecosystem).
- **Re-introduces nalgebra** to the workspace tree. Mostly cost-of-ecosystem; not a correctness risk but visible in `cargo-deny` and `versions.md`.
- **`forbid(unsafe_code)` no longer holds for the full cad-core dep tree** — only for cad-core itself. Surfaced explicitly in the failure-class declaration and `versions.md`.
- **T-junction handling is incomplete** in csgrs (TODO per the README). Downstream rendering and lineage may surface artifacts on adversarial inputs. Mitigated by validating fixtures in test gates.

### Mitigations

- **Capability surface declaration.** Per ADR-104, declare `boolean_robust_under_tolerance: false` and `deterministic_triangulation: best-effort` on the csgrs-backed Boolean operator until proven otherwise via the §13.2 / §13.6 gates. The non-equivalence doctrine carries the failure mode forward as a documented capability gap rather than a hidden bug.
- **Capability-gate test fixtures.** Land the Phase 7 §13.6 "1000 random parametric edits" determinism gate as a soak test the moment Boolean lands. Failure invokes the `snapshot-recoverable` failure-class path (§1.13) — Boolean failure rolls back the operation, surfaces a diagnostic, doesn't panic the editor.
- **Explicit truck-migration follow-up.** Open ADR-113 (placeholder) tracking "when does cad-core migrate to truck Solids" with concrete gates: e.g. when `cad-projection` needs B-Rep face IDs from Boolean output for Phase 7.4 lineage to make its §13.6 gate.

## Alternatives explicitly NOT chosen and why

**Option 2 (`parry3d`) is not a real option.** parry's intersection / contact / decomposition queries are *spatial accelerators*, not boolean operators. Choosing parry standalone for Boolean reduces to "build a CSG layer on parry's BVH" — which is a redescription of Option 4 (roll-our-own) using parry as the broad-phase. It carries Option 4's costs without the determinism guarantees Option 4's full ownership would give us. We already get parry transitively via rapier3d for physics; reusing it for Boolean would muddle the dependency story without paying for itself.

**Option 3 (`truck`) is not chosen *now* but is the intended *destination*.** The truck choice is made at the *cad-core kernel adoption* layer (§1.5.4.4 doctrine), not the *Boolean operator* layer. Adopting truck now forces a Phase 7 pivot — every existing operator (Cuboid / Transform / Extrude / Revolve) and the cad-projection bridge would change shape in the same dispatch — and that violates the dispatch boundary discipline this workspace runs on. The right time to adopt truck is a separately-scoped dispatch with its own ADR, after Phase 7.4 has surfaced what topology lineage actually needs from the kernel. csgrs is the right *bridge*; truck is the right *terminus*.

**Option 4 (roll-our-own ~1500 LoC BSP CSG) is structurally appealing and tactically wrong.** The 1500 LoC budget is credible (csg.js's 538 LoC ports to ~700 LoC of Rust algorithm with idiomatic types and ~600 LoC of tests). Determinism and `forbid(unsafe_code)` purity are real wins. But Phase 7's stated risk profile is "many architectures die here" — and the way they die is by spending the implementation budget on the wrong layer. Spending three weeks re-deriving csgrs's edge-case fixes when csgrs ships them under MIT is precisely the kind of misallocation IMPLEMENTATION.md §7 warns against. We revisit this *if* csgrs's licensing changes, *if* its bus-factor materializes (long stall + open issues), *or* *if* its determinism story falls short of §13.6. Until then, the buy-vs-build math points at csgrs.

## Implementation guidance for D-Boolean dispatch

- **API shape recommendation:**
  ```rust
  pub enum BooleanMode { Union, Intersection, Difference }
  pub struct BooleanOp { pub mode: BooleanMode }
  // Operator impl: arity = 2; lhs = inputs[0], rhs = inputs[1]
  // Edge wiring uses existing EdgeKind::Input(0) for lhs, Input(1) for rhs.
  ```
  No new `EdgeKind` variant needed — the existing `Input(port)` carries lhs/rhs ordering via port 0 / port 1, exactly as the operators/mod.rs doc-comment already anticipates ("Future operators with multiple ordered inputs (e.g. a Boolean union with `lhs=0` and `rhs=1`) reuse the same variant").

- **`OperatorNode` / `OpKind` extension:** add `OpKind::Boolean` discriminant and `OperatorNode::Boolean(BooleanOp)` variant. Extend `as_operator()` match arm. Match-exhaustiveness audit (per the D-Extrude / D-Revolve dispatches' discipline) should confirm zero downstream sites need updating beyond cad-core itself.

- **`structural_hash` recipe:**
  ```
  BLAKE3(b"boolean:" || mode_discriminant_u8 || nothing_else)
  ```
  No parameters beyond `mode` — csgrs's BSP construction does not expose tunables we'd want in the hash. Upstream lhs/rhs hashes are folded in by `OperatorGraph::evaluate`'s recursive `effective_hash` already (Phase 7.1 D-prime substrate), so the operator's *local* hash stays parameter-only.

- **Failure-class declaration in module:** `//! Failure class: snapshot-recoverable` on the new module file, matching cad-core's already-declared class. Map: csgrs panic / pathological-input failure → `OpError::EmptyResult` or `OpError::InvalidParameter("boolean failed: <diag>")` → cad-core's existing `begin_operation` + rollback machinery surfaces an undo entry per §1.13.

- **csgrs feature flags:** depend on csgrs with `default-features = false`, opt in only to `mesh` (and `f32` if it's a feature flag — confirm at integration time). Decline `parallel`, `bevymesh`, `wasm`, `metaballs`, `sdf`, `offset`, all `*-io` features. Revisit at next dep audit.

- **Test fixtures (mandatory before declaring D-Boolean done):**
  1. `cube ∪ cube_offset` — overlapping unit cubes shifted by `(0.5, 0.5, 0.5)`. Validates basic union shape (vertex-count bounds, watertight output).
  2. `cube ∩ cube_offset` — same inputs as above, intersection. Validates basic intersection.
  3. `cube – cube_offset` — both orderings; difference is non-commutative.
  4. `cube – sphere`, where sphere is approximated by a tessellated icosphere (`subdivisions = 2` is enough). Validates curved-surface vs. planar-surface boolean.
  5. **Coplanar-faces stress.** Two unit squares sharing the +Y face exactly. csg.js / csgrs explicitly handle this; our test gates it to lock the behavior in.
  6. **Non-watertight input handling.** Feed a deliberately-open mesh; assert `OpError::InvalidParameter` (or csgrs's diagnostic projected through `OpError`) — we explicitly *do not* claim boolean-on-open-meshes works.
  7. Determinism gate: same fixtures × 1000 iterations → `BLAKE3(positions || indices)` byte-identical across runs. Closes §13.2 / §13.6 gate.

- **Phase 7.2 / 7.4 hook:** csgrs returns triangle soup with no per-face identity. The Phase 7.2 / 7.4 design must:
  1. Label faces *upstream* of Boolean (Cuboid produces 6 named faces + 12 named edges; Extrude produces 2 caps + N side-walls; Revolve produces N side-rings; etc.).
  2. Pass labels through Boolean as per-triangle attributes (csgrs supports per-polygon metadata via its `Mesh` API — confirm at integration).
  3. Reconstruct lineage post-Boolean from plane-equation matching: surviving / split / merged / deleted classified by which input plane each output triangle's plane equation matches.
  4. Surface the reconstruction confidence via the `LineageEdge.confidence: f32` field already specified in §1.5.4.3.

- **Capability surface entry (per ADR-104):** declare csgrs-backed Boolean as
  ```
  KernelCapabilities {
      boolean_robust_under_tolerance: false,   // BSP, no exact arithmetic
      healing_strategies: {},                  // none
      nurbs_eval: NurbsEvalQuality::None,      // no NURBS
      step_round_trip_fidelity: StepFidelity::None,
      deterministic_triangulation: true,       // gated by §13.6 1000-iter test
  }
  ```
  This makes the gap explicit per the non-equivalence doctrine and ensures we don't accidentally claim truck-grade properties.

## Followups / open questions

- **ADR-113 (deferred):** "when does cad-core migrate to truck `Solid` as the operator-output type?" Concrete gate proposed: when Phase 7.4 lineage prototype demonstrates that plane-equation-based lineage reconstruction misses the §13.6 "1000 random parametric edits" target — at that point truck's native face-identity story becomes the Phase 7.4 unblocker. Tracked in HANDOFF.md. **STATUS: still deferred** — D-7.4 plane-only heuristic shipped 2026-05-07 with documented v0 limitations; D-7.4-followup csgrs metadata-passthrough closed the immediate Difference-vs-Split gap, so the truck-migration trigger has not yet fired.
- **csgrs metadata-passthrough verification:** confirm at D-Boolean implementation time whether csgrs's `Mesh` polygon metadata round-trips through Boolean (via `clipTo` / `invert` / `build`) intact. If it doesn't, plane-equation reconstruction is the only Phase 7.4 hook; if it does, we get cleaner identity. This is a 30-minute spike inside the D-Boolean dispatch. **RESOLVED 2026-05-06 in D-Boolean spike + 2026-05-07 in D-7.4-followup integration**: csgrs preserves per-polygon `Mesh<S>` metadata through Union/Intersection (clone through plane splits + clip_polygons); Difference retags rhs's clipped polygons with lhs's metadata (csgrs quirk; documented + special-cased in `infer_lineage_labeled`). End-to-end labeled path lands in `BooleanOp::evaluate_labeled` and `infer_lineage_labeled`.
- **T-junction policy:** csgrs's README lists T-junction handling as TODO. Decide at D-Boolean time whether we (a) accept T-junctions in output and document it, (b) post-process with our own welding pass, or (c) upstream a fix. Likely (a) for the first land, (b) if rendering surfaces visible artifacts on golden scenes. **STATUS: option (a) chosen by silence** — D-Boolean shipped 2026-05-06 without T-junction welding; no visible artifacts surfaced in test fixtures or D-7.4 integration smoke. Revisit when rendering surfaces visible artifacts on golden scenes (option b); upstream fix (option c) remains attractive long-term.
- **Determinism story for the §13.2 1000-edit gate:** csgrs's BSP construction order is deterministic given input ordering, but float-epsilon handling is not exposed. Need a soak test (probably new `cad-core/tests/cad_boolean_determinism.rs`) that runs the gate on every CI to detect regressions if csgrs internals change between versions. **PARTIALLY RESOLVED 2026-05-06**: `cad-core/tests/cad_boolean_determinism.rs` ships running 100 iterations × Union+Difference (200 total) per `cargo test` invocation, asserting BLAKE3(positions||indices) byte-identity across all iterations. CI wiring of the §13.6 1000-iter periodic-soak gate is deferred to a future CI integration dispatch (per Status.md "criterion benches not wired into CI" carry-over).
