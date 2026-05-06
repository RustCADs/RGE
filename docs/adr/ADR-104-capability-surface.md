# ADR-104: Capability surface for CAD operators

| Status | Accepted 2026-05-08 (preliminary — doc-comment-only canonical form; full struct deferred per trigger condition below) |
|---|---|
| Date | 2026-05-08 |
| Deciders | (RGE architecture review) |
| PLAN references | §1.5.4.4 (CAD kernel non-equivalence + capability surface), §1.5.4 (cad-core), §13.2 / §13.6 (CAD validation gates) |
| ADR references | ADR-098 (topology lineage substrate — `output_labeled_when_input_labeled` field), ADR-112 (cad-core Boolean CSG library — first capability-surface entry), ADR-113 (deferred — truck cad-native backend) |
| Implementation phase | Phase 7 — CAD Spike (carries the kernel-non-equivalence doctrine) |

## Context

PLAN §1.5.4.4 names a "CAD kernel non-equivalence doctrine": kernels are not interchangeable, and capability differences (notably "boolean robustness under tolerance") are surfaced explicitly rather than papered over. ADR-112 (D-Boolean) bit on this doctrine concretely — csgrs is BSP-without-exact-arithmetic, so its Boolean output is *not* robust under tolerance the way truck (eventual cad-native) would be. ADR-112 §"Capability surface entry" sketched a `KernelCapabilities` struct and committed to declaring `boolean_robust_under_tolerance: false` on the csgrs-backed Boolean operator.

The ADR-112 sketch landed as a doc-comment on `BooleanOp` rather than a real struct. This was deliberate: the architectural-debt registry's "Deferred (defensible until trigger fires)" entry holds that the struct can wait until a downstream consumer needs to programmatically filter operators by capability OR until a second CAD kernel implementation lands (truck, ADR-113-deferred). Until then, doc-comments are the canonical source of truth and consumers (none yet) would read them by hand.

This ADR formalises the doc-comment-then-struct rollout. It captures (1) the decision space — what gets declared, why doc-comments-first, what the trigger conditions are — and (2) the canonical name + initial fields for the struct so when the trigger fires the materialisation is mechanical.

## Decision

**Declare `KernelCapabilities` as a doc-comment-canonical capability surface today, materialise it as a real `struct` + `Operator::capabilities()` trait method when a trigger condition fires.**

Two sub-decisions follow.

1. **Doc-comment-only canonical form is acceptable until trigger.** Each operator's module-level (or impl-level) doc-comment contains a `# Capability surface (per ADR-104)` block listing the capability fields and their values for that operator. Today's only non-trivial entry is `BooleanOp` — its `capability surface` doc-block lists `boolean_robust_under_tolerance: false`, `healing_strategies: none`, `deterministic_triangulation: true (gated by 200-iter soak)`, `t_junction_handling: false (csgrs upstream TODO)`. Cuboid / Extrude / Revolve / Transform inherit the trivial defaults (everything `true`/`N/A`) and currently document only their `output_labeled_when_input_labeled` value.

2. **Trigger conditions for materialisation.** The struct must materialise when ANY of:
   - **Editor-UI operator-picker.** When `editor-ui` (or any future caller) needs to programmatically filter the operator list by capability ("show only operators with `boolean_robust_under_tolerance: true`", "show operators with deterministic triangulation"), the doc-comment-only form is no longer sufficient. The struct + trait method becomes load-bearing.
   - **Second CAD kernel.** When truck lands (ADR-113-deferred) as the `cad-native` backend, the workspace has *two* Boolean implementations with different capability profiles. Without a struct, the operator-picker (and any cache-keying consumer) has to pattern-match on `OpKind` to discriminate, which is exactly what the capability surface was supposed to abstract. The struct must materialise to keep the abstraction intact.
   - **Capability-aware tessellation cache.** The cache currently keys on `structural_hash + Tolerance + labeled-state`. A future requirement to key on capability (e.g. "this cached entry was produced by a non-robust operator; invalidate when robustness gates change") fires materialisation.

Until any of those fires, the doc-comment-only form is the canonical source.

### Initial field set (canonical names; materialise as-is when trigger fires)

