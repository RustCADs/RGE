# SEMANTIC_ARCHITECTURE_LAWS

| Status | Doctrine-tier v0; consolidated semantic law set. Binding as architectural doctrine, but not a new crate, lint, ADR, or runtime subsystem. |
|---|---|
| Audience | Reviewers deciding whether a feature has a clear semantic authority, mutation path, projection boundary, identity story, replay story, propagation story, and drift signal. |
| Companion to | `REACTIVE_INVALIDATION.md` (dependency propagation), `SCENE_EXTRACTION_CONTRACT.md` (projection ownership), `INVARIANT_ENFORCEMENT_STRATEGY.md` (what remains prose vs what graduates to enforcement), `docs/§18/PIE_SNAPSHOT.md` (snapshot participation), `docs/§18/EDITOR_ACTIONS_COMMAND_BUS.md` (mutation bus), `docs/§18/CAD_TOPOLOGY_LINEAGE.md` (topology evolution), `docs/§18/CAD_CORE_MODEL.md` (CAD authority). |
| Sibling docs | `NON_GOALS.md`, `REACTIVE_INVALIDATION.md`, `SCENE_EXTRACTION_CONTRACT.md`, `INVARIANT_ENFORCEMENT_STRATEGY.md`, `ARCHITECTURAL_TEST_TAXONOMY.md`. |
| Source fragments | `RGE_Semantic_Architecture_Docs.zip`: `semantic_constitution.md`, `semantic_authority.md`, `mutation_law.md`, `projection_law.md`, `identity_continuity.md`, `replay_model.md`, `propagation_model.md`, `drift_detection.md`, `cascade_preview_model.md`. `RGE_Improvement_Direction.zip`: `RGE_Improvement_Direction.md`. `RGE_100_Percent_Semantic_Runtime.zip`: `RGE_100_Percent_Semantic_Runtime.md`. `RGE_Semantic_Runtime_Rerating.zip`: `RGE_Semantic_Runtime_Rerating.md`. |

> Doctrine-tier doc - semantic law set. These laws describe how RGE treats
> semantic truth, mutation, projections, identity, replay, propagation, drift,
> and preview. They do not create a new "semantic runtime" crate. They name the
> invariants existing and future substrates must respect.

## 1. Imported law set

The source archive contained nine short law fragments. They are consolidated
here so the doctrine tier stays navigable and the laws can be read as one
coherent authority surface.

| Law | Source fragment | Core principle |
|---|---|---|
| Semantic Constitution | `semantic_constitution.md` | Define the architectural law governing the semantic runtime. |
| Semantic Authority Law | `semantic_authority.md` | There must be exactly one authoritative semantic substrate. Everything else is projection. |
| Mutation Law | `mutation_law.md` | All mutations must be deterministic, replayable, traceable, and dependency-aware. |
| Projection Law | `projection_law.md` | Projections are views, not authorities. |
| Identity Continuity | `identity_continuity.md` | Entities survive transformation. |
| Replay Model | `replay_model.md` | State must be reconstructable from mutation history. |
| Propagation Model | `propagation_model.md` | Every semantic mutation propagates through dependency topology. |
| Drift Detection | `drift_detection.md` | Semantic fragmentation must become observable. |
| Cascade Preview Model | `cascade_preview_model.md` | Before any mutation executes, the runtime previews impact. |

## 2. Vocabulary

**Semantic fact** means a piece of meaning that other systems may rely on: a
CAD face identity, an ECS component value, an editor selection, a dependency
edge, a material assignment, a checkpoint, a mutation record.

**Authority** means the one substrate that owns the canonical value for a
semantic fact. The authority can differ by fact. This law does not mandate one
global store for all meaning; it mandates one owner per fact.

**Projection** means a derived view of authoritative state: tessellation,
scene extraction, render resources, editor overlays, caches, inspection views,
diagnostic summaries, and similar derived products.

