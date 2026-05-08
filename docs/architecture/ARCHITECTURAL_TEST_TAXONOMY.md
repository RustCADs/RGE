# ARCHITECTURAL_TEST_TAXONOMY

| Status | Doctrine-tier v0; Phase 1 Executable Governance Architecture second deliverable. Classification of the workspace's existing test shapes plus criteria for assigning a candidate test to the right shape. Binding for shape decisions, not for any single test. The doctrine answers "which architectural invariants deserve a dedicated test, and at which shape?" — it does NOT prescribe new test infrastructure, harnesses, or coverage targets. |
|---|---|
| Audience | Reviewers asked whether a proposed test belongs as an architecture-lint fixture vs an integration smoke vs a per-crate unit test; dispatch authors weighing whether a coverage gap warrants a regression test or doctrine prose; subsystem authors deciding whether to pin a property at the test level vs the type level vs leave it on the doctrine layer; future doctrine authors landing the next governance commitment. |
| Companion to | The 9-lint architecture-enforcement suite (`docs/§18/ARCHITECTURE_LINTS.md`) and its 69 fixture tests; `INVARIANT_ENFORCEMENT_STRATEGY.md` (Phase 1 first deliverable — same governance discipline applied to enforcement-mechanism choice rather than test-shape choice); ADR-116 canary protocol (the most recent architectural-test pattern with a per-canary retroactive harness). |
| Sibling docs | `docs/architecture/INVARIANT_ENFORCEMENT_STRATEGY.md` (the parent doctrine — invariant graduation feeds test-shape decisions), `docs/architecture/REACTIVE_INVALIDATION.md` (the load-bearing invariants whose verification surface this doctrine governs), `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` (ownership rules whose architectural-test exemplars are cited in §11), `docs/architecture/NON_GOALS.md` (closest sibling in spirit — "we will NOT" framing applied to test scope rather than runtime scope). |
| Reference impls | `tools/architecture-lints/tests/fixtures/<lint>/<scenario>/` (the 69-fixture test substrate — see §11 probe 1); `kernel/plugin-host/src/host/host_tests/{registration,lifecycle,diagnostics,panic_recovery,resource_leak}.rs` (the audit-3 sub-module split — §11 probe 6); `tests/cross_substrate_determinism.rs::pie_three_participant_round_trip_50_iter` (cross-substrate integration soak — §11 probe 2); the 5 plugin canaries (`cad-projection` / `gfx` / `physics` / `audio` / `editor-ui` — §11 probe 3); the 4 PluginError × PluginPhase auto-emit cells (§11 probe 4). |

> *Doctrine-tier doc — meta-rule on which architectural invariants warrant a test and at what shape. Does not itself add tests. The deliverable is reasoning about test-shape choice, not a coverage backlog.*

## 1. Why this doctrine exists

`INVARIANT_ENFORCEMENT_STRATEGY.md` answered "which stabilized truths deserve mechanized enforcement?" and named tests as one of five enforcement tiers. It deliberately did not subdivide that tier. This doctrine subdivides it.

The workspace has accumulated multiple distinct test shapes — architecture-lint fixtures, cross-substrate integration soaks, dogfood-rule canaries, sub-module unit tests, regression cells, SemVer-hardening pattern-match tests — each shape carrying different costs, different ossification profiles, and different failure modes. Conflating them under "tests" loses the distinctions that drive shape choice. A coverage-gap closure is not the same kind of test as a fixture lint binary; the cost calculus differs.

The risk this doctrine guards against is **shape drift**: a test landing at the wrong tier (a fixture binary that should have been an integration soak, an integration test that should have been a unit test, a regression cell that should have remained doctrine prose). Shape-drift is not flagged by the architecture-lint gate or by the workspace test counter; it is governance debt that surfaces only as maintenance pain over time. Sub-1's asymmetric weighting on costs and failure modes carries through here: the largest near-term risk is over-testing at the wrong shape, not under-testing of architectural invariants.

The cross-review-#10 framing applies again — *"architectural pressure repeatedly failed to force premature abstraction expansion"* — and this doctrine extends that discipline into the test substrate. The graduation question becomes: when does an architectural invariant warrant a dedicated test, and which test shape minimizes ossification of the wrong thing? Most candidate tests answer "no" or "stay at the lighter shape"; that is feature, not gap.

## 2. Vocabulary — test-shape distinctions

Six terms recur across this doctrine and the sub-1 parent. Conflation degrades the §5 graduation criteria into "did we add coverage?" — itself a §6.3 false-confidence symptom. The distinctions below are load-bearing for every other section.

