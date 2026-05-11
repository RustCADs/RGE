# ADR-117: Render-input handoff mechanism for Phase 6 §6.3 Gate C

| Status | Accepted 2026-05-11 (binding semantic decision; implementation primitive deferred to Gate C dispatch 4) |
|---|---|
| Date | 2026-05-11 |
| Deciders | (RGE architecture review) |
| PLAN references | §1.5.2 (render-side snapshot staging — `(ECS_tick_N, CadCheckpointId_N)` immutability across sim/render-thread boundary), §13.6 (B-Rep / topology / cad-core gates — render-side snapshot quality gate: "topology mutation during frame doesn't invalidate render thread") |
| ADR references | ADR-114 (PluginContext owned-handoff — owned-cross-boundary substrate precedent), ADR-115 (graph-metrics substrate design — semantics-first, implementation-deferred precedent), ADR-116 (canary protocol — minimal-not-meta codification precedent) |
| Doctrine refs | `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` §3.3 / §5.2 / §6.2 / §6.3, `docs/§18/GFX_RENDER_TIER.md` §1 / §11.2 / §12 |
| Implementation phase | Gate C dispatch 3 (this ADR; design-only). Gate C dispatch 4 implements the chosen semantics in a new `crates/editor-shell/src/render_input/handoff.rs` (or analogue), ~150-250 LOC. Gate C dispatch 5 lands the §13.6 empirical test. |

## Context

Phase 6 §6.3 Gate C is the workspace's "topology mutation during frame doesn't invalidate render thread" quality gate. PLAN §13.6 names it verbatim. Its semantic anchor lives in §1.5.2:

> Standard render-thread / sim-thread separation with double-buffered scene state. Render thread sees immutable snapshot of `(ECS_tick_N, CadCheckpointId_N)` while sim builds N+1.

§13.6 then says the cross-architecture coherence side of the gate is:

> render-side snapshot: topology mutation during frame doesn't invalidate render thread

Gate C's measurability path was de-risked across three small dispatches in 2026-05-11 so the design pressure could be split off from any source-mutation lift:

- **Dispatch 1** (2026-05-11): named the boundary. `crates/editor-shell/src/render_input.rs` exposes a borrowed read-only view-type `RenderInput<'a> { editor_camera: &'a EditorCameraState }` with constructor `RenderInput::from_editor_shell(shell: &'a EditorShell)`. `resize_render_path` and `render_frame` were widened to consume `&RenderInput<'_>` instead of reading `self.editor_camera` directly. Public API of `EditorShell` was preserved. NO `wgpu::*` in the view-type, NO `Send`/`Sync` discipline, NO ownership claim — the dispatch deliberately *named* the boundary and deferred ownership + threading mechanism to a later dispatch.
- **Dispatch 2** (2026-05-11): pinned the boundary discipline as a regression test. `crates/editor-shell/tests/render_input_boundary.rs` reads `render_path.rs` via `include_str!` and asserts the per-frame / per-resize function bodies never re-introduce a `self.editor_camera` read. The rule is editor-shell-local; no `tools/architecture-lints/**` machinery needed.
- **Dispatch 3** (this ADR): pins the **handoff semantics** that the owned variant must satisfy when dispatch 4 implements it. Crate-choice / std-only primitive choice is intentionally **not** decided here — the ADR fixes WHAT the handoff must guarantee, not HOW.

The two dispatch-1/2 substrates leave Gate C *boundary-named + regression-protected* but the handoff itself is still notional: there is no `RenderInputOwned: Send` today, no publish/consume primitive, no anchor wiring, and no test that exercises a sim-side mutation against a held render-side snapshot. Dispatches 4 and 5 close that.

The remaining design pressure splits cleanly into two: (a) what semantic does the handoff need to satisfy to make Gate C measurable per §13.6, and (b) what primitive realises it. This ADR is exclusively (a). The user's expected good outcome is **latest-only immutable snapshot handoff** — whether implemented via the `triple_buffer` crate, `arc-swap`, a manual `Arc<AtomicPtr<_>>` swap, or some other std-primitive composition is a dispatch-4 implementation detail as long as it satisfies the semantics pinned here.