**Mutation** means an intentional change to authoritative semantic state. A
mutation may be small, but it is still part of the deterministic history of
the fact it changes.

**Drift** means more than one substrate appearing to own the same fact, a
projection that no longer corresponds to its authority, or an untracked change
that prevents replay or dependency propagation from explaining the current
state.

**Cascade preview** means a side-effect-free forecast of what a mutation will
invalidate or update. It is a semantic impact preview, not permission to mutate
early.

## 3. Semantic Authority Law

For any semantic fact, exactly one substrate is authoritative.

RGE intentionally has several authoritative substrates because different facts
live in different domains: the CAD operator graph owns CAD operation structure,
the ECS world owns entity/component state, the audit ledger owns mutation
history, and editor-state owns coordination-only editor state. The law is not
"put everything in one crate"; the law is "never let two crates both claim to
be the canonical owner of the same fact."

Rules:

- An authority owner must be named before a feature adds a second store of the
  same semantic fact.
- A projection may cache, index, summarize, or display authoritative state,
  but it must not become a competing source of truth.
- If an authority and a projection disagree, the authority wins and the
  projection must be invalidated, rebuilt, or diagnosed.
- A new cross-domain semantic fact must declare which existing substrate owns
  it, or justify a new owner through the normal architecture-freeze gate.

## 4. Mutation Law

All semantic mutations must be deterministic, replayable, traceable, and
dependency-aware.

Deterministic means the same starting state and same mutation payload produce
the same semantic result. Replayable means the mutation can be applied again in
the same order to reconstruct the relevant state. Traceable means a reviewer,
diagnostic, or audit tool can explain what changed and where the mutation came
from. Dependency-aware means the mutation identifies enough affected topology
that derived views can be invalidated intentionally.

Rules:

- Mutations to authoritative state must pass through the appropriate mutation
  surface for that domain: command bus, CAD checkpoint operation, ECS snapshot
  participant, audit-ledger event, or a domain-specific equivalent.
- Mutations must not rely on ambient nondeterminism. Time, IO, randomness, and
  platform state must be converted into explicit inputs before they affect
  authoritative state.
- A mutation that cannot describe its dependency impact is not ready to become
  a semantic mutation. It can remain a local implementation detail or an
  experimental helper until its dependency surface is understood.
- Error paths must be traceable. Silent partial mutation is semantic drift.

## 5. Projection Law

Projections are views, not authorities.

The renderer does not own canonical geometry. A projected mesh does not own
the CAD operation that produced it. An editor overlay does not own the entity,
component, face, edge, or material it highlights. A cache does not own the
semantic fact it accelerates.

Rules:

- A projection must name the authority it derives from.
- A projection may be discarded and rebuilt without semantic loss.
- A projection must not mutate its authority by side effect. Mutations travel
  through the authority's mutation surface.
- Projection identity is allowed, but it is derived identity. It must not be
  confused with canonical semantic identity.

## 6. Identity Continuity

Entities survive transformation.

Identity continuity means a topology-preserving or placement-only operation
does not fabricate a new semantic identity for the same fact. Transforming a
CAD node, moving an entity, or rebuilding a cuboid with new dimensions must not
destroy identity when the underlying topology is unchanged.

Rules:

- Identity must not be derived from parameter-sensitive hashes when the
  feature promises rebuild stability.
- Topology-preserving operations should inherit or preserve existing identity.
- Topology-changing operations must either describe the evolution explicitly or
  return an explicit unsupported/topology-changing result.
- A feature that cannot distinguish topology-preserving from
  topology-changing mutation must not promise identity continuity.

Current examples include B-Rep owner-seeded face and edge identities in
`cad-core`, Transform inheritance through graph-level face/edge resolvers, and
explicit topology-changing errors for unsupported operators.

## 7. Replay Model

Semantic state must be reconstructable from mutation history, together with
the canonical inputs those mutations reference.