| Concept | Meaning |
|---|---|
| **Architectural test** | A test whose primary purpose is verifying an architectural invariant — a cross-crate dependency rule, a substrate contract, a deterministic property, an authority boundary. The thing under test is the *architecture*, not the function. |
| **Semantic test** | A test verifying functional / algorithmic correctness within a crate — does this function compute the right answer? Architectural and semantic tests overlap in form (both use `#[test]`); they differ in *what they assert*. |
| **Fixture test** | A test built around a deliberately-shaped input fixture (e.g. `tools/architecture-lints/tests/fixtures/forbidden_dep/violation/` carries a Cargo.toml that violates the rule under inspection). The fixture is the test's substrate; the assertion is "the lint fires / does not fire on this fixture." |
| **Regression test** | A test pinning a specific past-bug closure or coverage-gap closure — the four PluginError × PluginPhase auto-emit cells added 2026-05-08 are the canonical recent example. The test's purpose is "this used to be wrong; pin it correct." |
| **Smoke / integration test** | A small end-to-end test verifying cross-substrate composition works at all (tessellation cache → operator graph → projection → snapshot round-trip). Smokes are not exhaustive; they are the workspace's "is the wiring intact?" assertion. |
| **Canary test** | A test exercising the dogfood-rule equivalence path (a Tier-2 plugin proves the same `Plugin` trait Tier-3 plugins use is structurally sufficient). Canaries are architectural — the assertion is "the trait holds under real consumer pressure." |

The most important non-equivalence: **assertion form ≠ assertion target.** Two tests using identical `assert_eq!` machinery may be doing entirely different jobs — one asserting that a function returns the right value (semantic), the other asserting that a substrate contract is structurally honored (architectural). Vocabulary discipline prevents this conflation from degrading the workspace's test substrate into "all tests are equivalent because they all run via `cargo test`."

A second non-equivalence: **passing test ≠ enforced invariant.** A passing test asserts only what the test exercises — the §6.3 false-authority cost. Treating green-bar as the authority surface for an invariant is the most common form this confusion takes, and the §11 probes name several places the workspace has resisted that pattern.

## 3. Test-class taxonomy — what is the test asserting?

Workspace tests partition into seven classes by the kind of claim they assert. The classes are not exhaustive (a test can assert claims in multiple classes); they are diagnostic — they help locate where a candidate test fits.

- **Structural-authority assertion.** "Crate X's import of crate Y is forbidden / required." Examples: the 9 enforcement architecture lints' fixture tests; `kernel-isolation` overlap detection; the `forbidden-dep` rule fixtures. Tests of this class belong as architecture-lint fixtures because the violation manifests as source-shape, and the source-shape is what the lint walks.

- **Determinism assertion.** "Repeated execution produces byte-identical output." Examples: `cross_substrate_determinism::pie_three_participant_round_trip_50_iter`; the 1000-tick physics replay; D-Boolean 200-iter union-difference soak; `effective_hash_and_label` recursion fold. Tests of this class belong as integration soaks because the property is observational over many runs and bytes-out-equal-bytes-in is the canonical assertion shape.

- **Substrate contract assertion.** "Implementations of trait X must satisfy property Y." Examples: ADR-116's retroactive `*_plugin_impls_canary_protocol` tests across all five canaries (`cad-projection` / `gfx` / `physics` / `audio` / `editor-ui`); the `SnapshotComponent` registry round-trip; failure-class declarations. Tests of this class belong as per-trait suites that every implementor's crate carries (canary pattern), or as a workspace-level walker (failure-class lint).

- **Coverage-gap closure.** "This previously-untested invariant variant is now pinned." Examples: the four PluginError × PluginPhase auto-emit cells (2026-05-08 — see §11 probe 4); the ADR-116 retroactive harness when first applied. Tests of this class are regression cells; they live in the consumer crate's test sub-module and document the gap closure in their doc-comment.

- **API-shape assertion.** "Public type X exposes the documented surface and refuses other inputs." Examples: `Tolerance::new(t)` rejecting non-finite or non-positive; `Polygon2D::new(...)` rejecting `<3` points; `IoRequestId::from_bytes` round-trip. Tests of this class belong as inline unit tests at the type's foot — they are the type author's responsibility, not the workspace's.