```rust
// crates/cad-core/src/operators/mod.rs (when struct materialises)
pub struct KernelCapabilities {
    /// True iff the operator is robust under tolerance (e.g. exact-arithmetic CSG).
    /// BooleanOp = false (csgrs is BSP without exact arithmetic).
    pub boolean_robust_under_tolerance: bool,

    /// True iff the operator's triangulation is bit-deterministic given
    /// deterministic input ordering.
    /// All current operators = true (verified via 200-iter soak for BooleanOp,
    /// 100-iter soak for cross-substrate operators).
    pub deterministic_triangulation: bool,

    /// True iff the operator handles T-junctions in its output.
    /// BooleanOp = false (csgrs upstream TODO; documented in ADR-112).
    pub t_junction_handling: bool,

    /// True iff the operator accepts concave input polygons / surfaces.
    /// Extrude = false, Revolve (partial) = false, Revolve (full) = true,
    /// Cuboid = N/A (closed-form generative).
    pub concave_input_supported: bool,

    /// Number of upstream tessellations the operator's `evaluate` expects.
    /// Already exposed via `Operator::arity()`; included for completeness.
    pub arity: u32,

    /// True iff the operator preserves face labels through evaluation when
    /// any of its inputs is labeled. Default: `inputs.iter().any(|b| *b)` —
    /// any labeled input ⇒ labeled output. Operators that strip labels
    /// (e.g. TransformOp = false) override.
    /// See ADR-098 / `Operator::output_is_labeled`.
    pub output_labeled_when_input_labeled: bool,
}

// trait method (when struct materialises)
impl Operator for ... {
    fn capabilities(&self) -> KernelCapabilities { ... }
}
```

The fields are bool-or-enum flags, all const-derivable from operator type. No allocation, no per-call cost.

## Consequences

### Positive

- **Non-equivalence doctrine is surfaced today.** `BooleanOp`'s doc-comment already declares `boolean_robust_under_tolerance: false` per ADR-112 §"Capability surface entry". Future maintainers reading the operator's source see the capability gap rather than discovering it via test-failures.
- **Materialisation is mechanical when the trigger fires.** Field set is canonical; trait shape is documented; no design work remains. Materialisation dispatch is bounded.
- **Doc-comment-only form keeps the design honest.** Operators without meaningful capability deltas (Cuboid / Extrude generative path / Revolve trivial cases) don't accrete boilerplate. When non-trivial capabilities exist, the doc-comment is the exact place a maintainer looks. When they don't, no friction.
- **Trigger conditions are concrete.** "Editor-UI operator-picker" is a real upcoming dispatch; "second CAD kernel" is the truck-adoption dispatch ADR-113 already foresees; "capability-aware cache" is a known future cache-extension. Each trigger has a clear next-step.

### Negative / risks

- **Doc-comment-only form is not machine-checkable.** A future operator implementer can forget to update the capability block. Mitigation: a future architecture-lint can scan operator modules for the `# Capability surface (per ADR-104)` header and warn on its absence (deferred until materialisation pressure exists).
- **Initial field set may need extension at materialisation time.** Future operators (lofts, sweeps, fillets) may surface capabilities not in the initial six (e.g. `tangent_continuity_supported`, `surface_evaluation_quality`). Mitigation: add fields at materialisation; old code paths default to documented values.
- **Two sources of truth during the doc-comment-only phase.** PLAN §1.5.4.4 names some capability dimensions; this ADR names six concrete fields; ADR-112 names the BooleanOp values. If they drift, the canonical source is **this ADR** (until materialisation, when the struct itself becomes canonical and PLAN / ADR-112 should be updated to point at it).

### Mitigations

- **ADR-112 §"Capability surface entry" cross-link.** ADR-112 declares Boolean's specific values; this ADR declares the framework. Each ADR cites the other.
- **ADR-098 cross-link for `output_labeled_when_input_labeled`.** That field is the trait-method form of the lineage substrate's `Operator::output_is_labeled` invariant; ADR-098 documents the runtime behaviour, this ADR documents the capability-surface form.
- **HANDOFF.md / Status.md tracker entries.** Both list "ADR-104 doc-comment-only deferred" with the trigger conditions; a maintainer searching for "what's still deferred" finds the entry.

## Alternatives explicitly NOT chosen and why

**Materialise the struct today, even without a downstream consumer.** This was the structurally cleanest option but has no payoff: with one CAD kernel and no operator-picker, the struct's read sites are zero. Materialising means a trait method on every existing operator (boilerplate) and a cache-key extension that defends against a non-existent collision class. The architectural-debt registry's "Deferred (defensible until trigger fires)" entry already holds this defence; this ADR codifies it.

**Skip the capability surface entirely; pattern-match on `OpKind` for filtering.** `OpKind` is a discriminant; capabilities are *properties* of operators that may differ across kernels of the same `OpKind` (e.g. csgrs-Boolean vs. truck-Boolean). Pattern-matching on `OpKind` couples capability awareness to operator identity, which violates the kernel non-equivalence doctrine of §1.5.4.4: callers should ask "does this operator support X?" not "which kernel is this?". Pattern-matching is the right form for type-level dispatch (e.g. `as_operator()` in `OperatorNode`), wrong for capability filtering.