`docs/architecture/SCENE_EXTRACTION_CONTRACT.md` §3.3 ("The renderer NEVER owns authoritative geometry") and §5.2 ("Topology remains canonical") are the upstream invariants this ADR's handoff has to compose with: the snapshot the render thread reads is a *derived view* anchored by `CadCheckpointId_N`, never an authoritative shadow. §6.2 / §6.3 (canonical "today vs anticipated" + "Migration discipline when Phase 6 lands") pre-binds the wire-format stability rules but defers the wire-format ADR itself. `docs/§18/GFX_RENDER_TIER.md` §1 (incremental Phase-6 build-out), §11.2 (the future `gfx.render-snapshot` PIE participant), and §12 (failure-class `recoverable`) further bound what the handoff must NOT impose: GPU resource state stays render-thread-local, payload stays `recoverable`.

The load-bearing semantic this ADR has to satisfy in one sentence: **the render thread sees a stable immutable snapshot of `(ECS_tick_N, CadCheckpointId_N)` while the sim builds the next one, and sim mutations during that frame must not invalidate the render thread's view.**

## Decision

**The render-input handoff is `latest-only immutable snapshot`. Sim publishes immutable `Arc<RenderInputOwned>` allocations whenever a new snapshot is ready. Render reads the most-recently-published snapshot at frame-start. Older un-read snapshots are dropped (their `Arc` reference count reaches zero once the next snapshot replaces them AND any in-flight reader has finished). Render NEVER blocks sim. Sim NEVER blocks render beyond a trivial atomic-pointer-swap. The pair `(ECS_tick_N, CadCheckpointId_N)` is stored inside `RenderInputOwned` as fields; their values at publish time ARE the snapshot's identity. `GfxContext` remains render-thread-local; its `RefCell<PipelineCache<_>>` does not cross the boundary.**

Five sub-decisions follow.

### Sub-decision 1 — handoff semantics: latest-only immutable snapshot

The snapshot has four binding properties:

1. **Latest-only**. Render reads the most recent complete snapshot at frame-start. If sim publishes K snapshots between two render frames, only the most recent (the Kth) is visible at the next frame; the first K-1 are dropped. The render thread NEVER sees a partially-built snapshot.
2. **Immutable from publish**. Once sim publishes a snapshot, sim does NOT mutate it. The next snapshot is a fresh allocation. (`Arc<RenderInputOwned>` makes this a Rust-level invariant: `Arc<T>` exposes only `&T`, never `&mut T`, to readers.)
3. **Non-blocking on both sides**. Render NEVER waits for sim to publish; if no snapshot has ever been published, render either skips the frame or uses a sentinel — the choice belongs to dispatch 4 / 5. Sim NEVER waits for render to drain; sim publishes whenever it has a complete snapshot ready, regardless of whether render has read the previous one.
4. **Anchored by `(ECS_tick_N, CadCheckpointId_N)`**. The snapshot's identity is the value of those two fields *at publish time*. Cross-architecture coherence per PLAN §13.2 + SCENE_EXTRACTION_CONTRACT.md §5.4 is anchored on the pair; the render thread reading the pair off the snapshot is equivalent to reading the kernel-ecs tick and the cad-projection checkpoint at the publish-time atomic instant.

### Sub-decision 2 — ownership model: `Arc<RenderInputOwned>` with `Send` bound

The owned variant is the type `pub struct RenderInputOwned { ... }` shipped under `crates/editor-shell/src/render_input/` in dispatch 4. The trait bound is `RenderInputOwned: Send + 'static`; the handoff stores and exchanges `Arc<RenderInputOwned>` allocations.

`Arc<_>` over a raw `Box<_>` or owned-by-value handoff is chosen because:

- **Future-proofs multi-reader.** Today's render thread is one consumer. A future side-channel (e.g. a render-thread-adjacent observability collector that wants to walk the snapshot it just consumed) is a single `Arc::clone` away.
- **Drop semantics align with latest-only.** When sim publishes a new snapshot, the previous one's strong-count drops by one; once render is also done with it, strong-count reaches zero and `Drop` runs. No explicit lifetime tracking, no rendezvous, no separate buffer pool.
- **Render-thread retention.** Render reads `Arc<RenderInputOwned>` at frame-start, holds the strong-count for the entire frame, and releases at frame-end. Sim can publish freely during that window without invalidating the render-thread view — the previous snapshot stays alive exactly as long as the render thread is using it.

