//! Phase 7 §7.3 `cad-projection` minimal — gate-closure test.
//!
//! Mirrors the §7.2 gate-closure idiom (`crates/cad-core/tests/
//! phase_7_2_gate_closure.rs`). Proves the existing `cad-projection`
//! substrate (D-7.3, shipped 2026-05-06) satisfies the §7.3 minimal
//! exit criterion: `BRepHandle` / `EntityCadMap` / `ProjectedMesh` /
//! invalidation-on-commit / triangle-fallback-always-available.
//!
//! # Mechanism
//!
//! * Seeded `xorshift64` PRNG (zero-dep, same idiom as §7.2).
//! * 10 `BRepHandle`-backed entities, each bound to its own `Cuboid`
//!   cad node.
//! * 100 random parametric edits — each picks an entity, picks a new
//!   `(width, height, depth)` triple, adds the resulting cuboid node
//!   to the graph, commits, remaps the entity to the new node, then
//!   ticks the projection.
//!
//! # Per-edit assertions
//!
//! 1. `cad.head()` strictly advances per commit (`new.0 > prev.0`).
//! 2. `projection.tick()` returns `Ok(TickReport)` with
//!    `head_advanced_to == cad.head()` (invalidation-on-commit).
//! 3. `report.entities_reprojected >= 1` (the remapped entity is dirty
//!    and re-projects within this tick).
//! 4. `projection.projected_mesh(entity)` returns `Some(mesh)` AND the
//!    mesh equals what `cad.graph().evaluate(node, ...)` returns for
//!    the same node (positions / indices / face_labels byte-identical —
//!    triangle fallback always available + `ProjectedMesh` ↔ cad-core
//!    `evaluate()` consistency).
//! 5. `EntityCadMap` coherence: `projection.node_for(entity) ==
//!    Some(new_node)` AND `projection.entity_for(new_node) ==
//!    Some(entity)`.
//!
//! # Substrate
//!
//! D-7.3 shipped 2026-05-06 (`BRepHandle` / `EntityCadMap` /
//! `ProjectedMesh` / `CadProjection::tick`). This dispatch adds the
//! consolidated umbrella gate mirroring the §7.2 idiom; substrate is
//! untouched.
//!
//! Failure class inherited: snapshot-recoverable (test-only).

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, TessellationCache, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};
use rge_kernel_graph_foundation::NodeId;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// §7.3 mnemonic — STRESS-DEAD-BEEF base plus the §7.3 dispatch
/// tag (`0x7E5A_0003`) so the seed differs from §7.2's
/// `0x7E5A_DEAD_BEEF_C0DE` while sharing the lineage convention.
/// Computed value: `0x7E5A_DEAE_3D49_C0E1`.
const STRESS_TEST_SEED: u64 = 0x7E5A_DEAD_BEEF_C0DE_u64.wrapping_add(0x7E5A_0003);

const NUM_ENTITIES: usize = 10;
const NUM_EDITS: usize = 100;

// ---------------------------------------------------------------------------
// Tiny deterministic PRNG (xorshift64; no Cargo.toml change permitted)
// ---------------------------------------------------------------------------

/// Same xorshift64 idiom as `phase_7_2_gate_closure.rs`. Reproduced here
/// so the §7.3 test stays self-contained (cross-crate test dependency
/// would require a Cargo.toml change; HALT-on-stop forbids it).
struct TinyRng(u64);

impl TinyRng {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0, "xorshift64 cannot accept seed=0");
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_range_usize(&mut self, max_exclusive: usize) -> usize {
        debug_assert!(max_exclusive > 0);
        (self.next_u64() % max_exclusive as u64) as usize
    }

    fn next_f32_in_range(&mut self, min: f32, max: f32) -> f32 {
        debug_assert!(max > min);
        let unit = (self.next_u64() & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32;
        min + unit * (max - min)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Add a `Cuboid(w,h,d)` operator + set_root + commit. Returns the new
/// node id. Same pattern as the §7.3 smoke test's `add_cuboid`.
fn add_cuboid(cad: &mut CadGraph, w: f32, h: f32, d: f32, label: &str) -> NodeId {
    cad.begin_operation().expect("begin");
    let node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: w,
            height: h,
            depth: d,
        }))
        .expect("add");
    cad.graph_mut().expect("mut2").set_root(node).expect("root");
    cad.commit(label).expect("commit");
    node
}