- **SemVer-hardening assertion.** "Cross-crate consumers compile correctly under `#[non_exhaustive]` patterns." Examples: `plugin_error_non_exhaustive_pattern_match_compiles`; `priority_non_exhaustive_pattern_compiles_with_wildcard`; `op_kind_non_exhaustive_pattern_match_compiles`. Tests of this class are minimal pattern-match probes that exist solely so the workspace breaks if a future variant addition silently becomes a breaking change.

- **Documented-behavior assertion.** "The behavior described in the doc-comment matches the runtime behavior." Examples: doctests; the inline examples in `kernel/graph-foundation::Graph::node_count` doc-comment under ADR-115 phase-1. Tests of this class are doctests — they belong inline in the doc-comment, not in a test module.

The taxonomy intentionally has no single "right" shape per class — that is §4. The class label suggests the *kind* of claim; the shape choice depends on cost.

## 4. Test-shape taxonomy — which mechanism for which class?

Five shapes by ossification cost, ordered from most ossifying to least. Each shape has characteristic strengths and weaknesses; the §5 graduation question maps a candidate test onto a shape.

- **Architecture-lint fixture binary.** A dedicated `tests/fixtures/<lint>/<scenario>/` directory plus an integration-test driver that runs the lint against the fixture. The most ossifying shape: the fixture itself becomes substrate (a real Cargo.toml / source tree), and the lint algorithm becomes coupled to the fixture's exact layout. Strengths: highest verisimilitude (the lint runs against real-shaped code); per-rule provenance via fixtures-as-substrate; CI-visible. Weaknesses: maintenance debt as fixture conventions drift; deleting a lint requires deleting the fixture set; fixture-test divergence from real workspace code is a common form of rot. Reference: 69 fixture-test binaries across `tools/architecture-lints/tests/`.

- **Cross-substrate integration soak.** A `tests/<area>.rs` integration test that wires multiple substrates together and runs an assertion over many iterations or many input shapes. Strengths: catches cross-substrate composition failures the per-crate suites can't; the soak count anchors the determinism claim. Weaknesses: slow (the 50-iter PIE test runs 50× more than a unit-tier round-trip); ossifies the iteration count (50 is incidental, not load-bearing); pins the substrate composition shape, so swapping a substrate breaks the test. Reference: `cross_substrate_determinism.rs`; `boolean_panic_recovery.rs`.

- **Per-trait canary suite.** A small canary plugin / impl per substrate-family (CAD-graph / GPU / physics-world / audio / editor-ui), each carrying its own test smoke that exercises the full lifecycle through the shared trait. Strengths: structural proof that the trait scales across resource families; cheap to extend (one new canary = one new substrate proof point); ADR-116 pattern keeps each canary minimal. Weaknesses: every canary is N+1 substrate to maintain; the canary's existence pressures every Tier-2 family to host one; over-extension dilutes the proof. Reference: 5 canaries to date (`cad-projection` / `gfx` / `physics` / `audio` / `editor-ui`).

- **Per-crate unit-test sub-module.** Tests grouped by orchestrator-side concern in `src/<area>/<area>_tests/{registration, lifecycle, diagnostics, panic_recovery, resource_leak}.rs`. Strengths: keeps test code close to source under test; sub-module organization scales beyond the 1000-line single-file cap; per-concern grouping makes coverage gaps visible by inspection. Weaknesses: the split-when-needed discipline can drift into split-by-default cargo-cult; over-decomposition obscures the test surface. Reference: `kernel/plugin-host/src/host/host_tests/` (audit-3 split).

- **Inline file-foot unit tests.** A `#[cfg(test)] mod tests` block at the foot of the source file under test. The least ossifying shape: tests live with the code, get deleted with the code, are seen by the author every time the file is touched. Strengths: zero coordination cost; refactor-friendly; low ceremony. Weaknesses: bloats the file; pressures the 1000-line cap; can hide cross-cutting tests that should have been promoted up the hierarchy. Reference: every `crates/cad-core/src/operators/<op>.rs` carries 17–22 inline tests at the foot.

The shapes are not a hierarchy of correctness — they are a hierarchy of **coupling strength traded against locality**. A correctly-classified test at the inline shape is a stronger architectural commitment than an incorrectly-classified one at the fixture-binary shape, because the latter ossifies an incidental fixture layout into the workspace's test substrate.

## 5. Graduation criteria — when an invariant earns a dedicated test

Deliberately short and conservative. The default answer is **no test** — the invariant lives at the doctrine, ADR, or type level. A test is added when ALL of the following hold:

1. **The invariant has been violated more than once.** A single failure is anecdote; two or more is pattern. The four PluginError × PluginPhase cells were graduated because the audit-2 backlog flagged them as a pattern (4 cells had no coverage), not because one cell broke.
2. **The violation is silent at compile time.** If the type system or the architecture-lint gate already prevents the violation, a redundant test pins the same shape twice and adds maintenance debt without adding coverage. The Plugin trait's `Send + 'static` bound does not need a test asserting Send-ness.
3. **The test's assertion is structural, not incidental.** A test asserting "Plugin Y emits exactly 3 diagnostics on init failure" has pinned an incidental count; one asserting "Plugin Y emits at least one diagnostic with severity=Error" has pinned the structural property. Incidental assertions are the §6.4 ossification cost in disguise.
4. **The right shape is unambiguous.** If the candidate test could equally well be a fixture binary or an integration soak or an inline unit test, the graduation question hasn't been answered; reach for prose first.

The criteria are AND, not OR. A candidate that satisfies three of four does not graduate; it stays at the parent tier (typically prose or type-level). Walk every cost in §6 against the candidate; **each cost is a veto, not a vote.** This mirrors sub-1's graduation discipline exactly because the underlying risk-priority is identical: over-testing at the wrong shape is a worse outcome than under-testing of an architectural invariant whose authority is doctrinal.

## 6. Costs of architectural tests

Seven costs. The longest section of this doctrine by intent — the asymmetry mandate carries through from sub-1.

### 6.1 Test-substrate ossification

An architectural test pins SHAPE, not just behavior. The 50-iter PIE soak pins 50 because that is the count someone wrote; refactoring the cross-substrate composition to add a fourth participant requires re-deriving why 50 still suffices. The shape becomes substrate: removing it requires explanation, even when it should not.

### 6.2 Maintenance debt

Tests need updating as the workspace evolves. The 69 architecture-lint fixture-test binaries each carry a fixture-tree that must remain consistent with the lint's evolving algorithm; cargo-toml-shape changes upstream cascade into fixture updates. Maintenance debt grows superlinearly with test count because cross-test interactions surface only on workspace changes.

### 6.3 False authority

A passing test asserts only what the test exercises. A `forbidden_dep::cad_core_cannot_import_editor_ui` lint fixture tests only the path it walks; the absence of a fixture for "cad-core cannot import gfx" does NOT mean the rule is unenforced (it is — the lint walks all imports), but a future contributor reading only the fixture set may believe coverage is exhaustive. Treating the test as authoritative for the invariant is the §11 probe 8 anti-pattern instance.

### 6.4 Implementation coupling

Tests pin specific implementation shapes that may need to evolve. A test asserting "Plugin Host emits diagnostics in registration order" couples the test to the iteration order of `BTreeMap<PluginId, PluginRecord>` as a side-effect of plugin-id naming; refactoring the plugin host to use insertion-order iteration breaks the test even though the architectural invariant ("plugin-fatal isolation") is unchanged.

### 6.5 Refactor friction

Restructuring code requires restructuring matching tests. The audit-3 host.rs split (1766L → 695L production + 1071L distributed across `host_tests/{registration, lifecycle, diagnostics, panic_recovery, resource_leak}`) was a clean architectural win — but it required moving every test fixture and every `use super::super::*` import. The friction was low because the tests were already organized by concern; if they had been organized by Plugin lifecycle method instead, the split would have required reorganizing the tests too.

### 6.6 Test discoverability rot

Test names and comments describe behavior at the moment they were written. `tick_all_emits_warning_for_contract_violation` was correct when it landed; if `ContractViolation` is later promoted to Error severity (a doctrine change), the test name becomes a lie until manually updated. Comment rot is silent — `cargo test` does not flag stale doc-comments on test functions.

### 6.7 Test-substrate gravity

A test substrate (fixture-binary harness; canary-protocol harness; soak-loop harness) exerts gravity on future invariants — once the substrate exists, it pulls candidate invariants toward itself. This is the §7.4 gravity failure mode in cost form: the existence of `tools/architecture-lints/tests/fixtures/` makes "add a fixture" feel cheaper than the §5 walk-every-cost criterion would conclude. Costs decoupled from §5 weigh against the workspace's long-term health.

### 6.8 Cost walk closing

The cost walk is binding for graduation: **each cost is a veto, not a vote.** A candidate test that triggers any of §6.1 through §6.7 in a non-trivial way does not graduate, even if all four §5 criteria are met. The opposite of cost-aware graduation is "we have time to write the test, so we should" — that path produces the workspace's first test-substrate erosion, and §7 names what that erosion looks like at scale.

## 7. Failure modes of over-testing

Seven system-level failure modes. Counterweight to §6's per-test costs: §6 answers "what does this test cost?"; §7 answers "what does too many of them cost the workspace?"

### 7.1 Test substrate becomes the architecture

The shape of the test framework dictates the shape of the code under test. If the canary-protocol harness assumes Pattern A (straight-line tick) and Pattern B (lazy-build-on-first-tick), a Pattern C plugin lands as awkward shoehorning — the harness ossified the pattern set. **The signal:** new substrate work is described in terms of "fits the harness" rather than "fits the architecture."

### 7.2 Test-driven false confidence

A 1798-test green bar implies coverage; coverage was only ever what the tests exercised. The four PluginError × PluginPhase cells were uncovered for months under green-bar; the gap was real, the signal was absent. **The signal:** "all tests pass" replaces "the architecture is sound" in dispatch closures.

### 7.3 Flaky-test triage replaces real verification

A test that fails 1-in-50 runs becomes the workspace's most expensive test even when its assertion is correct, because every triage cycle costs more than the test asserts. Workspaces accumulate flaky tests faster than they retire them; the failure mode at scale is `cargo test --no-fail-fast` becoming the workspace's default because deterministic CI no longer exists. **The signal:** retry-on-flake commits in `change.md`.

### 7.4 Test-substrate gravity at scale

§6.7 named the per-test gravity; at scale the workspace develops a test-substrate culture — new invariants are landed by writing tests, not by reasoning about graduation. This is the meta-form of premature mechanization: the substrate's existence pressures the workspace toward over-mechanization. **The signal:** dispatch briefs default to "add a test" without articulating which §3 class the invariant fits.

### 7.5 Test-as-spec

The test becomes the canonical authority surface for the invariant — the §6.3 false authority promoted to architectural law. Once a contributor consults the test to learn what is true (rather than the doctrine), the doctrine has been demoted; updating the doctrine without updating the test loses the doctrine, and updating the test without updating the doctrine creates the §6.6 rot. **The signal:** doctrine docs cite test names instead of the inverse.

### 7.6 Architecture-test gating overuse

Every PR must add a test, even when the change is doctrinal, even when the type system already prevents the change from being wrong. The gate becomes ceremonial; its information content drops. **The signal:** PR templates that require "list new tests" with no equivalent doctrine prompt.

### 7.7 Inability to evolve the test substrate

Once enough tests depend on a fixture-tree shape, evolving that shape is prohibitively expensive — the workspace fossilizes around the fixture pattern. The 69-fixture lint substrate is healthy today because it is bounded; if it doubled, swapping the lint algorithm (e.g. from string-based to AST-based) would require updating 138 fixtures. **The signal:** PRs proposing fixture-tree changes are bundled with "this requires updating N fixtures" estimates that exceed the proposed change's intrinsic size.

The seven modes feed each other: gravity (§7.4) accelerates substrate-becomes-architecture (§7.1); test-as-spec (§7.5) accelerates rot (§6.6); flaky triage (§7.3) accelerates discoverability rot. The reinforcement cycle is the load-bearing risk, not any single mode.

## 8. Why selective non-coverage is healthy

**Lack of test coverage does not imply lack of importance.** This is the test-shape analog of sub-1 §8's vocabulary-discipline opening: an invariant can be load-bearing for the workspace and remain untested forever. The ownership rule "renderer NEVER owns authoritative geometry" (`SCENE_EXTRACTION_CONTRACT.md` §3.3) is one of the workspace's strongest architectural commitments and has no dedicated test — the absence of a callable from `gfx` that mutates `CadGraph` is observable by inspection, the `forbidden-dep` lint blocks the import direction, and the doctrine holds the truth claim. Adding a test that "asserts no callable from gfx mutates cad-graph" would re-state via runtime probe what the imports already prove structurally; the test would ossify the proof while adding maintenance debt.

The sub-1 `importance ≠ executability` distinction extends here: **importance ≠ test coverage.** A green-bar workspace with 100% line coverage can still violate every architectural commitment in this doctrine if the tests cover the wrong thing. Coverage-as-quality-metric is a §7.2 false-confidence specialization.

Selective non-coverage is healthy when:
- **The invariant is structurally enforced.** Imports, type bounds, lints — these prevent the violation at compile time. A test would re-assert what compilation already requires.
- **The invariant is observational, not actionable.** "The renderer should look fast" is doctrinal; no test reproduces a human's perception of frame pacing.
- **The shape is in active design pressure.** Testing an unstable invariant ossifies the unstable shape — the §3 *unstable experimentation* class warns against this.
- **The cost-walk closes one of §6.1–§6.7.** A candidate that triggers any §6 cost is not improved by being tested.

The §11 probes name several invariants in this category. They are not coverage gaps; they are correctly-classified non-tests.

## 9. Doctrine that should remain prose / non-tested

Six categories where the workspace's architectural truth is best expressed as doctrine and worst expressed as a test. Each is a worked example of §8 in action.

- **The architecture freeze policy (`PLAN.md` §0.6).** The freeze is a process invariant — "PLAN.md does not change without ADR amendment." Testing the absence of changes is meaningless; the discipline is social-governance, and the ADR amendment record is the verification surface. A test asserting "PLAN.md hash matches" pins an incidental cryptographic fact and breaks on any whitespace edit.

- **The dogfood rule's full content (`PLAN.md` §10.4).** "Tier-2 plugins use the same `Plugin` trait as Tier-3." The five canary tests verify the trait holds for representative resource families; the dogfood rule itself — that this equivalence is *required* for any future Tier-2 plugin — is doctrine, not test. A meta-test asserting "every Tier-2 crate has a canary" is the §10 anti-pattern manifesting; the canary suite is bounded and intentional.

- **Subsystem maturity classifications.** "GPU abstraction is Early; ECS / runtime core is Strong experimental" (`docs/architecture/README.md` Subsystem maturity table). These are governance assessments; testing them via metric thresholds confuses §3 *advisory semantics* with structural authority and creates the §7.5 test-as-spec failure mode.

- **The `importance ≠ executability` and `importance ≠ test coverage` distinctions.** Doctrine-tier; meta-claims about other claims. A test asserting "lint count exceeds doctrine count" or "test count exceeds doctrine count" reifies an incidental ratio.

- **Anti-patterns enumerated in §10 below.** The anti-patterns are negative — "do not do X." Testing the absence of an anti-pattern requires walking every contributor's commits for the pattern; the cost-benefit collapses immediately.

- **Naming conventions.** `kernel/<crate>` / `crates/<tier-2>` / `Cargo.toml` `package.metadata.rge.formats` keys / `ParticipantId(String)` conventions. The conventions are inherited by example through dispatch-pattern memory; the architecture-lint gate covers structural rules, not naming. A test asserting naming would pin an incidental shape and ossify the convention against legitimate evolution.

For each category, the non-test status is **deliberate**, not pending. Promoting any of them to a dedicated test would make the workspace worse. The deliberate-vs-pending distinction is what §8's `importance ≠ test coverage` opening pre-empts.

## 10. Anti-patterns

Seven test-substrate anti-patterns. Each is a specialization of sub-1's broader anti-pattern set, applied to test shape.

### 10.1 Universal test framework / test-everything zealotry

A generic harness that all tests run through. Promotes the §7.1 substrate-becomes-architecture mode — every new test pulls the harness's assumptions into the new domain. The workspace's deliberate choice across `tools/architecture-lints/tests/`, `tests/cross_substrate_determinism.rs`, the canary suites, and the per-crate `host_tests/` sub-modules is **multiple bounded harnesses**, each shaped to its test class.

### 10.2 Meta-test abstractions

Tests testing tests. A `tests/test_harness_test.rs` that asserts the canary harness handles ContractViolation correctly is a recursive substrate that has no terminal — testing the harness's harness becomes the next dispatch's coverage gap. Sub-1's anti-pattern set names this directly; the test-shape specialization is "stop at one level of abstraction."

### 10.3 Test registries / discovery frameworks

A central registry that lists all architectural tests, with discovery harness, with metadata. Reflective registry pattern; sub-1 §10's reflection-registry anti-pattern in test form. The workspace's test discovery is `cargo test`; that is sufficient and intentional.

### 10.4 Test-driven design

Writing tests to drive what code is added. The pattern is widely advocated outside this workspace; in this workspace it is the §7.4 gravity failure mode operationalized. Tests should follow architectural commitments, not generate them; doctrine is generated by reasoning, ADRs by decisions, tests by graduation.

### 10.5 Architecture DSLs / test description languages

A meta-language for expressing architecture lints / tests: "describe the test in YAML, the harness runs it." The substrate becomes the architecture (§7.1) at compile-time-of-the-DSL; updating the DSL is more expensive than the tests it generates.

### 10.6 Coverage thresholds as quality gates

"PRs must maintain 80% line coverage." The threshold is incidental; the §7.2 false-confidence and §7.5 test-as-spec failure modes follow immediately. Coverage is a per-test-class diagnostic, not a global quality metric.

### 10.7 "Architectural test" as separate test directory

A `tests/architectural/` separate from `tests/integration/` or `tests/<area>/`. The directory split implies a substrate decision (architectural tests are *different*) and pulls the §3 vocabulary into a structural form. The workspace's choice is to keep architectural tests in the location their class dictates — fixtures in `tools/architecture-lints/tests/`, integrations in `tests/<area>.rs`, canaries in `crates/<plugin>/tests/plugin_adapter_smoke.rs`, sub-module units in `crate/src/<area>_tests/`. Per-class location is a coordination cost paid once; the directory split is a coordination cost paid forever.

## 11. Workspace probes — canonical exemplars

Eight existing tests that exemplify their §3 class and §4 shape. **These are classification probes, not implementation tasks.** Each probe surfaces a non-trivial classification choice the workspace has already made.

### Probe 1 — Architecture-lint fixture binaries (Class: Structural-authority assertion; Shape: Fixture binary)

`tools/architecture-lints/tests/fixtures/forbidden_dep/{violation,allowed,exempted}/` plus 8 sibling lint directories. **69 fixture-test binaries** carry the architectural-test substrate; each fixture is a real Cargo.toml shape that the lint algorithm walks. The classification choice: the fixture *must be a real shape*, not a mocked input — the lint's algorithm can only be validated against the same kind of substrate it operates on in production. Probe value: this is the upper bound on architectural-test investment in the workspace; further fixture proliferation triggers §7.7 evolution failure.

### Probe 2 — PIE 50-iter cross-substrate determinism soak (Class: Determinism; Shape: Integration soak)

`tests/cross_substrate_determinism.rs::pie_three_participant_round_trip_50_iter`. Three participants (`World`, `CadGraph`, `CadProjection`) compose into a `PieSnapshot`; 50 round-trips assert byte-identical output. The classification choice: the iteration count is incidental (50 ≅ 51); the structural property is "byte-identical regardless of iteration count." The test pins the shape; doctrine carries the absolute claim. Probe value: the test could have been written as 1-iter and the determinism property would still be asserted; the soak guards against introduction of nondeterminism that surfaces only at scale.

### Probe 3 — Five plugin canaries under ADR-116 (Class: Substrate contract assertion; Shape: Per-trait canary suite)

`crates/cad-projection/src/plugin_adapter.rs` + `crates/gfx/src/plugin_adapter.rs` + `crates/physics/src/plugin_adapter.rs` + `crates/audio/src/plugin_adapter.rs` + `crates/editor-ui/src/plugin_adapter.rs`, each with a sibling `tests/plugin_adapter_smoke.rs`. Five canaries cover four resource families (CAD-graph / GPU / physics-world / audio + editor-ui-as-fifth-substrate). The classification choice: each canary is bounded — there is no canary for crates that do not host a plugin. Probe value: the bounded canary set is the workspace's most successful application of §5 graduation discipline; expanding the canary set requires explicit substrate justification, not coverage instinct.

### Probe 4 — PluginError × PluginPhase 4-cell auto-emit (Class: Coverage-gap closure; Shape: Per-crate unit-test sub-module)

`kernel/plugin-host/src/host/host_tests/diagnostics.rs::{init_all_emits_warning_for_contract_violation, shutdown_all_emits_warning_for_contract_violation, init_all_emits_error_for_runtime_fault, shutdown_all_emits_error_for_runtime_fault}` — added 2026-05-08. The four cells pin the host's by-variant severity dispatch (`emit_plugin_err_diagnostic` at host.rs:659) for Init and Shutdown phases (Tick was already covered). The classification choice: regression cells live with the consumer code, not in a separate `tests/regression_<n>.rs` file. Probe value: the gap was visible in HANDOFF.md backlog for months without urgency — graduation criteria 1 ("violated more than once") was satisfied by *4 untested cells*, not 4 violations.

### Probe 5 — ADR-116 retroactive `*_plugin_impls_canary_protocol` tests (Class: Substrate contract assertion; Shape: Per-canary inline)

Each canary crate carries one `<canary>_plugin_impls_canary_protocol` test asserting the `CanaryPlugin` trait is implemented; the harness is identical across all five canaries. The classification choice: the proof is per-canary, not centralized; centralization would have produced the §10.3 registry anti-pattern. Probe value: the retroactive harness extension was bounded and lazy — the trait was added to existing canaries one PR at a time, never as a meta-dispatch.

### Probe 6 — host_tests/ sub-module split (Class: Implementation-coupling avoidance; Shape: Per-crate unit-test sub-module)

`kernel/plugin-host/src/host/host_tests/{fixtures,registration,lifecycle,diagnostics,panic_recovery,resource_leak}.rs`. Pre-emptive Phase-5 split (audit-3 carryover) when host.rs hit 1766L. The classification choice: split by *orchestrator-side concern*, not by Plugin lifecycle method (init / tick / shutdown). Probe value: the concern-axis split survived the addition of 4 new tests for the 4-cell coverage gap (probe 4) without restructure — the split's axis is load-bearing.

### Probe 7 — Failure-class declaration enforcement (Class: Substrate contract assertion; Shape: Architecture-lint walker)

`tools/architecture-lints/src/failure_class.rs`. The lint walks every Tier-1 + Tier-2 crate's `lib.rs` and asserts the presence of `//! Failure class: <kind>`. **Not** a test of any specific failure class; a test that the *declaration* is present. The classification choice: the lint enforces presence; the doctrine holds the meaning of each class (`recoverable`, `snapshot-recoverable`, `kernel-fatal`, `plugin-fatal`). Probe value: the lint's value is *zero* without the doctrine; the doctrine's value is *partial* without the lint. They cooperate; neither is sufficient alone.