`Send + 'static` is the trait bound that lets the snapshot cross a future sim-thread / render-thread boundary. Today's renderer runs inline on `WindowEvent::RedrawRequested` (single-threaded — see "Explicit non-decisions" below), so `Send` is not yet *required* by the runtime, but pinning it now means dispatch 4 ships the cross-thread substrate at the same time as the single-threaded usage, and the future move to two threads is a zero-API-change deployment.

Per-field ownership of what *goes into* `RenderInputOwned` (camera, light state, projected meshes, material handles, …) is intentionally **not** decided in this ADR; per-field ownership is a "what is the wire-format payload" question that belongs to the wire-format ADR (see Explicit non-decisions §1).

### Sub-decision 3 — anchor shape: `(ECS_tick_N, CadCheckpointId_N)` fields + monotonic `AtomicU64` generation

`RenderInputOwned` carries two anchor fields directly, set at publish time:

```rust
pub struct RenderInputOwned {
    pub ecs_tick: u64,           // value of kernel-ecs tick at publish-time
    pub checkpoint_id: u64,      // value of cad-projection CheckpointId at publish-time
    // … payload fields (deferred to wire-format ADR)
}
```

A separate `AtomicU64` generation counter lives on the handoff substrate (not inside `RenderInputOwned`): sim increments it monotonically on each publish. Render reads the generation at frame-start, locks onto the corresponding snapshot, and uses the snapshot's `(ecs_tick, checkpoint_id)` as its cross-architecture coherence anchor for the rest of the frame.

The generation counter is NOT the same as `ecs_tick` or `checkpoint_id`. It is an opaque-to-consumers monotonic identifier whose only job is to let render answer "did sim publish a new snapshot since I last looked?" in O(1) without locking. Render-thread inspection of `ecs_tick` and `checkpoint_id` is what feeds cross-architecture coherence + render-thread immutability tests; the generation is internal to the handoff mechanism.

This shape composes cleanly with the cross-architecture coherence quality gate (PLAN §13.2): a `PIE` participant that captures the snapshot's `(ecs_tick, checkpoint_id)` at the same instant the render thread reads them gives a deterministic anchor to compare across replays.

### Sub-decision 4 — render-lag behavior: latest-only / drop-old

If sim publishes 3 snapshots between two render frames (e.g. sim runs at 240 Hz while render runs at 60 Hz on a frame the render thread happened to skip), the render thread sees only the most recent of the 3 at the next frame-start. The older two are dropped — their `Arc` strong-counts go to zero as sim's atomic-pointer-swap replaces them.

This is **load-bearing** for two reasons:

- **Render never blocks sim.** A bounded-channel-with-blocking-write semantics would couple sim's pace to render's pace; once render misses a deadline, sim stalls on send. Gate C is explicitly about decoupling these two. Latest-only / drop-old preserves the decoupling by construction.
- **Render never sees stale.** A "queue every snapshot and drain at frame-start" semantics would let render fall arbitrarily behind sim (a slow render frame produces a deep queue, then the next frame has to chew through all of it). Latest-only / drop-old means render always reads "what sim believed at the most recent moment a snapshot was ready" — the best approximation of "now" the substrate can produce.

Render NEVER blocks sim. Sim NEVER waits for render. The atomic-pointer-swap on the publish side is the only synchronization primitive that has to be uncontended on the hot path.

### Sub-decision 5 — implementation primitive: pin SEMANTICS, defer crate/std choice to dispatch 4

The semantics above are realisable by at least three distinct primitives:

