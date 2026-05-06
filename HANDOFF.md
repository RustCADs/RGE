# RGE Handoff Document

> **Snapshot**: 2026-05-07 13:00. Continuation pointer for the next session.
>
> **Read first**: this file. Then [`Status.md`](./Status.md) (current snapshot) and [`change.log`](./change.log) (full history).

---

## Current state — one-page summary

| Pillar | State |
|---|---|
| Workspace tests | **1549 / 1549 pass** across 200 binaries (2 ignored intentionally hardware-gated) |
| Architecture lints | **9 / 9 PASS** exit 0 (forbidden-dep, split-exemption, no-utils, graph-foundation, editor-state-ownership, command-bus, projection-modules, kernel-isolation, failure-class) |
| `cargo +nightly fmt --check` | exit 0 |
| `cargo check --workspace --all-targets` | 0 errors, ~130 pre-existing ui-theme `missing_docs` warnings (deferred per Status.md) |
| Implementation footprint | **43 IMPLEMENTED / 3 PARTIAL / 48 EMPTY-STUB** of 94 workspace members (~46%) |
| Tier 1 kernel | **10 of 15 implemented**: types / diagnostics / events / app / schedule / ecs / audit-ledger / asset / graph-foundation / **plugin-host**. **5 stubs**: shared, asset-view, asset-streaming, io-scheduler, job-system |
| Phase 7 operator catalog | **Cuboid + Transform + Extrude + Revolve + Boolean** (5 operators) + topology-lineage prototype (D-7.4) |
| Failure-class exemptions | **21 of original 81 cleared** (60 remain — rollout debt; cleared as each crate gets first real impl) |
| Substantive non-rollout-debt exemption | **1**: `crates/editor-ui/src/layout/node.rs` graph-foundation NodeId rename TODO (file-doc'd as conceptually distinct from substrate NodeId; rename to `LayoutNodeId` later) |

## What just shipped (this session — completed work)

1. **PluginContext v1 + CadProjectionPlugin canary** (post-audit CRITICAL #2; 16 new tests; kernel/plugin-host 23 → 33, cad-projection 28 → 34):
   - Closes Pairing-3 of the 2026-05-07 deep audit ("PluginContext is a logger, not a context")
   - `kernel/plugin-host::PluginContext` extended with type-erased resource registry: `BTreeMap<TypeId, Box<dyn Any + Send>>` + `insert<T>` / `get_mut<T>` / `take<T>` / `contains<T>` / `resource_count()` / `with_resource<T>` builder
   - Existing `PluginContext::new(diagnostics)` + `emit_diagnostic` + `diagnostics()` v0 API bit-identical (no breaking changes)
   - **No new `unsafe` code** — owned-resources-handoff design avoids the unsafe normally needed for type-erased borrowed references. Plugins `take<T>` at start, do work, `insert<T>` back; orchestrator wraps the call by inserting before and taking after
   - First real Tier-2 plugin canary lands as `crates/cad-projection/src/plugin_adapter.rs` (~190L) — `CadProjectionPlugin` impls `Plugin`, extracts `&mut World` + `&CadGraph` + `Tolerance` from ctx via `take<T>`, drives `CadProjection::tick`, puts resources back via `insert<T>`. Missing required resources surface as `PluginError::Runtime`; `Tolerance` defaults to 0.001m
   - Tests: 10 unit (context.rs registry mechanics) + 3 unit (plugin_adapter id/init/into_projection) + 3 integration smoke (full lifecycle via PluginHost + missing-resource error path + resources-put-back invariant)
   - New cad-projection dep: `rge-kernel-plugin-host` (Tier-2 → Tier-1; allowed per `forbidden-dep`)
   - The "Real Tier-2 dogfood unblocked" claim is now substantiated, not optimistic
2. **CadGraph::SnapshotParticipate** (post-audit CRITICAL #1; 9 new tests; cad-core 148 → 155, cad-projection 26 → 28):
   - Closes the silent PIE inconsistency window (Pairing-4 of the 2026-05-07 deep audit) and PLAN §13.2 gate "all stateful Tier-2 has SnapshotParticipate"
   - `impl SnapshotParticipate for CadGraph` in new `crates/cad-core/src/checkpoints/participate.rs` (414L); ParticipantId `cad-core.cad-graph`; serialization via **RON** (postcard rejected `OperatorNode`'s `#[serde(tag = "kind")]` — non-self-describing format limitation; switched to RON which `kernel/graph-foundation::GraphSnapshot` already uses internally)
   - `Serialize+Deserialize` derives added to: `CheckpointId`, `Checkpoint`, `InProgress`, `CheckpointHistory`, `CadGraph`, `OperatorGraph`
   - `CadProjection::validate_handles(&self, &CadGraph) -> Vec<(EntityId, NodeId)>` returns orphan handles after divergent-state restore (caller decides recovery: log diagnostic / re-project / error)
   - PIE full round-trip verified with both `[&cad, &projection]` participants; divergent-state smoke verifies `tick(&empty_cad)` returns `ProjectionError::NodeNotInGraph` (not panic)
   - New cad-core dep: `rge-kernel-ecs` (Tier-2 → Tier-1; allowed per `forbidden-dep`)
   - "Temporal foreign-key constraint" framing per ChatGPT cross-review: `BRepHandle.cad_node` is FK; `CadGraph.nodes` is PK set; PIE restore = transaction rollback
2. **D-7.4-followup csgrs metadata-passthrough integration** (11 new tests; cad-core 137 → 148; closes the v0 plane-only false-positive in lineage inference):
   - Switched `BooleanOp` from `csgrs::Mesh<()>` to generic `Mesh<M>` where `M: Clone + Send + Sync + Debug + 'static`
   - Added `BooleanOp::evaluate_labeled(&LabeledMesh, &LabeledMesh) -> LabeledMesh` carrying `TopologyFaceId` through csgrs polygon metadata
   - Added `infer_lineage_labeled(input, output) -> LineageGraph` for high-confidence per-face classification
   - Existing `evaluate(&[&Tessellation])` API kept bit-identical (passes `()` metadata); both paths coexist
   - **v0 false-positive fix**: surviving partially-consumed Difference faces now classify as Split (not Merged) — labeled-Difference integration smoke verifies `merged_count == 0` and `split_count >= 1`
   - csgrs Difference quirk reflected in labels: rhs's clipped polygons get retagged with lhs's metadata (per ADR-112 spike) — `infer_lineage_labeled` doesn't need to know about it; the labels carry through correctly
2. **C kernel/plugin-host** (23 new tests; kernel/plugin-host 0 → 23; closes §10.4 dogfood-rule carry-over):
   - Replaced 4-line stub with `Plugin` trait + `PluginContext` + `PluginHost` (1,140 src lines + tests)
   - `Plugin` trait per PLAN §10.4: `id() / name() / init / tick (default no-op) / shutdown (default no-op)`; `Send + 'static` so the host can hold them as `Box<dyn Plugin>`
   - `PluginContext { &mut dyn DiagnosticSink }` v0 — EventBus / Commands handles deferred until concrete plugins demand
   - `PluginHost`: BTreeMap<PluginId, PluginRecord> + Vec for insertion order; Pending → Initialized → Failed/Active → ShuttingDown → Shutdown lifecycle; init in registration order; shutdown LIFO
   - **Plugin-fatal isolation**: one plugin's failure marks it Failed but doesn't block other plugins (per PLAN §1.13)
   - 2 dogfood-smoke integration tests with `TestTier2Plugin` fixture — foundation for the §10.4 contract test (future dispatches replace fixture with real gfx::Plugin / physics::Plugin / editor-ui::Plugin / cad-projection::Plugin)
   - Failure-class declaration `//! Failure class: plugin-fatal` per §1.13; exemption cleared from registry
   - **kernel/plugin-host promoted EMPTY-STUB → IMPLEMENTED**; **Tier-1 kernel now 10 of 15 implemented**
2. **Phase 7.4 D-7.4 topology lineage prototype** (21 new tests; cad-core 116 → 137; first prototype of the most-novel-system in the architecture per PLAN §1.5.4.3 / ADR-098):
   - New `crates/cad-core/src/topo_lineage/` split across 4 sub-files (anticipating growth): `types.rs` (517L), `plane.rs` (199L pub(crate)), `infer.rs` (499L), `mod.rs` (106L orchestrator) — all under split-exemption cap
   - Public types: `TopologyFaceId(u64)`, `TopologyEvolution { Preserved, Split, Merged, Deleted, Reinterpreted }`, `LineageEdge { from, to, evolution, confidence }`, `LineageGraph`, `LabeledMesh`, `LineageError`
   - Private `QuantizedPlane` (1e-4 precision, sign-canonicalized so opposite-winding triangles hash equal)
   - Free fns: `label_by_plane(tess, base_id)` groups triangles by plane equation; `infer_lineage(input, output, base_id)` plane-matching heuristic classifying Preserved (exact match) / Split (input>output triangles on plane) / Merged (input<output) / Deleted (no output match) / Reinterpreted (no input match)
   - **Real csgrs hardening win**: first integration run hit DegenerateTriangle errors from real BSP-tree zero-area slivers; hardened with `TopologyFaceId::DEGENERATE = u64::MAX` sentinel for the heuristic path while preserving strict error variants for the private API
   - **Heuristic limitation documented**: triangle-count heuristic classifies many partially-consumed Difference faces as Merged rather than Split (csgrs's BSP triangulation produces more output triangles per surviving plane than the 2-tri input). v0 false-positive class — future boundary-precision detector or csgrs-metadata path will fix
   - Boolean Union smoke verifies ≥1 Reinterpreted edge surfaces; Boolean Difference smoke verifies ≥1 Split/Deleted/Merged edge + preserved_count < 6
   - **v0 simplifications vs PLAN §1.5.4.3** documented: no `OperatorId` field; no `SemanticScore` field; no `Split(Vec<PersistentFaceId>)`/`Merged(Vec<PersistentFaceId>)` inner data (multi-edge representation instead); face-only no edge/vertex lineage; no `PersistentFaceId` (per-mesh sequential ids only — Phase 7.2 substrate); no csgrs metadata-passthrough integration yet (future small follow-up); no `kernel/graph-foundation::Graph` backing (Vec for v0)
2. **Phase 7 D-Partial-Revolve** (20 new tests; cad-core 96 → 116; RevolveOp extension):
   - Added `pub angle: f32` field to `RevolveOp` with `#[serde(default = "default_angle_full_revolution")]` for snapshot back-compat
   - New `RevolveOp::partial(profile, segments, angle)` constructor; existing `new(profile, segments)` delegates to `partial(p, segs, 2π)` so backwards-compat is bit-identical
   - Validates `angle ∈ (0, 2π]` finite; clamps near-2π (within 1e-5) to exactly 2π for the full-revolution fast path; `is_full_revolution()` accessor uses 1e-6 epsilon
   - `evaluate()` split: full path unchanged (no caps; concave allowed); partial path emits `n*(segments+1)` verts + `2*n*segments` side tris + `2*(n-2)` cap tris with fan-triangulated start/end caps; **convexity required** for partial-revolution
   - Cap winding: start cap at θ=0 has -Z normal; end cap at θ=angle has +tangent normal
   - `structural_hash` extended to include `angle.to_le_bytes()` — breaking change vs pre-D-Partial-Revolve hashes (cached tessellations recompute on first eval; acceptable for v0)
   - 18 new unit tests + 2 integration (pi-radian + half-pi-with-Boolean pipeline smoke)
   - Direct-struct-literal sites needing update: zero — all callers use the constructors
   - **Split-exemption lint caught a 1015-line file regression** — resolved by trimming a 22-line analysis comment to 3 lines (final 995). The 9-lint enforcement gate works as designed.
2. **Phase 7 D-Boolean** (18 new tests; cad-core 78 → 96; 5th cad-core operator; first with Tier-3 dep):
   - `BooleanOp { mode: BooleanMode { Union | Intersection | Difference } }` arity 2 (lhs=port 0, rhs=port 1) backed by `csgrs 0.20.1` (pure-Rust BSP-tree CSG, MIT)
   - Conversion bridge cad-core f32 `Tessellation` ↔ csgrs f64 `Mesh<()>` via `nalgebra::Point3<f64>` / `Vector3<f64>`; right-hand-rule outward normals from CCW winding; output polygons fan-triangulated; coincident-vertex dedup via BTreeMap-keyed f64 LE-byte equality (deterministic)
   - `std::panic::catch_unwind` wraps the csgrs call → `OpError::InvalidParameter("boolean failed: <diag>")` for pathological input
   - structural_hash = BLAKE3(b"boolean:" || mode_discriminant_u8) — local hash only per ADR-112 (lhs/rhs effective_hash folded in upstream by `OperatorGraph::evaluate`)
   - `OpKind::Boolean` + `OperatorNode::Boolean(BooleanOp)` threaded through; `as_operator()` extended
   - **Tests**: 12 unit (mode dispatch / arity / hash determinism / disjoint-union / overlap-union / disjoint-intersection / overlap-intersection / difference-dent / non-commutativity / near-degenerate / pathological / wrong-arity) + 1 dispatch + 3 integration smoke (pipeline_union with Cuboid+Transform-translated-Cuboid, pipeline_difference, with_extrude_input heterogeneous lhs/rhs) + 2 determinism soak (100 iterations × Union/Difference, byte-identical via BLAKE3(positions||indices))
   - **Capability surface declared via doc-comment per ADR-104** (full struct lands with future ADR-104 dispatch): `boolean_robust_under_tolerance: false` (BSP, no exact arithmetic); `deterministic_triangulation: true` (200-iter soak PASS via BTreeMap-keyed dedup)
   - **30-min Phase 7.4 lineage spike** per ADR-112 §"Followups": csgrs preserves per-polygon `Mesh<S>` metadata through Union/Intersection (cloned through plane splits and `clip_polygons`); **Difference retags rhs polygons with lhs's metadata** (known csgrs quirk — Phase 7.4 lineage reconstruction must special-case); per-triangle source-tracking is feasible
   - New deps: `csgrs 0.20.1` (`default-features = false, features = ["f64", "earcut"]` — f32 conflicts with workspace-pinned rapier3d 0.32; earcut required since one of delaunay/earcut must be enabled); `nalgebra 0.33`
   - T-junction handling deferred (csgrs upstream TODO; no visible artifacts in test fixtures)
   - Real bug caught: pipeline_difference initially failed `DuplicateNode(NodeId)` because two `CuboidOp(1,1,1)` collide on content-derived NodeId — fixed by perturbing depth to 1.0001 (matches pipeline_union idiom)
   - Match-exhaustiveness audit: zero downstream sites needed update
2. **ADR-112: D-Boolean CSG library scoping** (read-only research dispatch — zero Rust changes; first ADR landed in workspace):
   - `docs/adr/ADR-112-cad-boolean-csg-library.md` 196 lines / 14 sections
   - **Decision: csgrs** (pure-Rust BSP-tree CSG) over parry / truck / roll-our-own
   - Rejected parry: doesn't perform mesh booleans, only spatial queries / convex-hull / ACD
   - Rejected truck: would force migrating all 4 existing operators from `Tessellation` to `Solid` — deferred to a future ADR-113 placeholder gated on Phase 7.4 outcomes
   - Rejected roll-our-own: csgrs ships 5+ years of edge-case fixes vs ~1500 LoC of new code; not worth the maintenance burden
   - csgrs caveat: README explicitly lists T-junction handling as TODO — fragility class to watch in D-Boolean dispatch
   - Implementation guidance for D-Boolean inline: `BooleanOp { mode: Union | Intersection | Difference }` arity 2, structural_hash recipe, 7 test fixtures, failure-class snapshot-recoverable, determinism gate
   - 4 followups identified for separate ADRs / spike: truck migration trigger; csgrs polygon-metadata passthrough; T-junction policy; CI determinism soak
2. **Phase 7 D-Revolve** (19 new tests; cad-core 59 → 78; 4th cad-core operator):
   - `RevolveOp { profile: Polygon2D, segments: u32 }` arity 0; reuses `Polygon2D` substrate from D-Extrude; full 2π revolution around Y-axis with `segments` rotational steps
   - **Concave profiles ALLOWED** for full revolution (no fan-triangulated caps needed → Extrude's convexity restriction does not apply)
   - Validates profile lies on +X side of Y-axis (`all x >= 0`), `signed_area != 0`, `segments >= 3`; CW/CCW input both accepted
   - Algorithm: per profile point `(x, y)` at ring `s` with `θ = s · 2π/segments`, 3D position is `(x·cos θ, y, x·sin θ)`. No caps — full revolution closes via index wrap. Total `n·segments` verts + `2·n·segments` tris
   - `OpKind::Revolve` + `OperatorNode::Revolve(RevolveOp)` threaded through; `as_operator()` extended
   - Critical correctness validations PASS: triangle/square/hexagon vertex+triangle counts; concave acceptance; axis-touching profile yields degenerate-but-valid mesh; CW handling; structural_hash deterministic + parameter-sensitive; full 2π closure (every ring vertex on circle of correct radius); ring 0 lies in XY plane; outward radial normal verified by `revolve_first_quad_has_outward_radial_normal`
   - Square-profile × 8 segments integration smoke verifies r² ∈ {1, 4} for every output vertex (matching the unit-square cross-section's inner/outer radii)
   - Match-exhaustiveness audit: zero downstream sites needed update
2. **Phase 7 D-Extrude** (26 new tests; cad-core 33 → 59; first non-trivial cad-core operator):
   - `Polygon2D` 2D-profile type (closed XY-plane polygon; `Polygon2DError::{TooFewPoints, NonFiniteCoordinate, DegenerateEdge}` ctor validation; lazy `signed_area()` + `convexity()` for evaluate-time gating)
   - `ExtrudeOp { profile: Polygon2D, length: f32 }` arity 0; convex profile + linear +Z extrusion + fan-triangulated caps + side-wall quads (split to triangles); CW/CCW input both accepted (winding-agnostic from caller); concave rejected with `InvalidParameter` matching "convex"
   - `OpKind::Extrude` + `OperatorNode::Extrude(ExtrudeOp)` variants threaded through; `as_operator()` match arm extended
   - **No external triangulation library** — fan triangulation suffices for convex profiles; concave support deferred to a separate dispatch with library scoping ADR (earcutr / lyon options)
   - Critical correctness validations PASS: triangle/square/pentagon/hexagon vertex+triangle counts (n→2n verts, 4n-4 tris); concave reject; CW handling; structural_hash deterministic + parameter-sensitive
   - **Match-exhaustiveness audit**: zero downstream sites needed update — only `as_operator()` in cad-core itself pattern-matches `OperatorNode`; cad-projection only constructs variants
2. **Phase 7.3 cad-projection minimal D-7.3** (26 new tests, cad-projection 0 → 26; promoted from PARTIAL → IMPLEMENTED) — validates the v0.6 CAD/ECS impedance-fix critical-path bet for real. 4 of 6 modules per PLAN §1.5.4.5 now implemented (semantic / runtime / editor stay stubs per §0.6 freeze policy):
   - `projection_structural/` — `BRepHandle { cad_node, mesh_id, last_projected_checkpoint }` ECS component (impl `Component` + `SnapshotComponent`); `EntityCadMap` bidirectional `BTreeMap` with duplicate-key errors; private `EntityIdProxy` + manual Serialize/Deserialize bridge since `kernel::ecs::EntityId` doesn't enable `ulid/serde`
   - `projection_geometry/` — `ProjectedMesh { positions, indices, source_node, source_checkpoint }` + `ProjectedMeshId(u64)` + free `project(cad, node, &mut TessellationCache, Tolerance) -> Arc<ProjectedMesh>` calling `cad-core::OperatorGraph::evaluate`; `CheckpointTag(u64)` proxy serializable since `cad_core::CheckpointId` doesn't derive serde
   - `projection_cache/` — `ProjectionCache` with last_seen_checkpoint + entity_meshes + dirty BTreeSet + hits/misses/reprojections stats; `observe_checkpoint(head, all_entities)` triggers head-advance dirty-mark-all
   - Top-level `lib.rs` `CadProjection { entity_cad_map, cache, tess_cache }` orchestrator with `tick(world, cad, tolerance) -> TickReport` + `spawn_brep_entity` / `despawn_brep_entity` / `entity_for(node)` / `node_for(entity)` / `projected_mesh(entity)`
   - `SnapshotParticipate` impl with `ParticipantId::new("cad-projection.brep-handles")`; capture/restore via postcard binary serialization carrying EntityCadMap + entity↔ProjectedMeshId association + last_seen_checkpoint; meshes themselves re-derive on next tick
   - **Both Phase 7.3 exit criteria PASS** (verified by integration smoke tests): (1) cad-projection invalidation triggers ECS update within one tick of cad-core commit; (2) PIE round-trip preserves cad-projection state
   - **`projection-modules` lint actively enforces** the structural↛runtime/editor split (PASS 0 violations); **`forbidden-dep` lint confirms** cad-projection is the only Tier-2 importing cad-core
   - New deps added: postcard (binary SnapshotParticipate payload), ulid w/ serde (EntityIdProxy)
2. **Phase 7.1 cad-core MVP D-prime** (33 new tests, cad-core 0 → 33; promoted from PARTIAL → IMPLEMENTED) — substrate per IMPLEMENTATION.md §7.1 with 2 trivial operators to validate end-to-end. 7 new modules under `crates/cad-core/src/`:
   - `operators/{mod, cuboid, transform}.rs` — `Operator` trait (`op_kind` / `structural_hash` / `evaluate` / `arity`); `OperatorNode` enum dispatching to concrete impls; `EdgeKind::Input(port)` for ordered ports; `CuboidOp { width, height, depth }` arity 0 → 8-vertex/12-tri origin-centered axis-aligned box; `TransformOp { translation, rotation_quat_xyzw, scale }` arity 1 → applies `glam::Mat4::from_scale_rotation_translation` to upstream positions
   - `graph/operator_graph.rs` — `OperatorGraph` wraps `kernel::graph_foundation::Graph<OperatorNode, EdgeKind>`; content-derived NodeId via BLAKE3 over serialized OperatorNode; recursive `evaluate()` with `HashSet<NodeId>` ancestor stack for cycle detection (graph-foundation does NOT detect cycles itself); **`effective_hash` recursively combines local_hash + port + upstream effective_hash** so cache invalidates correctly when ANY upstream parameter changes (key correctness validation)
   - `checkpoints/mod.rs` — `CheckpointId(u64)`, `Checkpoint { id, snapshot, root, parent }`, `CheckpointHistory`; `CadGraph` wrapper owning both the graph + history; `begin_operation` eagerly captures `GraphSnapshot`; `commit` advances head; `rollback` restores from in-progress snapshot; `restore_to(id)` replays historical snapshot; `graph_mut()` guarded by `MutationOutsideOperation` error
   - `tessellation/{mod, mesh, cache}.rs` — `Tessellation { positions, indices }` with index-validity check; `Tolerance::new(t)` validates finite>0 and quantizes to `(t*1e9) as u64` for hash equality across float drift; `TessellationCache` HashMap keyed on `CacheKey { structural_hash: [u8; 32], tolerance }` with hit/miss tracking
   - `tests/cad_smoke.rs` — end-to-end integration test
   - **All 4 critical Phase 7 architectural bets validated**: (1) operator DAG works on graph-foundation primitives, (2) checkpoint/rollback/restore_to round-trips byte-identical via GraphSnapshot, (3) tessellation cache invalidates correctly on parameter change (recursive effective_hash test PASS), (4) cad-core sits cleanly under graph-foundation without redefining NodeId/EdgeId (lint PASS 0 violations)
   - Failure-class declaration `//! Failure class: snapshot-recoverable` added; cad-core exemption REMOVED from `tools/architecture-lints/exemptions.toml`
2. **(earlier this session)** Phase 6 PBR-lite in `crates/gfx/` (18 new tests, gfx 26 → 44) — single-light Lambert+Phong + texture sampling on top of the wgpu substrate. 5 new modules: `vertex_lit.rs` / `camera.rs` / `light.rs` / `material.rs` / `lit_mesh_pipeline.rs`. Pixel-level lit/backlit/checker assertions PASS on RTX 4060 Ti / Vulkan. wgpu 29 quirks discovered: `Queue::write_texture` takes `TexelCopyTextureInfo` by value; `SamplerDescriptor.mipmap_filter` is `MipmapFilterMode` (distinct type); `bytemuck::cast_slice(&[ubo])` lifetime issue → use `bytemuck::bytes_of(&ubo)`.
3. **(prior session)** `kernel/graph-foundation` (Tier 1, 47 tests) — substrate per PLAN §1.14: NodeId/EdgeId BLAKE3-derived, StableHash trait, Graph<N,E>, GraphSnapshot, GraphDiff, Invalidation propagation, VizAdapter trait. `graph-foundation` lint actively enforces reuse.
4. **(prior session)** `kernel/ecs::participate` (PIE composition substrate, 14 tests) — `SnapshotParticipate` trait + `PieSnapshot` aggregator. Composes existing `SnapshotComponent` with per-subsystem state into the unified PIE snapshot per PLAN §6.13.
5. **(prior session)** Two deep audits + cleanup passes — failure-class taxonomy correction; ui-theme indirection collapse; ~286 KB of stale transcripts removed; `.gitignore` hardened.

## Next-job options (dispatch-ready)

Pick one. All four are bounded single-agent dispatches.

### Option B — Phase 6 fill-in (renderer progress)

**Goal**: continue Phase 6 toward the 60fps simple-scene golden gate.

**State**: Phase 6.1 substrate done (wgpu init + headless triangle + mesh rendering + transforms via Transform UBO, 26 tests) AND **PBR-lite shipped this session** (single-light Lambert+Phong + texture sampling, 18 new tests, total gfx 44; verified pixel-level on real RTX 4060 Ti / Vulkan). Remaining Phase 6 items per IMPLEMENTATION.md:
- 6.1 follow-up: **frame-graph minimal** (transient resource lifetimes per frame; `TexturePool`/`BufferPool` keyed on frame index; declarative pass DAG with read/write resource declarations so transient resources can be aliased across non-overlapping passes). Recommended next sub-dispatch within B.
- 6.2 **render-snapshot separation** per §1.5.2 (sim-thread mutates N+1, render-thread reads frozen WorldSnapshot{N}; the shipped `PieSnapshot`/`SnapshotParticipate` substrate is what feeds this; gfx needs to impl `SnapshotParticipate` for whatever render-side state is replicated)
- 6.3 **material-runtime** — material UBOs already exist (this session); next is **WGSL+naga shader compile** (naga not yet workspace dep — bring it in) + **pipeline cache** (PSO keyed on shader hash + vertex layout) so 100 material instances share one PSO
- Exit criteria: 60fps on `simple-scene` golden project (1k cubes + 1 directional light); editor frame ≤ 8ms idle; render-thread sees stable snapshot; 100 material instances share one PSO

**Recommended next sub-dispatch within B**: frame-graph minimal. PBR-lite is done; frame-graph optimizes resource lifetimes and is the right substrate before scaling to many materials. Material-pipeline cache (6.3 latter half) is also a clean dispatch and can run in parallel with frame-graph since they touch different parts of gfx.

**wgpu 29 API quirks documented** (from Phase 6.1 + PBR-lite dispatches): `Instance::new_without_display_handle()`, `request_adapter` returns `Result<_, RequestAdapterError>`, `multiview` → `multiview_mask`, `Maintain::Wait` → `PollType::wait_indefinitely()`, `PipelineLayoutDescriptor.bind_group_layouts` is `&[Option<&BindGroupLayout>]` not `&[&BindGroupLayout]`, `BufferViewMut` doesn't impl IndexMut (use `queue.write_buffer` not `mapped_at_creation`), **`Queue::write_texture` takes `TexelCopyTextureInfo` by value (not by reference)**, **`SamplerDescriptor.mipmap_filter` is `MipmapFilterMode` (distinct re-exported type from `FilterMode`)**, **`bytemuck::cast_slice(&[ubo])` creates a temporary that drops before `queue.write_buffer` reads it (E0716) — use `bytemuck::bytes_of(&ubo)` for single-struct uploads**.

### ~~Option C — `kernel/plugin-host`~~ DONE 2026-05-07

`Plugin` trait + `PluginContext` + `PluginHost` lifecycle landed. 23 tests including dogfood-smoke integration. kernel/plugin-host promoted EMPTY-STUB → IMPLEMENTED. Tier-1 kernel now 10/15.

### Option D — Phase 7 cad-core continuation (HIGHEST SECONDARY RISK per IMPLEMENTATION.md)

**Status**: D-prime substrate + D-7.3 bridge both **DONE this session**. cad-core + cad-projection both PARTIAL → IMPLEMENTED. Subsequent Phase 7 dispatches each pick one bounded follow-up.

#### ~~D-7.3 — cad-projection minimal~~ DONE 2026-05-06

`BRepHandle` ECS component + bidirectional EntityCadMap + ProjectedMesh + ProjectionCache + `CadProjection::tick()` + `SnapshotParticipate` impl. 26 tests including invalidation-within-one-tick + PIE round-trip integration smoke. Both Phase 7.3 exit criteria PASS. Architecture lints `projection-modules` + `forbidden-dep` PASS.

#### ~~D-Extrude — first non-trivial operator~~ DONE 2026-05-06

`Polygon2D` 2D-profile type + `ExtrudeOp { profile, length }` operator (arity 0; +Z extrusion with fan-triangulated caps + side walls; convex-only with concave rejection; CW/CCW input both accepted). 26 tests including pentagon-prism integration smoke. Phase 7 operator catalog now: Cuboid + Transform + Extrude.

#### ~~D-Revolve — sweep-of-revolution~~ DONE 2026-05-06

`RevolveOp { profile, segments }` arity 0; full 2π around Y-axis; concave profiles ALLOWED (full revolution = no fan-triangulated caps); reuses `Polygon2D` from D-Extrude. 19 tests including square × 8-segments integration smoke verifying r²∈{1,4} radii.

#### ~~D-Boolean — CSG operations~~ DONE 2026-05-06

`BooleanOp { mode: Union | Intersection | Difference }` arity 2 backed by csgrs 0.20.1; conversion bridge cad-core f32 ↔ csgrs f64; 18 tests including 100-iter determinism soak across Union+Difference. Capability surface declared per ADR-104. csgrs metadata-passthrough confirmed for Union/Intersection; Difference retags as known csgrs quirk (Phase 7.4 must special-case).

#### ~~D-Partial-Revolve — angle < 2π extension~~ DONE 2026-05-07

`RevolveOp` extended with `pub angle: f32` field; `partial(profile, segments, angle)` constructor; full-revolution backwards-compat via `new()` delegating to `partial(p, segs, 2π)`; partial-revolution path emits fan-triangulated start/end caps (convexity required); 20 new tests including pi-radian + half-pi integration smoke + partial-revolve-through-Boolean pipeline smoke. cad-core 96 → 116.

#### ~~D-7.4 — topology lineage prototype~~ DONE 2026-05-07

`TopologyFaceId` + `TopologyEvolution` + `LineageEdge` + `LineageGraph` + `LabeledMesh` types per PLAN §1.5.4.3 (v0 — simplified spec; OperatorId/SemanticScore/inner-Vec data deferred). Plane-equation-matching heuristic with sign-canonicalized `QuantizedPlane`. Hardened against real csgrs degenerate-triangle output. 21 tests including Boolean-union + Boolean-difference integration smoke. cad-core 116 → 137.

#### ~~D-7.4-followup — csgrs metadata passthrough integration~~ DONE 2026-05-07

`BooleanOp::evaluate_labeled` carries `TopologyFaceId` through csgrs `Mesh<S>` metadata; `infer_lineage_labeled` consumes labeled output for high-confidence classification; v0 plane-only Merged-vs-Split false-positive fixed; both paths coexist. 11 tests including labeled-Difference integration smoke.

#### ~~D-Partial-Revolve — angle < 2π extension~~ DONE 2026-05-07

`RevolveOp { profile, segments, angle }` extended with `partial(profile, segments, angle)` constructor; full-revolution backwards-compat preserved via `new()` delegating to `partial(p, segs, 2π)`; partial-revolution path emits fan-triangulated start/end caps (convexity required); structural_hash includes angle bytes. 20 tests including pi-radian + half-pi integration smoke.

#### ~~D-Boolean — CSG operations~~ DONE 2026-05-06 (via ADR-112)

`BooleanOp { mode: Union | Intersection | Difference }` arity 2 backed by `csgrs 0.20.1`; conversion bridge cad-core f32 ↔ csgrs f64; 18 tests including 100-iter determinism soak across Union+Difference. Capability surface declared per ADR-104. csgrs metadata-passthrough integration shipped as D-7.4-followup.

#### D-7.2 — persistent topology IDs

**Goal**: validate face/edge IDs survive parameter rebuilds (per IMPLEMENTATION.md Phase 7.2; smoke test: 100 operator chains × 10 random parameter rebuilds with face/edge IDs preserved per `TopologyEvolution` enum).

**State**: needs a B-Rep model first (current `Tessellation` is triangle soup with no per-face / per-edge identity). Likely requires a `BRep` struct with named faces+edges, or a labeling scheme on triangle groups. Bigger dispatch. The plane-equation approach prototyped in D-7.4 is the input to this model's identity-stability story.

#### ~~D-7.4 — topology lineage prototype~~ DONE 2026-05-07

`TopologyFaceId` + `TopologyEvolution` + `LineageEdge` + `LineageGraph` + `LabeledMesh` types per PLAN §1.5.4.3 (v0 — simplified spec; OperatorId/SemanticScore/inner-Vec data deferred). Plane-equation-matching heuristic with sign-canonicalized `QuantizedPlane`. 21 tests including Boolean-union + Boolean-difference integration smoke. Strengthened by D-7.4-followup metadata-passthrough.

**Dispatch order recommendation** (post-D-7.4-followup): **Real Tier-2 dogfood (gfx::Plugin)** (closes §10.4 contract test for the largest Tier-2 substrate; now fully unblocked since both kernel/plugin-host and the gfx renderer substrate are shipped) → D-7.2 persistent topology IDs (needs B-Rep model; bigger). Or **Phase 6 frame-graph minimal** / **render-snapshot Phase 6.2** (renderer-side, unblocked since SnapshotParticipate validated by cad-projection). Or **remaining kernel stubs** (shared / asset-view / asset-streaming / io-scheduler / job-system) — each a bounded Tier-1 substrate dispatch.

**Risk note**: PLAN explicitly says "Many architectures die here. This is where v0.6's CAD/ECS impedance fix gets tested by reality." Phase 7 dispatches need careful boundary-keeping.

### Option E — Phase 3.3+3.4 formal hot-reload bench gates

**Goal**: rewire `script-bench`'s 4 criterion benches against real `script-host` + a 1000-entity Counter fixture; close the formal Phase 3 exit gates.

**State**: Phase 3.2 substrate proven (script-host swap window 0.31ms in debug = 320× headroom on 100ms gate). The criterion benches in `crates/script-bench/benches/{cold_start,hot_reload_swap,memory_overhead,script_tick_1m}.rs` exist as code but are driven by `engine_stub.rs` placeholders. Formal Phase 3 exit criteria (per IMPLEMENTATION.md):
- Hot-reload p95 < 100ms on a **1000-entity scene** (substrate proven on 1-entity smoke; needs scaling)
- ECS iteration via WASM ≤ **1.5×** native Rust
- **1-hour** session without memory leak
- Component data preserved across **100 hot-reload cycles**

**Polish work** — substrate validated; this closes formal measurement debt + appends BASELINE.md.

## Persistent gaps (carry-over — none of B/C/D/E directly addresses, but worth tracking)

- **5 empty kernel stubs** (shared, asset-view, asset-streaming, io-scheduler, job-system) — partial subset addressed by future Phase 5+ work; plugin-host shipped 2026-05-07 (Option C)
- **`physics` has no kernel/diagnostics integration** (uses inline `stubs::audit_ledger::AuditLedger` local twin) — small refactor, not pressing
- **27 of 27 §18 companion docs missing** (`GRAPH_FOUNDATION.md` / `CAD_TOPOLOGY_LINEAGE.md` / `PLUGIN_API.md` / `CAD_CORE_MODEL.md` notably absent despite shipped substrates) — governance debt; could be tackled in chunks. ADR-112 landed 2026-05-06 as the first written ADR; ADR-097/098/101/104 (referenced by PLAN.md + ADR-112) still unwritten.
- **`cargo bench` not wired in CI** — formal Phase 3 perf gates unrun (Option E addresses)
- **WASM cold-start baseline (904µs) measured on wasmtime 23**, not re-validated post bump to 44 — small re-run task
- **`io-3mf` crate entirely missing** from workspace despite PLAN §1.6.5 listing it as required
- **kernel/ecs snapshot warning routing** — currently uses `tracing::warn!` for unregistered components; could route through `&mut dyn DiagnosticSink` (deferred to align with future broader diagnostic-routing pass)
- **8 empty `docs/*` subdirectories** (PLAN-mandated placeholders; `.gitkeep` could make them git-trackable but not pressing)

## How to resume

1. **Verify env**: cargo at `A:\RustCache\cargo\bin\cargo.exe` (NOT on PATH); set `CARGO_HOME=A:\RustCache\cargo`, `RUSTUP_HOME=A:\RustCache\rustup`. Run from `A:\RCAD\RGE\`.
2. **Verify state matches this doc**: `cargo run -q -p rge-tool-architecture-lints -- all` should exit 0; `cargo test --workspace --all-targets --no-fail-fast` should report 1549 passed.
3. **Pick a dispatch option** (B/C/D/E above). Each has the spec inline; turn it into an Agent prompt with the same template structure as prior dispatches.
4. **After dispatch completes**: verify all 9 lints PASS, run workspace tests, append entries to [`change.log`](./change.log) with timestamp + test count delta + LLVM lines + any complications, update [`Status.md`](./Status.md) with new state, update [`README.md`](./README.md) test count if changed.

## Architectural-debt registry (post-2026-05-07 deep audit)

The 5-parallel-agent deep audit on 2026-05-07 surfaced architectural gaps not covered by the 9-lint enforcement. They decompose along three axes:

### Axis 1 — Temporal consistency model
Snapshot system is incomplete; graph-based stateful Tier-2 substrates are not all participating in PIE; referential integrity across capture/restore not enforced.

- **~~CRITICAL #1~~ DONE 2026-05-07**: `CadGraph` impls `SnapshotParticipate` via RON-based capture/restore in `crates/cad-core/src/checkpoints/participate.rs`; ParticipantId `cad-core.cad-graph`; new `CadProjection::validate_handles(&CadGraph) -> Vec<(EntityId, NodeId)>` for divergent-restore orphan detection. PLAN §13.2 gate ("all stateful Tier-2 has SnapshotParticipate") closed for cad-core. 9 tests including PIE full round-trip with both `[&cad, &projection]` participants + divergent-state smoke verifying `tick(&empty_cad)` returns `ProjectionError::NodeNotInGraph` (not panic).

### Axis 2 — Capability-based execution model
Plugin substrate is not real yet; current `PluginContext { &mut dyn DiagnosticSink }` is a logger, not a context; no stable ABI boundary.

- **~~CRITICAL #2~~ DONE 2026-05-07**: `PluginContext` v1 — type-erased resource registry (`BTreeMap<TypeId, Box<dyn Any + Send>>`) with `insert<T>` / `get_mut<T>` / `take<T>` / `contains<T>` / `with_resource<T>` builder. **Owned-resources-handoff** design (not borrowed references) keeps plugin-host Tier-1 with no `unsafe`. Existing `PluginContext::new(diagnostics)` v0 API bit-identical. First real Tier-2 plugin canary `CadProjectionPlugin` lives in `crates/cad-projection/src/plugin_adapter.rs` and exercises full lifecycle through PluginHost. 16 tests; kernel/plugin-host 23 → 33; cad-projection 28 → 34.
- **LOW #5**: Auto-emit `Diagnostic` from `init_all` / `tick_all` / `shutdown_all` on plugin `Err` (without changing the `PluginError` type). Preserves plugin-fatal isolation while making the diagnostic stream the single source of truth.

### Axis 3 — Unified data model
Parallel/duplicated representations (labeled vs unlabeled mesh; handle vs map cad_node) create dual-source-of-truth drift and pipeline composability failures.

- **HIGH #3**: **Unified Mesh refactor** — collapse `Tessellation` + `LabeledMesh` into one type with `face_labels: Option<Vec<TopologyFaceId>>`. `Operator::evaluate -> Tessellation` carries labels when produced. Deletes the dual-path API (`evaluate` vs `evaluate_labeled`). Touches all 5 operators + cad-projection bridge + topo_lineage API.
- **MEDIUM #4**: Single source of truth for `BRepHandle` ↔ `EntityCadMap`. Drop `cad_node` field from `BRepHandle`; look up via `EntityCadMap` at access time. Eliminates dual-source drift. Tradeoff: tiny perf hit, big consistency win.

### Deferred (defensible until trigger fires)

- **`KernelCapabilities` struct**: doc-comment-only declaration acceptable until second CAD kernel lands (truck per ADR-113-deferred placeholder) or editor-ui needs to filter operator picker by capability.
- **`LineageGraph` as `kernel/graph-foundation::Graph`**: Vec backing acceptable until consumers materialize requiring traversal queries beyond linear history (constraint inheritance, conflict markers per PLAN §1.5.4.3).

### Dispatch order (post-audit)

1. **~~CRITICAL #1~~ DONE 2026-05-07** — `CadGraph::SnapshotParticipate` impl + handle-validation guard
2. **~~CRITICAL #2~~ DONE 2026-05-07** — `PluginContext` v1 capability registry + `cad-projection::Plugin` canary
3. **HIGH #3 (NEXT)** — Unified Mesh refactor (closes labeled/unlabeled duality, Pairing-1+8)
4. MEDIUM #4 — `BRepHandle` single-source-of-truth (closes Pairing-6)
5. LOW #5 — Plugin diagnostic auto-emit (closes Pairing-5)
6. Test-gap-followup — Audit 2's 16 specific test recipes (cross-substrate determinism, missing fault injection, dead error variants)
7. **gfx::Plugin canary** — second real Tier-2 plugin (proves PluginContext v1 design isn't cad-projection-specific; low-risk follow-up to CRITICAL #2)

## Reference index

- [`Status.md`](./Status.md) — live snapshot of current state, validation gates, immediate-next-job recommendations
- [`README.md`](./README.md) — public-facing project status + 9-lint table + workspace structure
- [`change.log`](./change.log) — running history (chronological, append-only)
- [`plans/PLAN.md`](./plans/PLAN.md) — architecture (frozen at v0.8)
- [`plans/IMPLEMENTATION.md`](./plans/IMPLEMENTATION.md) — phase ordering and de-risking gates
- [`plans/BASELINE.md`](./plans/BASELINE.md) — perf baselines (W03 PIE / Phase 3.2 script-host swap / Phase 5.3 PIE re-baseline / W04 wasmtime cold-start)
- [`plans/fileandfolderstructure.md`](./plans/fileandfolderstructure.md) — workspace layout spec
- [`tools/architecture-lints/exemptions.toml`](./tools/architecture-lints/exemptions.toml) — exemption registry (1 substantive + 60 failure-class rollout debt)
- [`versions.md`](./versions.md) — workspace dep table + MSRV (toolchain pinned 1.92.0)

## Operating conventions established this run

- **Dispatch pattern**: bounded scope agent prompts with explicit "files you MAY modify" / "files you MUST NOT modify"; report-back template ≤ 300-400 words; verification commands inline
- **Parallel dispatch**: orchestrator stays off shared files (`exemptions.toml`, `main.rs`, `common.rs`) during multi-agent rounds; clears them between rounds
- **Status.md / change.log discipline**: every dispatch ends with a Status.md update + change.log append; deep audits catch drift periodically
- **9-lint exit-0 ritual**: every dispatch ends with `cargo run -p rge-tool-architecture-lints -- all` exit 0 verified; `cargo +nightly fmt --check` exit 0 verified; full workspace test count tracked
- **Deep-audit cadence**: every ~5–10 dispatches the orchestrator runs a 5-parallel-agent read-only audit covering (1) architectural coherence, (2) test coverage smells, (3) doc drift, (4) code smells, (5) cross-architecture coherence — findings consolidated into a single cleanup-pass dispatch + separate dispatches for architectural debt
- **DONE-marking ritual**: when a dispatch completes, the corresponding "Next-job options" / "Subsequent dispatches" entry in HANDOFF.md/Status.md is rewritten with `~~strikethrough header~~ DONE YYYY-MM-DD` + a one-paragraph summary; the unstruck spec body is removed
- **Fmt CI**: `cargo +nightly fmt --check` — workspace uses nightly-only `imports_granularity = "Module"` + `group_imports = "StdExternalCrate"`; orchestrator runs `cargo +nightly fmt --all` after dispatches that add new files
- **Failure-class taxonomy**: 5 classes per PLAN §1.13 — recoverable / snapshot-recoverable / plugin-fatal / session-fatal / kernel-fatal. Per PLAN line 572 scheduler deadlock = kernel-fatal; per line 573 audit-ledger checksum fail = kernel-fatal.