This law does not claim every cache or projection is replayed byte-for-byte.
Derived views can be rebuilt. Snapshots can accelerate restore. The semantic
claim is that authoritative state has an explainable history and that replay
does not depend on hidden mutable projection state.

Rules:

- Mutation history must include enough payload to explain the semantic change.
- Snapshots are allowed, but they do not replace the replay obligation for
  systems that claim replay semantics.
- External assets must be named by stable identity or content-addressed input
  when they participate in replay.
- A replay failure is a semantic diagnostic, not an invitation to silently
  accept the current derived state.

## 8. Propagation Model

Every semantic mutation propagates through dependency topology.

Propagation is the bridge between authority and projection. If an authority
changes, every dependent projection, cache, render resource, editor view, and
diagnostic summary must either update, invalidate, or report that it cannot
prove freshness.

Rules:

- Dependency topology must be explicit enough to walk.
- Propagation must be deterministic; the same mutation frontier must produce
  the same invalidation frontier.
- Propagation may be lazy, but laziness must be observable. "Not rebuilt yet"
  is a state, not a hidden accident.
- Cross-layer propagation must respect authority boundaries. A lower-authority
  projection cannot push mutation back into the owner to make its cache easier
  to maintain.

## 9. Drift Detection

Semantic fragmentation must become observable.

The architecture cannot prevent every drift case at the type level. It can
require that drift is diagnosable: duplicate authority, stale projection,
untracked mutation, and replay mismatch must have a surface where they can be
observed.

Rules:

- Duplicate authority claims should be caught at review time, doctrine time,
  or lint time when the pattern stabilizes.
- Stale projections must be detectable through revision, checkpoint,
  dependency, or explicit freshness metadata.
- Replay mismatch must produce an actionable diagnostic.
- Drift detection is not automatic repair. Repair policy belongs to the owner
  of the semantic fact or to a future explicit recovery subsystem.

## 10. Cascade Preview Model

Before a semantic mutation executes, the runtime should be able to preview its
impact where the relevant dependency topology exists.

This is a vision-level law today, not a global runtime gate. RGE already has
pieces of the necessary substrate: dependency graphs, CAD checkpoints,
reactive invalidation doctrine, B-Rep identity, and projection caches. A
general cascade preview surface should land only where real mutation pressure
demonstrates the shape.

Rules:

- Preview must be side-effect-free.
- Preview must report the authority nodes, projections, caches, and identities
  expected to change.
- Preview must distinguish certainty from advisory estimation.
- If a subsystem cannot yet preview impact, it must not pretend it can. It may
  surface "unknown impact" honestly.

## 11. Enforcement status

These laws are doctrine. Some are already partially enforced by existing
substrates; others intentionally remain prose until implementation pressure
stabilizes the right mechanism.

| Law | Current enforcement surface |
|---|---|
| Semantic Authority | Architecture review, dependency lints, projection doctrine, domain-specific owner APIs. |
| Mutation | Command bus, audit ledger, CAD checkpoint operations, snapshot participants, deterministic tests. |
| Projection | Scene extraction doctrine, cad-projection APIs, renderer ownership rules, review discipline. |
| Identity Continuity | B-Rep face/edge identity tests and Transform inheritance resolvers in `cad-core`. |
| Replay | PIE snapshot tests, audit-ledger replay tests, deterministic substrate tests. |
| Propagation | Reactive invalidation doctrine, graph-foundation substrates, projection cache tests. |
| Drift Detection | Diagnostics substrate, architecture lints, freshness/checkpoint fields where implemented. |
| Cascade Preview | Prose-only vision until a concrete preview substrate lands. |

## 12. Non-goals

This law set does not add:

- A new `semantic-runtime` crate.
- A global semantic database.
- A new architecture lint.
- A new ADR.
- A new test harness.
- A requirement that every mutation surface implement cascade preview today.
- A requirement that projections become more stateful so they can explain
  themselves.
- A shortcut around the existing architecture-freeze gate for new first-class
  subsystems.