### Probe 8 — `non_exhaustive_pattern_match_compiles` SemVer hardening (Class: API-shape assertion; Shape: Inline file-foot unit test)

`kernel/plugin-host/src/plugin.rs::tests::plugin_error_non_exhaustive_pattern_match_compiles` plus siblings in `kernel/io-scheduler/src/{priority, request}.rs` and `crates/cad-core/src/operators/mod.rs`. Each is a single match expression with a wildcard arm marked `#[allow(unreachable_patterns)]`. The classification choice: the test is *inline* — it lives at the type's foot, not in a workspace-level SemVer harness. Probe value: the wildcard arm becomes a structural fact about cross-crate consumer expectations; deleting the test is a non-event for the type; deleting the type is a non-event for the test. Each pair is independent.

The eight probes do not exhaust the workspace's architectural-test substrate. They are diagnostic — each represents a class × shape choice already made and validated by surviving subsequent dispatches without restructure.

## 12. Out of scope

This doctrine **does not**:

- Introduce a new test harness, a new test framework, or a new test-discovery substrate.
- Add new tests to close any coverage gap. Coverage-gap closure is a separate dispatch (the 4-cell PluginError × PluginPhase work was such a dispatch).
- Propose graduation triggers for any specific invariant — that is a per-invariant decision applying §5 to a candidate.
- Mandate per-crate test-count thresholds or coverage thresholds. Coverage-as-quality-metric is anti-pattern §10.6.
- Reclassify existing tests into the §3 / §4 taxonomy. The taxonomy is diagnostic; existing tests retain their location and shape.
- Open Phase 1 EG sub-3 (governance-test substrate) or sub-4 (enforcement-boundary). Those land as separate doctrine docs after this one is durable.