// ---------------------------------------------------------------------------
// THE GATE TEST
// ---------------------------------------------------------------------------

/// Phase 7 §7.3 cad-projection minimal — gate-closure umbrella test.
///
/// Builds 10 cuboid-backed entities, then applies 100 random parametric
/// edits with per-edit assertions of the five §7.3 substrate
/// guarantees. See module-level docs for the mechanism + assertion
/// catalog. Seed: `STRESS_TEST_SEED`.
#[test]
fn phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1() {
    // The function name encodes the resolved seed (post `wrapping_add`)
    // for grep-ability. Verify the constant matches the literal so a
    // future rename can't silently drift them apart.
    assert_eq!(
        STRESS_TEST_SEED, 0x7E5A_DEAE_3D49_C0E1,
        "function-name seed must match STRESS_TEST_SEED constant"
    );

    let mut rng = TinyRng::new(STRESS_TEST_SEED);

    // -----------------------------------------------------------------
    // Step 1: build CadGraph with 10 initial Cuboid nodes (one per
    // future entity). Each cuboid gets a unique `(w,h,d)` to ensure
    // distinct content-derived NodeIds.
    // -----------------------------------------------------------------
    let mut cad = CadGraph::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let mut projection = CadProjection::new();

    let mut entities: Vec<EntityId> = Vec::with_capacity(NUM_ENTITIES);
    let mut current_nodes: Vec<NodeId> = Vec::with_capacity(NUM_ENTITIES);

    for i in 0..NUM_ENTITIES {
        // Initial seed dimensions in [0.5, 2.0] with index-jittered
        // offsets to guarantee unique content hashes.
        let w = rng.next_f32_in_range(0.5, 2.0) + (i as f32) * 0.01;
        let h = rng.next_f32_in_range(0.5, 2.0) + (i as f32) * 0.02;
        let d = rng.next_f32_in_range(0.5, 2.0) + (i as f32) * 0.03;
        let node = add_cuboid(&mut cad, w, h, d, &format!("§7.3 init e{i}"));
        let entity = projection
            .spawn_brep_entity(&mut world, node)
            .expect("spawn");
        entities.push(entity);
        current_nodes.push(node);
    }

    // -----------------------------------------------------------------
    // Step 2: initial tick — projects all 10 entities.
    // -----------------------------------------------------------------
    let initial_head = cad.head();
    let r0 = projection
        .tick(&mut world, &cad, tol())
        .expect("initial tick");
    assert_eq!(
        r0.entities_reprojected, NUM_ENTITIES,
        "initial tick must project all {NUM_ENTITIES} entities"
    );
    assert_eq!(
        r0.head_advanced_to, initial_head,
        "tick must observe cad.head() (seed 0x{STRESS_TEST_SEED:016X})"
    );

    // Verify every entity has a projected mesh that matches cad-core's
    // direct evaluation — establishes the post-initial-tick baseline
    // for the triangle-fallback-always-available + ProjectedMesh ↔
    // cad-core::evaluate() consistency guarantee.
    for (idx, &entity) in entities.iter().enumerate() {
        let node = current_nodes[idx];
        let mesh = projection
            .projected_mesh(entity)
            .unwrap_or_else(|| panic!("initial mesh missing for entity idx {idx}"));
        assert_eq!(mesh.vertex_count(), 8, "cuboid must yield 8 vertices");
        assert_eq!(mesh.triangle_count(), 12, "cuboid must yield 12 triangles");

        // ProjectedMesh ↔ cad-core::evaluate() byte-identity (positions
        // + indices + face_labels) via a fresh TessellationCache.
        let mut tess_cache = TessellationCache::new();
        let tess = cad
            .graph()
            .evaluate(node, &mut tess_cache, tol())
            .expect("evaluate");
        assert_eq!(
            mesh.positions, tess.positions,
            "initial baseline: ProjectedMesh positions must equal cad-core evaluate() output (entity idx {idx}, seed 0x{STRESS_TEST_SEED:016X})"
        );
        assert_eq!(
            mesh.indices, tess.indices,
            "initial baseline: ProjectedMesh indices must equal cad-core evaluate() output (entity idx {idx}, seed 0x{STRESS_TEST_SEED:016X})"
        );
        assert_eq!(
            mesh.face_labels, tess.face_labels,
            "initial baseline: ProjectedMesh face_labels must equal cad-core evaluate() output (entity idx {idx}, seed 0x{STRESS_TEST_SEED:016X})"
        );

        // EntityCadMap coherence at baseline.
        assert_eq!(
            projection.node_for(entity),
            Some(node),
            "EntityCadMap: node_for(entity) must equal expected node (entity idx {idx})"
        );
        assert_eq!(
            projection.entity_for(node),
            Some(entity),
            "EntityCadMap: entity_for(node) must equal expected entity (entity idx {idx})"
        );
    }

    // -----------------------------------------------------------------
    // Step 3: 100 random parametric edits.
    // -----------------------------------------------------------------
    let mut prev_head = initial_head;

    for edit_idx in 0..NUM_EDITS {
        // Pick a random entity to mutate.
        let target_idx = rng.next_range_usize(NUM_ENTITIES);
        let target_entity = entities[target_idx];

        // Pick new dimensions in [0.5, 3.0] with an edit-jittered offset
        // to guarantee a new content-derived NodeId distinct from prior
        // edits on the same entity.
        let bump = (edit_idx as f32) * 0.001;
        let w = rng.next_f32_in_range(0.5, 3.0) + bump;
        let h = rng.next_f32_in_range(0.5, 3.0) + bump;
        let d = rng.next_f32_in_range(0.5, 3.0) + bump;
        let new_node = add_cuboid(
            &mut cad,
            w,
            h,
            d,
            &format!("§7.3 edit {edit_idx} e{target_idx}"),
        );

        // Assertion (1): cad.head() strictly advanced per commit.
        let new_head = cad.head();
        assert!(
            new_head.0 > prev_head.0,
            "Phase 7.3 gate FAIL — edit {edit_idx}: cad.head() did not advance (prev={prev_head:?} new={new_head:?}, seed 0x{STRESS_TEST_SEED:016X})"
        );

        // Remap the entity to the new node. This is the canonical
        // post-2026-05-08 SSoT path (BRepHandle no longer carries the
        // cad-node FK; EntityCadMap is the SSoT). Remap marks the
        // entity dirty.
        projection
            .remap_entity(target_entity, new_node)
            .expect("remap");

        // Assertion (5a): EntityCadMap coherence post-remap (before
        // tick).
        assert_eq!(
            projection.node_for(target_entity),
            Some(new_node),
            "Phase 7.3 gate FAIL — edit {edit_idx}: EntityCadMap.node_for(entity) must equal new_node after remap (seed 0x{STRESS_TEST_SEED:016X})"
        );
        assert_eq!(
            projection.entity_for(new_node),
            Some(target_entity),
            "Phase 7.3 gate FAIL — edit {edit_idx}: EntityCadMap.entity_for(new_node) must equal target_entity after remap (seed 0x{STRESS_TEST_SEED:016X})"
        );

        // Tick — invalidation must trigger re-projection within this
        // tick (the §7.3 exit criterion).
        let report = projection
            .tick(&mut world, &cad, tol())
            .expect("tick must succeed");

        // Assertion (2): tick observed the new head.
        assert_eq!(
            report.head_advanced_to, new_head,
            "Phase 7.3 gate FAIL — edit {edit_idx}: tick.head_advanced_to must equal cad.head() (got {:?}, expected {new_head:?}, seed 0x{STRESS_TEST_SEED:016X})",
            report.head_advanced_to
        );

        // Assertion (3): at least one entity reprojected this tick (the
        // remapped one is dirty; head advance also dirties all known
        // entities — see CadProjection::tick docstring). So all 10 are
        // reprojected per tick post-commit.
        assert_eq!(
            report.entities_reprojected, NUM_ENTITIES,
            "Phase 7.3 gate FAIL — edit {edit_idx}: head advance + remap must re-project all {NUM_ENTITIES} entities (got {}, seed 0x{STRESS_TEST_SEED:016X})",
            report.entities_reprojected
        );

        // Assertion (4): the new mesh matches cad-core::evaluate() for
        // the new node — proves the projection ran against the
        // post-edit geometry, not stale data.
        let mesh = projection
            .projected_mesh(target_entity)
            .unwrap_or_else(|| {
                panic!(
                    "Phase 7.3 gate FAIL — edit {edit_idx}: projected_mesh missing after tick (seed 0x{STRESS_TEST_SEED:016X})"
                )
            });
        assert_eq!(mesh.vertex_count(), 8, "cuboid mesh must have 8 vertices");
        assert_eq!(
            mesh.triangle_count(),
            12,
            "cuboid mesh must have 12 triangles"
        );
        assert_eq!(
            mesh.source_node, new_node,
            "Phase 7.3 gate FAIL — edit {edit_idx}: mesh.source_node must match new_node (seed 0x{STRESS_TEST_SEED:016X})"
        );

        let mut tess_cache = TessellationCache::new();
        let tess = cad
            .graph()
            .evaluate(new_node, &mut tess_cache, tol())
            .expect("evaluate");
        assert_eq!(
            mesh.positions, tess.positions,
            "Phase 7.3 gate FAIL — edit {edit_idx}: ProjectedMesh.positions must equal cad-core evaluate() output (seed 0x{STRESS_TEST_SEED:016X})"
        );
        assert_eq!(
            mesh.indices, tess.indices,
            "Phase 7.3 gate FAIL — edit {edit_idx}: ProjectedMesh.indices must equal cad-core evaluate() output (seed 0x{STRESS_TEST_SEED:016X})"
        );
        assert_eq!(
            mesh.face_labels, tess.face_labels,
            "Phase 7.3 gate FAIL — edit {edit_idx}: ProjectedMesh.face_labels must equal cad-core evaluate() output (seed 0x{STRESS_TEST_SEED:016X})"
        );

        // Update bookkeeping for the next iteration.
        current_nodes[target_idx] = new_node;
        prev_head = new_head;
    }

    // -----------------------------------------------------------------
    // Step 4: final sanity — all 10 entities still present + valid
    // (none were silently dropped during the 100 edits).
    // -----------------------------------------------------------------
    for (idx, &entity) in entities.iter().enumerate() {
        let node = current_nodes[idx];
        assert_eq!(
            projection.node_for(entity),
            Some(node),
            "final: EntityCadMap.node_for(entity idx {idx}) must equal current_nodes[{idx}]"
        );
        assert_eq!(
            projection.entity_for(node),
            Some(entity),
            "final: EntityCadMap.entity_for(current_nodes[{idx}]) must equal entity idx {idx}"
        );
        let mesh = projection
            .projected_mesh(entity)
            .unwrap_or_else(|| panic!("final: projected_mesh missing for entity idx {idx}"));
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.source_node, node);
    }

    println!(
        "Phase 7.3 gate CLOSED: {NUM_ENTITIES} entities × {NUM_EDITS} edits = {} mutations. \
         Seed: 0x{STRESS_TEST_SEED:016X}.",
        NUM_ENTITIES * NUM_EDITS
    );
}

// ---------------------------------------------------------------------------
// Sanity test on the harness itself
// ---------------------------------------------------------------------------

/// Same seed produces the same first 10 outputs — basic
/// deterministic-PRNG smoke, mirroring §7.2's
/// `tiny_rng_deterministic_for_fixed_seed`.
#[test]
fn tiny_rng_deterministic_for_fixed_seed() {
    let mut a = TinyRng::new(STRESS_TEST_SEED);
    let mut b = TinyRng::new(STRESS_TEST_SEED);
    for _ in 0..10 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}