## 13. Improvement direction

The improvement-direction source frames RGE's next maturity step as:

```text
making semantic coherence mechanically unavoidable
```

This is an advisory direction, not a new execution plan. It should guide
priority decisions after the current substrate has earned pressure from real
consumer work.

Highest-value improvement themes:

| Direction | Purpose | Doctrine link |
|---|---|---|
| Canonical semantic graph | Prevent authority fragmentation by making real truth explicit. | Semantic Authority Law |
| Deterministic mutation pipeline | Route changes through validate -> dependency analysis -> propagation -> replay log -> commit. | Mutation Law |
| Dependency visibility | Let the runtime answer "what depends on this?" without ad hoc inspection. | Propagation Model |
| Cascade preview | Forecast affected entities, rebuild cost, instability risk, dependency depth, and rollback regions before mutation. | Cascade Preview Model |
| Semantic lineage everywhere | Explain where semantic state came from, what changed it, and what depends on it. | Identity Continuity + Replay Model |
| Hidden-state elimination | Prevent caches and projections from becoming unofficial authorities. | Projection Law + Drift Detection |
| Replay inspector | Support mutation stepping, topology history inspection, propagation tracing, semantic diffs, and causality inspection. | Replay Model |
| Drift diagnostics | Detect projection mismatch, identity divergence, replay instability, authority duplication, and semantic fragmentation early. | Drift Detection |
| Semantic/execution split | Keep semantic meaning distinct from GPU execution, serialization format, and render implementation. | Projection Law |
| Plugin doctrine stress | Ensure plugin APIs stay authority-safe, replay-safe, and propagation-aware before ecosystem entropy scales. | Mutation Law + Semantic Authority Law |

Near-term priority should favor semantic substrate over:

- fancy rendering,
- editor polish,
- plugin marketplaces,
- cloud complexity,
- scripting sprawl,
- ecosystem expansion.

The strategic shift is from:

```text
engine with semantic systems
```

to:

```text
semantic runtime with rendering capabilities
```

The long-term threat this section guards against is semantic entropy: truth
slowly fragmenting across subsystems until replayability, observability,
propagation correctness, and topology coherence decay. The three highest
leverage priorities from the source are:

| Priority | Why it matters |
|---|---|
| Canonical Semantic Graph | Prevents authority fragmentation. |
| Deterministic Mutation Pipeline | Enables replayable semantic evolution. |
| Cascade Preview | Creates propagation intelligence before mutation executes. |

Together they steer RGE toward a semantic infrastructure kernel rather than
only CAD + ECS + rendering.

## 14. Near-theoretical maturity horizon

The "100 percent semantic runtime" source is deliberately aspirational. Literal
100 percent semantic coherence is probably unattainable in any living system:
the world, users, assets, plugins, and distributed collaborators are not
perfectly deterministic, complete, or semantically stable.

The useful question is not "how do we become perfect?" It is:

```text
how does semantic coherence keep scaling instead of collapsing?
```

The source frames the remaining gap as the hardest part of the architecture:
not rendering, syntax, or tooling, but semantic philosophy, distributed truth,
entropy resistance, ontology scaling, and runtime self-awareness.

Near-theoretical maturity themes:

| Theme | Meaning | Status today |
|---|---|---|
| Self-healing semantic runtime | Detect -> diagnose -> isolate -> repair semantic corruption. | Future recovery direction; diagnostics and snapshots are prerequisites, not the full system. |
| Runtime self-understanding | The runtime models its topology, instability regions, mutation density, pressure zones, and authority-fragmentation risk. | Future observability direction. |
| Semantic compression | Hierarchical abstraction for large semantic worlds without losing identity continuity, replayability, or propagation correctness. | Future scale direction. |
| Distributed semantic consensus | Coherence across users, offline branches, conflicting mutations, and distributed authority. | Post-single-machine semantic substrate; harder than ordinary networking. |
| Cross-domain semantic unification | CAD, AI, finance, logistics, and other domains coexist under shared law while preserving domain-local ontology freedom. | Long-horizon possibility, not v1 scope. |
| Predictive propagation intelligence | Preview evolves from "what breaks?" into "where is fragility forming?" | Future cascade-preview evolution. |
| Semantic economics | Observability and propagation need adaptive detail, priority, and level-of-detail so graph cost does not explode. | Future performance/governance pressure. |
| Temporal branching | Replay can ask "what if this mutation never occurred?" and inspect alternate semantic timelines. | Future replay-inspector direction. |
| Formal semantic physics | Conservation-like laws for identity, causality, authority, and propagation. | Philosophical horizon; not a current substrate. |
| Anti-entropy architecture | Coherence resists duplication, shortcuts, hidden state, and shadow projections structurally. | Existing doctrine pushes in this direction. |

The ultimate architectural form described by the source is:

```text
self-observable
self-replayable
self-governing
topology-aware
semantic runtime substrate
```

This is rare because most software optimizes for feature accumulation rather
than semantic coherence. RGE's direction is different, but that difference must
not become permission to overformalize early.

Strategic warning:

- Do not try to build all of this immediately.
- Overformalization can kill momentum.
- Abstraction can become infinite.
- Governance can consume execution.

Correct evolution path:

```text
working substrate
-> observed pain
-> formal law
-> gradual convergence
```

not:

```text
perfect theory first
```

This section is therefore advisory horizon-setting. It does not supersede the
architecture-freeze gate, the non-goals doctrine, or the implementation order.
It records what the semantic law set is trying to make possible over time.

## 15. Semantic runtime gap assessment

The rerating source attempted to score RGE numerically across semantic
authority, replayability, identity continuity, propagation intelligence,
observability, drift resistance, self-governance, and cross-domain coherence.
Those percentages are not carried forward here: doctrine-doc accretion does
not substitute for substrate work, and unmethodologized scores create future
drift risk.

Qualitative interpretation:

- RGE is moving away from ordinary engine architecture and toward semantic
  infrastructure architecture.
- The doctrine now names early forms of self-observability, self-governance,
  anti-entropy pressure, semantic-physics horizon, and runtime introspection.
- The remaining bottleneck is no longer primarily rendering, CAD algorithms,
  ECS shape, or execution machinery. The bottleneck is whether semantic
  coherence can survive real-world scale and entropy.

Remaining theoretical gaps:

| Gap | Why it prevents literal 100 percent coherence |
|---|---|
| Real-world semantic ambiguity | Reality is fuzzy, incomplete, and sometimes contradictory. |
| Human meaning drift | Semantics evolve socially over time. |
| Distributed authority paradoxes | Multiple truths emerge under scale, offline work, and conflicting mutation. |
| Computational limits | Perfect observability becomes too expensive. |
| Emergent ontology conflict | Domains evolve incompatible concepts. |
| Entropy pressure | Ecosystems naturally fragment through shortcuts, duplication, and hidden state. |

The final insight from the source matches the maturity horizon:

```text
building a system
where semantic coherence scales
instead of collapsing
```

Use this section as a review prompt: which semantic dimension does a proposal
strengthen or weaken? Do not turn it into a numeric maturity target.

## 16. Admission checklist

When a feature claims semantic significance, the design must answer:

1. What semantic fact is being introduced or mutated?
2. Which substrate is authoritative for that fact?
3. Which mutation surface changes it?
4. Which projections derive from it?
5. What identity survives topology-preserving or placement-only changes?
6. What happens under topology-changing mutation?
7. How can the state be replayed or reconstructed?
8. What dependency frontier propagates from the mutation?
9. What diagnostic reveals drift if the contract is violated?
10. Is cascade preview implemented, unknown, or deliberately out of scope?

If these answers are unclear, the feature is not ready to become semantic
architecture. It may still be an experiment, local helper, or projection, but
it must not claim authority.
