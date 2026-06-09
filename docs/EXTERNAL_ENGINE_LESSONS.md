# RGE — External-Engine Lessons (harvest ledger)

> **Purpose.** A capture backlog of patterns observed in comparable engines, each mapped to the RGE
> locus it could pressure-test. Mirrors kariyer's "automation for ideas" (capture → stage → promote),
> **adapted to RGE's architecture freeze**: design docs (`PLAN`/ADRs) and code change **only when a row
> is `promoted`, and promotion requires demonstrated implementation pressure, not forecast**
> (`PLAN` §0.6: post-v0.8 subsystems require ≥3 concrete reproducible failure scenarios). This file is a
> **reporter / backlog, not a gate** — it authorizes nothing. Its value is to pre-stage designs so that
> when pressure arrives, we build faster and better-informed.

## The five kernel pressures

Every engine kernel faces recurring *pressures*; comparable engines are donors of *solutions* to them.
Harvested lessons are organized by which pressure they address (per the OpenClaw↔RGE analysis,
2026-06-09):

1. **Composition** — heterogeneous, content-varying object state, attached and queried generically.
2. **Declarative content** — content expressible to tooling + runtime loaders without recompiles.
   *Nuance:* a **fixed-schema** data-driven format can precede composition; it is **scalable /
   heterogeneous** content that eventually forces composable objects.
3. **Subsystem boundaries** — volatile/specialized services (physics, audio, importers, scripting)
   behind a contract, so backend detail does not contaminate the kernel.
4. **Execution-time** — the sim-time model (fixed vs variable step; determinism). A **kernel-identity**
   choice, *not* secondary: it legitimately bifurcates on "do you need deterministic
   editor/replay/netcode?" (OpenClaw → variable, playable single-player; RGE → Fiedler fixed-step,
   deterministic editor/replay). Same pressure, different valid answer.
5. **Authority arbitration** *(multi-model engines only)* — when two models describe the same world
   (RGE: ECS runtime state vs CAD operator-graph geometry, bridged by `EntityCadMap`), deciding which
   model **owns truth**. This is *semantic governance*, not plugin hygiene — and RGE's self-named #1
   risk (semantic-authority fragmentation). **No single-model donor engine has a pattern for it** — it
   is the column where RGE cannot borrow blindly.

## Two-gate promotion (how a row graduates)

```
captured → mapped → [Gate A: phase] → staged → [Gate B: pressure §0.6] → promoted → adopted / rejected / n-a
```

- **Gate A — Prepare (phase / roadmap trigger).** When the mapped RGE locus is ~1 phase from opening:
  mine 1–2 donor engines and draft a concrete design sketch **in this ledger** → `staged`. Forecast-OK
  because it commits nothing (no code, no `PLAN`/ADR/type change) and is freely discardable.
- **Gate B — Promote (pressure, §0.6).** Only ≥3 concrete reproducible failure scenarios / a real
  consumer graduate a `staged` row to `promoted` (ADR + `PLAN` update, build authorized). The phase
  trigger can **never** reach this alone.

### Guardrails

1. Staging commits nothing; a `staged` row with no pressure sits indefinitely or is discarded.
2. **Anti-sunk-cost:** a polished staged design is *not* a reason to build — only pressure is.
3. No code prototypes on forecast (a prototype is substrate → waits for Gate B).
4. One file, richer status column; **no new lint, no ADR-for-the-process, no parallel authority.**
5. Record deliberate divergences (`n-a`) so an external pattern cannot re-litigate a settled RGE
   decision.

## Backlog

| ID | Lesson | Donor | Pressure | RGE locus | Promotion gate | Risk if misapplied | Status |
|----|--------|-------|----------|-----------|----------------|--------------------|--------|
| LES-001 | Cooperative `Process` chains (state machine + child-promote-on-success) as an interim task primitive | OpenClaw `ProcessMgr` | execution-time | `kernel/job-system` (v0 vocabulary, no executor) | both — Gate A: a consumer wants sequenced async work; Gate B: that consumer is real | cooperative scheduling can mask the need for a true parallel executor — keep it explicitly interim | mapped |
| LES-002 | Double-buffered, ms-budgeted event drain (re-queue overflow) | OpenClaw `EventMgr::VUpdate` | subsystem boundaries | `kernel/events` | pressure-only — needs evidence of queue overrun / backpressure | a central bus competes with RGE's typed channels + change-detection — do not recentralize comms | captured |
| LES-003 | LRU cache + refcount reclaim (`MakeRoom` / `FreeOneResource`) | OpenClaw `ResourceCache` (+ Bevy/Godot residency) | declarative content | `kernel/asset-streaming` + `asset-view` (stubs, Phase 4) | both — Gate A: Phase 4 nears; Gate B: a scene actually thrashes memory | OpenClaw's single-thread refcount reclaim ignores GPU residency + async — do not copy its threading model | mapped |
| LES-004 | Real archetype buckets vs single catch-all (both engines hit the full-scan / AoS wall) | OpenClaw (hot-component caching) + Bevy (archetypes) | composition | `kernel/ecs` (single catch-all archetype, full-scan queries) | pressure-only — needs an entity-count profile showing the cost | premature archetype migration adds complexity with no measured win — wait for the profile | pressure-pending |
| LES-005 | Physics behind an interface (`IGamePhysics` Strategy façade) | OpenClaw | subsystem boundaries | `physics` plugin | n-a | RGE already does physics-as-Tier-2-plugin (rapier, snapshot-participating); a façade would be a *parallel* mechanism | n-a |
| LES-006 | *(two-model truth ownership: ECS runtime state vs CAD operator-graph geometry)* | **— no donor —** | authority arbitration | `cad-projection` `EntityCadMap` | RGE-internal only — no external pattern to stage | **cannot be borrowed:** no single-model engine solves two-model truth ownership; RGE must design this itself (semantic-authority fragmentation = stated #1 risk) | open |

## Donor engines to harvest next

Bevy (archetype ECS, scheduler, change-detection — RGE's closest sibling), Godot (scene/resource model,
editor UX), Fyrox & Flax (Rust / C# editor engines), O3DE (Gem / component model), *Game Coding
Complete* (OpenClaw's parent). **Practice:** when a cavity nears Gate A, mine 1–2 donors for how they
solved *that specific pressure* → capture rows → let Gate B decide promotion.

## Reporter (deferred)

A `report` script (à la kariyer's `ideas/report.mjs`) that prints status counts + flags `staged` rows
whose RGE locus is under active work — **deferred until the ledger is big enough to earn one** (don't
build the tool on forecast; eat our own dogfood).

---

*Seeded 2026-06-09 from the OpenClaw ↔ RGE kernel comparison. History: the five-pressure frame was
synthesized across a Claude/Codex exchange under owner arbitration; `LES-005` is a recorded divergence,
`LES-006` is the no-donor authority-arbitration row.*