**Use a `HashMap<&'static str, CapabilityValue>` instead of a typed struct.** This was discussed in the architectural-debt registry exchange and rejected: typed fields surface compile-time errors when an operator forgets a capability or when a new capability is added; a HashMap defers everything to runtime, hides typos, and breaks `match`-exhaustiveness. The struct is strictly safer.

**Make capabilities runtime-mutable (e.g. `KernelCapabilities { … }` returned by a method that consults runtime state).** All current capabilities are determinable at compile time from the operator type. Runtime-mutable capabilities would force the cache to re-key on every call, which is exactly the cost ADR-112's structural-hash recipe avoids. Keep capabilities `const`-derivable.

## Implementation guidance

### Doc-comment template (today, canonical form)

```rust
//! `MyOperatorOp` — <one-line description>
//!
//! ...
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: <true|false> — <one-line rationale>
//! * `deterministic_triangulation`: <true|false> — <one-line rationale + gate>
//! * `t_junction_handling`: <true|false> — <one-line rationale>
//! * `concave_input_supported`: <true|false|N/A> — <one-line rationale>
//! * `arity`: <N>
//! * `output_labeled_when_input_labeled`: <true|false> — <one-line rationale; default if any input labeled>
```

Operators that inherit all defaults (every field `true`/`N/A` and `output_labeled_when_input_labeled` is the default `inputs.iter().any(|b| *b)`) MAY omit the block; operators with any non-trivial capability MUST include it.

### Materialisation recipe (when trigger fires)

1. Define `KernelCapabilities` in `crates/cad-core/src/operators/mod.rs` with the six canonical fields.
2. Add `fn capabilities(&self) -> KernelCapabilities` to the `Operator` trait. No default impl — every operator must declare. (Pattern: same as `arity`, `output_is_labeled` already in the trait.)
3. For each operator, copy the doc-comment-canonical values into the trait-impl `capabilities()` method.
4. Update ADR-112 §"Capability surface entry" to point at the trait method rather than the struct sketch.
5. Add the trigger consumer (operator-picker, second-kernel discrimination, cache-key extension) — that's why we materialised.
6. Add the architecture-lint that scans for missing trait-impl coverage (replaces the today's doc-comment scanning).

### Test recipes (for the materialisation dispatch)

1. Every operator declares `capabilities()` (covered by trait dispatch — uncovered operators fail to compile).
2. `BooleanOp::capabilities().boolean_robust_under_tolerance == false` (locks in ADR-112's declaration).
3. `CuboidOp::capabilities().concave_input_supported` is N/A-equivalent (the field is `true` because the closed-form generative path doesn't reject any input).
4. `output_labeled_when_input_labeled` matches `Operator::output_is_labeled` for representative inputs (ensures the capability-surface field doesn't drift from the runtime invariant).

## Followups / open questions

- **Trigger to materialise.** Tracked in HANDOFF.md / Status.md. Materialisation is bounded; expect the trigger to fire when editor-ui operator-picker lands or when truck adoption begins.
- **Tolerance-budget propagation.** Per ADR-112 §"Capability surface entry": when callers pass a tolerance budget down the operator tree, the capability surface needs a `tolerance_budget_aware: bool` field. Defer until tolerance-aware callers exist.
- **Capability-aware tessellation cache.** Today the cache keys on `structural_hash + Tolerance + labeled-state`. A future cache-extension folds capability-fingerprint into the key so that swapping a non-robust operator for a robust one (e.g. csgrs → truck) invalidates the cache. Defer until the second-kernel dispatch.
- **Architecture-lint for missing capability blocks.** Adds a check that every operator module has the doc-comment block (or, post-materialisation, a real trait-impl). Defer until materialisation.
- **PLAN §1.5.4.4 cross-update.** When the struct materialises, PLAN should be updated to name the canonical fields rather than carrying the abstract "capability surface" language. Tracked as a documentation-cleanup follow-up.

## References

- PLAN.md §1.5.4.4 (CAD kernel non-equivalence + capability surface)
- IMPLEMENTATION.md §7 (Phase 7 — CAD Spike), §13.6 (CAD validation gates)
- ADR-112 §"Capability surface entry" (the first concrete capability-surface declaration; csgrs-backed Boolean)
- ADR-098 §"Operator trait extension" (`output_is_labeled` invariant — `output_labeled_when_input_labeled` is its capability-surface form)
- ADR-113 (deferred — truck cad-native backend; second-kernel trigger)
- `crates/cad-core/src/operators/boolean.rs` `# csgrs features / capability surface` block (canonical BooleanOp declaration)
- `crates/cad-core/src/operators/mod.rs` (`Operator` trait — `arity`, `output_is_labeled`, future `capabilities()`)
- Architectural-debt registry: "Deferred (defensible until trigger fires)" — `KernelCapabilities` doc-only entry
