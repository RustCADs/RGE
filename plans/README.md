# RGE Plans

Architecture and implementation plans for RGE. Frozen at v0.8 (architecture); execution-time docs.

## Index

| File | Purpose | Frozen? |
|---|---|---|
| [`PLAN.md`](./PLAN.md) | Architecture v0.8 — pillars, moat, constitution, three-tier model, all subsystem commitments | **Yes** (constitutional) |
| [`IMPLEMENTATION.md`](./IMPLEMENTATION.md) | Recommended implementation order (de-risking-driven) — 10 phases over weeks 1–48+ | Living |
| [`fileandfolderstructure.md`](./fileandfolderstructure.md) | Workspace skeleton spec — folder layout encodes architecture | Living |

## Reading order for new contributors

1. **`PLAN.md` §0** (pillars + moat + constitution) — what we're building and why
2. **`PLAN.md` §1** (architecture commitments) — the rules that don't bend
3. **`IMPLEMENTATION.md`** (sequential phase order) — what gets built first and what's behind which gate
4. **`fileandfolderstructure.md`** (workspace skeleton) — where things live on disk

## Adding new plans

The architecture is frozen at v0.8. New first-class subsystems require the §0.6 freeze-policy gate (four conditions: demonstrated implementation pressure, 3+ reproducible failure scenarios, cost/benefit vs alternatives, justification why a smaller primitive wouldn't suffice).

ADR additions go in [`../docs/adr/`](../docs/adr/), not here. This folder holds the load-bearing plan documents themselves.