The deliverable is reasoning about test-shape choice. It is binding for the *next* test added to the workspace, not retroactive for tests already shipping.

## 13. References

- `INVARIANT_ENFORCEMENT_STRATEGY.md` — sibling doctrine; this doctrine subdivides its tests-tier.
- `docs/§18/ARCHITECTURE_LINTS.md` — lint substrate; canonical fixture-test pattern.
- `docs/architecture/REACTIVE_INVALIDATION.md` — the 4-layer invalidation hierarchy whose verification surface is the integration-test substrate.
- `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` — ownership rules that cite probe 8's selective non-coverage stance.
- `docs/architecture/NON_GOALS.md` — sibling-in-spirit; "we will NOT" framing applied to runtime scope rather than test scope.
- `tools/architecture-lints/tests/fixtures/` — probe 1.
- `tests/cross_substrate_determinism.rs` — probe 2.
- `crates/{cad-projection, gfx, physics, audio, editor-ui}/src/plugin_adapter.rs` + sibling `tests/plugin_adapter_smoke.rs` — probe 3.
- `kernel/plugin-host/src/host/host_tests/` — probes 4, 6.
- `tools/architecture-lints/src/failure_class.rs` — probe 7.
- `kernel/plugin-host/src/plugin.rs::tests` — probe 8.
- `change.md` 2026-05-10 09:50 cross-review #10 archive — the consolidation-phase framing this doctrine extends.