- **Manual `Arc<AtomicPtr<RenderInputOwned>>`** — sim builds a `Box<RenderInputOwned>`, swaps the pointer with `AtomicPtr::swap(_, Ordering::AcqRel)`, then drops the returned old pointer via `Box::from_raw` (the only path that needs unsafe in this option — and the workspace policy is `unsafe_code = "forbid"`, so this option's "manual" variant is actually `Arc<Mutex<Option<Arc<RenderInputOwned>>>>` or equivalent **safe-Rust composition**: the lock is uncontended on the steady-state because both sides only touch it briefly to swap a single `Arc<_>` reference, and the workspace pledge is preserved).
- **`arc-swap` crate** — `ArcSwap<RenderInputOwned>` does exactly this with lockless reads and a safe wait-free swap. The crate is already widely-vetted (used by Tokio's runtime registry, hyper, etc.) but it is **not** in the workspace today.
- **`triple_buffer` crate** — present in `Cargo.lock` 8.1.1 as a kira transitive dep, NOT a workspace-direct dep. Idiomatic for the pattern but adds an explicit allocation per snapshot and is structurally more than is needed for the "latest-only single-`Arc`" case.

Dispatch 4 picks among these. The ADR's recommendation (not requirement) is to **start with the std-only safe-Rust composition** (`Arc<Mutex<Option<Arc<RenderInputOwned>>>>` or `Arc<RwLock<Option<Arc<RenderInputOwned>>>>` with the lock held only across the trivial pointer-swap) and escalate to `arc-swap` only if measurement reveals contention or correctness issues. The reasoning: (a) the snapshot payload today is one `EditorCameraState` (Copy, ~32 bytes); contention on a mutex held for an `Arc::clone` is negligible, (b) one fewer dep in the tree is one fewer governance line, (c) `arc-swap` is an excellent fit if the empirical primitive turns out to need it, but the workspace's discipline is "add the dep when measurement says so", not "add the dep speculatively".

**`GfxContext` remains render-thread local.** Today's `crates/gfx/src/context.rs` wraps a `RefCell<PipelineCache<wgpu::RenderPipeline>>` (single-thread by construction; `RefCell` is `!Sync`). Nothing in this ADR's handoff mechanism touches the renderer's GPU state — the render thread reads the immutable snapshot at frame-start and consumes it locally; the wgpu device + queue + pipeline cache stay on the render thread. The handoff IS the boundary; the renderer's GPU substrate stays where it is.

## Alternatives considered

| Alternative | Pros | Cons | Decision |
|---|---|---|---|
| **Latest-only triple-buffer semantics (this ADR's choice)** | Render never blocks sim; sim never blocks render; render always sees most-recent; ownership is by construction via `Arc` strong-counts | Slightly more memory than double-buffer (peak ~2-3 `RenderInputOwned` resident); choice of std-primitive vs vetted crate is a design call | **Chosen.** Satisfies §1.5.2 + §13.6 by construction; semantics-first decision lets dispatch 4 pick the primitive empirically. |
| **Double-buffer (manual, two-slot)** | Names PLAN §1.5.2's mechanism verbatim ("double-buffered scene state"); minimal memory (exactly two `RenderInputOwned` resident) | Render and sim must coordinate which buffer is being read; either a blocking flag-dance (violates "render never blocks sim") or an atomic-flip with the same `Arc<_>` discipline as the latest-only option (collapses to the chosen option) | Rejected. The "manual double-buffer" framing collapses to the chosen latest-only option once `Arc<_>`-based ownership is applied; the literal-double-buffer alternative is the flag-dance variant, which violates the non-blocking semantics. |
| **`Arc<RwLock<RenderInputOwned>>` (sim writes under exclusive lock; render reads under shared lock)** | Single primitive; standard library | Render blocks sim during read (every read takes the lock); concurrent readers possible but writer must wait for all readers; the lock is contended on the hot path | Rejected. Violates "render never blocks sim" semantics; render holding the read-lock across a long frame blocks sim's exclusive-write requirement. |
| **Channel (mpsc / `crossbeam-channel` bounded with drop-old policy)** | Familiar pattern; bounded queue with explicit drop policy | Render must drain the channel at frame-start to find the latest (or use a peek-and-discard loop); more machinery than an atomic pointer swap; sender-side must commit to a bound | Rejected as primary. Possible as a dispatch-4 implementation primitive if `crossbeam-channel` semantics turn out to be the cleanest realization of latest-only, but heavier than needed for the single-element case. |
| **Per-frame full clone via `Arc<>` with no handoff substrate (render asks sim for a fresh snapshot each frame)** | Trivial; no primitive needed | Synchronous coupling; render's request blocks sim's mutation path; no anchor on `(ECS_tick_N, CadCheckpointId_N)` since the snapshot is built ad-hoc | Rejected. Re-introduces the synchronous coupling the §1.5.2 separation exists to break; provides no answer to "sim mutated mid-frame, what does render see" except "the snapshot you asked for, which was built mid-sim-mutation". |
| **`SnapshotParticipate` PIE participant for `gfx.render-snapshot`** | Already an anticipated participant per GFX_RENDER_TIER.md §11.2; would PIE-serialise the snapshot for replay | PIE is per-tick state-replication, not per-frame ownership-handoff; the abstraction is wrong-shaped for "render holds immutable snapshot during frame while sim builds next" (per the prior Gate C inspect finding) | Rejected. The PIE substrate is a peer concern (cross-process / replay-time) and orthogonal to the per-frame in-process handoff. Keeping `gfx.render-snapshot` PIE participant decoupled from this ADR is deliberate; the participant remains deferred per GFX_RENDER_TIER.md §11.2. |

The decision matrix collapses to: the chosen option is the unique shape that satisfies both non-blocking requirements simultaneously with a single primitive that costs an atomic pointer swap on publish and an atomic pointer load on consume. The recommendation to start with std-primitives and escalate only on measurement is the conservative reading of "add the dep when measurement says so".

## Consequences

### Positive

- **Gate C measurability path is unblocked.** Dispatch 4 implements the chosen primitive; dispatch 5 lands a test that holds an `Arc<RenderInputOwned>` on the render side, drives a sim-side mutation (e.g. `CadGraph::commit` on the active entity rebuilding the projection), and asserts the held snapshot's `(ecs_tick, checkpoint_id)` is unchanged. The §13.6 quality gate becomes empirically gated.
- **Render-thread immutability is a Rust-level invariant, not a doctrine.** `Arc<T>` exposes only `&T`. Once a snapshot is published, sim has no path to mutate it; the next snapshot is a fresh allocation. Doctrine becomes structurally enforced.
- **Cross-thread future is a zero-API-change deployment.** The `RenderInputOwned: Send + 'static` bound is pinned now; today's single-threaded renderer + tomorrow's multi-threaded renderer use the same handoff substrate. The runtime threading model is an orthogonal future decision.
- **Cross-architecture coherence anchor is explicit.** `(ECS_tick_N, CadCheckpointId_N)` lives as concrete fields inside `RenderInputOwned`; downstream tooling (PIE participants, observability dashboards, replay diagnostics) can read the anchor without traversing the handoff substrate's internals.
- **Semantics-first decision composes with implementation flexibility.** Dispatch 4 has clear freedom on primitive choice (std-only Mutex-around-Arc, `arc-swap`, manual atomic with composition) and the ADR's success criterion is empirical: does the chosen primitive satisfy the four semantic properties pinned in sub-decision 1? Implementation-level escalation is bounded.

### Negative / risks

- **`Arc<RenderInputOwned>` allocates per publish.** Allocation cost on the sim side per snapshot is one `Arc::new` (one heap allocation per published snapshot). At sim-tick rate (~60-240 Hz depending on simulator pace), the cost is sub-microsecond on commodity hardware. Documented; if a future high-frequency scenario surfaces a real bottleneck, a pool / arena allocator can be layered on top of the existing primitive without re-litigating the semantics. (Same risk-class as ADR-114's "two allocator interactions per resource per call" — the workspace's discipline is to document the cost and escalate empirically.)
- **Peak resident memory is ~2-3× `RenderInputOwned`.** Sim's about-to-publish slot + the published-but-not-yet-read slot + (transiently) the previous published slot held by an in-flight reader. Today's `RenderInputOwned` payload is one `EditorCameraState` (~32 bytes); peak is negligible. Future payload growth (light state, projected meshes, material handles) compounds — per the wire-format ADR, payload size is a separate budget concern.
- **No anchor protocol pinned in this ADR.** The "sim reads `ECS_tick_N` and `CadCheckpointId_N` at publish-time" is the *concept*; the protocol that ensures sim atomically reads both values at the same instant the snapshot's payload was constructed is a dispatch-4 implementation concern. If the protocol cheats (e.g. sim reads `ecs_tick` before building the payload and reads `checkpoint_id` after), the anchor becomes meaningless. Dispatch 4 must pin a single read-instant; the wire-format ADR may codify the protocol.
- **`unsafe` is foreclosed at this layer.** The workspace's `unsafe_code = "forbid"` policy means the manual-`AtomicPtr<_>` variant requires the `Box::from_raw` / `mem::transmute` discipline that is not available without an `unsafe` block. This intentionally narrows the std-only implementation space to `Mutex<Option<Arc<_>>>` / `RwLock<Option<Arc<_>>>` / similar safe-Rust compositions. Dispatch 4 may NOT cite this ADR as justification for an `unsafe` block.

### Mitigations

- **Single-publisher, single-consumer is the v0 contract.** Today's sim is the only publisher; today's render is the only consumer. The handoff substrate's tests in dispatch 4 lock that down; the multi-publisher / multi-consumer extension is reserved for a future amendment if it surfaces.
- **The wire-format ADR captures the per-field ownership decision.** `editor_camera`'s final ownership (sim-state vs render-coord-state) is *not* pinned by placing it in `RenderInputOwned` here. The wire-format ADR (anticipated; see Future Work §1) catalogues the snapshot's fields, their authoritative source, and the per-field ownership convention.
- **Latest-only / drop-old foreclosed on a stuck-old-snapshot bug class.** A queue-based handoff could let render fall arbitrarily behind sim, holding a chain of stale snapshots. Latest-only / drop-old means at most 2-3 are live at once, and the oldest is droppable as soon as the next is published.
- **`GfxContext` render-thread-local-by-`RefCell` is preserved as a structural pre-condition.** No part of this ADR re-litigates the renderer's threading model; `GfxContext` stays render-thread local. The boundary is the snapshot; the GPU state is downstream of it.

## Explicit non-decisions

This ADR deliberately does NOT decide:

1. **The wire-format / payload shape of `RenderInputOwned`.** What fields beyond `(ecs_tick, checkpoint_id)` go into the snapshot (light state, projected meshes, material handles, …) is a separate wire-format ADR. SCENE_EXTRACTION_CONTRACT.md §6.3 pre-binds the stability rules (per-field additions OK; removals / renames break the contract); the wire-format ADR enumerates the v0 field set.
2. **Whether there is a dedicated render thread today.** Today's renderer runs inline on `WindowEvent::RedrawRequested` (single-threaded). The handoff mechanism this ADR pins works in single-threaded execution (the sim's publish and the render's consume happen serially within the same thread; the `Send` bound is satisfied trivially). The future move to a dedicated render thread is a runtime threading-model decision, out of scope.
3. **The renderer threading model proper.** Whether render lives on its own OS thread, on a tokio task, in a winit-driven event-loop closure, or in some other shape is unrelated to whether the handoff substrate's invariants hold. Out of scope.
4. **`editor_camera`'s final ownership (sim-state vs render-coord-state).** Today it lives on `EditorShell`; this ADR places it inside `RenderInputOwned`'s scope but does not pin sim-ownership vs render-coord-ownership. The wire-format ADR may further refine this when the v0 payload field set lands.
5. **Whether the `gfx.render-snapshot` `SnapshotParticipate` participant ever materialises.** The prior Gate C inspect found that the participant abstraction is PIE-shaped — wrong abstraction for the per-frame immutability concern this ADR scopes. This ADR's handoff is in-process and per-frame; PIE is cross-process and per-tick. The two substrates are orthogonal. The participant remains deferred per GFX_RENDER_TIER.md §11.2 and is NOT re-opened here.
6. **Specific crate vs std-only primitive choice.** Dispatch 4 picks empirically. This ADR's recommendation is "start with std-only safe-Rust composition; escalate to a vetted crate (`arc-swap` primarily) only if measurement reveals contention or correctness issues". Adding `arc-swap` or any other dep is a dispatch-4 concern requiring its own `Cargo.toml` justification.

## Future work

- **Dispatch 4 — implementation of the chosen semantics.** Footprint estimate: NEW `crates/editor-shell/src/render_input/handoff.rs` (or `crates/editor-shell/src/render_input/mod.rs` if the existing borrowed `RenderInput<'a>` moves into the module too); `RenderInputOwned: Send + 'static` struct + `SnapshotPublisher` (sim-side `publish_snapshot(input: RenderInputOwned)` API) + `SnapshotConsumer` (render-side `current_snapshot() -> Option<Arc<RenderInputOwned>>` API) + the chosen primitive (std-only safe-Rust composition recommended; `arc-swap` permissible if justified). ~150-250 LOC depending on primitive choice + tests. API names tentative.
- **Dispatch 5 — Gate C empirical test per PLAN §13.6.** Test holds an `Arc<RenderInputOwned>` on the render side at a known generation, drives a sim-side topology mutation (e.g. parameter rebuild on the active CAD entity triggering a `CadGraph::commit` + `cad-projection::tick`), reads back the held snapshot's `(ecs_tick, checkpoint_id)`, and asserts byte-identical to the captured-at-snapshot-time values. Footprint estimate: 1 new test file in `crates/editor-shell/tests/` or `crates/cad-projection/tests/`, ~80-120 LOC including the mutation-driving helpers. The §13.6 quality gate becomes empirically gated.
- **Wire-format ADR (sibling).** What fields beyond `editor_camera` go into `RenderInputOwned`'s v0 payload (likely camera + light state + projected mesh handles + material handles, all anchored on cad-projection IDs per PLAN §13.2). SCENE_EXTRACTION_CONTRACT.md §6.2 / §6.3 pre-binds the stability discipline; the wire-format ADR ratifies it as the formal contract. Trigger: when the second non-camera per-frame sim-side read surfaces.
- **Per-field ownership of future fields.** Each new field added to `RenderInputOwned` lands per-field in its own design discussion when added: is it sim-state (the field's authoritative truth lives sim-side and the snapshot captures a copy), render-coord-state (the field's truth lives render-side and the snapshot is a structural carrier), or shared (the snapshot is the field's authoritative location at runtime)? The wire-format ADR is the natural home for the catalogue.
- **PIE participant re-evaluation when Phase 6 wire-format lands.** GFX_RENDER_TIER.md §11.2 defers `gfx.render-snapshot` participant materialisation. The per-frame handoff this ADR pins is orthogonal to the per-tick PIE concern, but a future ADR may revisit whether the in-process snapshot's wire-format and the PIE participant's wire-format converge (likely YES — both anchor on cad-projection IDs and both serialize the same payload shape). Deferred until the wire-format ADR lands.

## References

- **PLAN.md §1.5.2** — render-side snapshot staging; sim/render-thread split target.
- **PLAN.md §13.6** — render-side snapshot quality gate.
- **PLAN.md §13.2** — cross-architecture coherence; PIE participant anchor.
- **ADR-114** — PluginContext owned-handoff (owned-cross-boundary substrate precedent).
- **ADR-115** — graph-metrics substrate design (semantics-first, implementation-deferred precedent).
- **ADR-116** — canary protocol (minimal-not-meta codification precedent).
- **`docs/architecture/SCENE_EXTRACTION_CONTRACT.md`** §3.3 / §5.2 / §6.2 / §6.3 — ownership rules + topology-canonical invariant + today-vs-anticipated + migration discipline.
- **`docs/§18/GFX_RENDER_TIER.md`** §1 / §11.2 / §12 — incremental Phase-6 substrate + `gfx.render-snapshot` participant deferral + failure-class.
- **`crates/editor-shell/src/render_input.rs`** — dispatch 1's borrowed view-type; the boundary this ADR's owned variant is the cross-thread complement of.
- **`crates/editor-shell/tests/render_input_boundary.rs`** — dispatch 2's regression test pinning the discipline.
- **`crates/gfx/src/context.rs`** — `GfxContext` with `RefCell<PipelineCache<_>>`; the render-thread-local substrate this ADR preserves.
- **`tools/architecture-lints/src/snapshot_participate.rs`** — current `STATEFUL_TIER2_CRATES` list; gfx absent per the audit-3 H3 closure; this ADR does NOT re-add gfx.
