# KERNEL_DIAGNOSTICS

| Companion to | PLAN.md §1.7 (unified diagnostic substrate) + PLAN.md §1.13 (failure-class taxonomy) |
|---|---|
| Status | Stable v0; consumed across kernel + Tier-2; auto-emit policy hardened post-2026-05-08 (audit-2 Phase 0 + LOW #5 closure) |
| Audience | Every subsystem author — diagnostics is the workspace's single typed error stream; every Tier-1 + Tier-2 crate routes through it |
| Sibling doc | `PLUGIN_API.md` — primary auto-emit consumer (every plugin lifecycle Err / Panic / leak path emits a structured `Diagnostic`) |
| Reference impls | `kernel/diagnostics/src/{lib,diagnostic,severity,failure_class,sink,aggregator,span}.rs` (substrate) · `kernel/plugin-host/src/host.rs` (auto-emit policy) · `crates/cad-projection/src/plugin_adapter.rs` + `crates/gfx/src/plugin_adapter.rs` + `crates/physics/src/plugin_adapter.rs` (Tier-2 consumers via `PluginContext::emit_diagnostic`) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the unified diagnostic substrate; subsystem-specific diagnostic conventions belong in their sibling §18 docs.

## 1. Why a substrate

Without a substrate, N subsystems would each invent their own logging — `eprintln!` in some, `tracing` in others, an ad-hoc `log_error` helper in a third. Editor UI, CI, replay logs would each have to consume N different streams. PLAN §1.7 commits to one typed `Diagnostic` enum + one `DiagnosticSink` trait + one aggregator pattern; this substrate is that commitment.

Per the lib-level module-doc, the design goals are:

- **Lightweight** — zero heavy deps (no `miette`, no `ariadne`). Adoptable by all ~80 downstream crates without compile-time penalty.
- **Object-safe** — `DiagnosticSink` is a plain `dyn`-safe trait so subsystems accept `&mut dyn DiagnosticSink` without generics explosion.
- **Stable surface** — every `pub` item is load-bearing for later kernel crates; additions require deliberate review.

Every Tier-1 + Tier-2 system routes failures through this single typed stream. Editor UI / CI / replay logs all consume the same `Diagnostic` shape.

## 2. `Diagnostic` — the primary type

Lives at `kernel/diagnostics/src/diagnostic.rs`. The structured diagnostic record:

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub failure_class: Option<FailureClass>,
    pub span: Span,
    pub message: String,
    pub suggestion: Option<Suggestion>,
}
```

Note the source-truth shape: there is NO `id: DiagnosticId` field and NO standalone `source_location: Option<SourceLoc>` — `SourceLoc` lives inside `Span` as `span.source: Option<SourceLoc>`. The dispatch spec speculatively listed `id` / `source_location`; the actual surface is the simpler 5-field shape above. (See report §3 for this inconsistency flag.)

### Constructor helpers

```rust
impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self;
    pub fn warning(message: impl Into<String>) -> Self;
    pub fn info(message: impl Into<String>) -> Self;
    pub fn suggestion(message: impl Into<String>) -> Self;
}
```

Each constructor sets `severity` and starts with `failure_class: None`, `span: Span::new()`, `suggestion: None`. Refine via builder methods:

```rust
let d = Diagnostic::error("shader compile failed")
    .with_span(Span::at_file("pbr.wgsl", 42, 1))
    .with_failure_class(FailureClass::Recoverable)
    .with_suggestion(Suggestion::new("check the syntax around line 42"));
```

`Display` renders as `[<severity>] <location>: <message>` with secondary `Span` fields appended as `(key=value)` pairs when present (e.g. `(node=mat::albedo)`, `(asset=textures/rock.png)`).

## 3. `Severity`

Lives at `kernel/diagnostics/src/severity.rs`. Four variants ordered by escalation:

```rust
pub enum Severity {
    Suggestion = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
}
```

Note the source-truth shape: there are **four** severity levels, NOT the six-level `Trace / Debug / Info / Warning / Error / Fatal` the dispatch spec speculatively listed. The actual surface is `Suggestion / Info / Warning / Error`. (See report §3 for this inconsistency flag.) `PartialOrd` / `Ord` reflect the escalation order so `Severity::Suggestion < Severity::Error`.

### When to emit each

- **`Suggestion`** — non-actionable hint that a better approach exists. Auto-fix candidate for the editor's quick-fix surface.
- **`Info`** — informational message; no action required. Lifecycle milestones, stat updates.
- **`Warning`** — likely-wrong but the operation can continue. Caller misconfiguration, non-fatal events.
- **`Error`** — hard error; the operation cannot produce valid output. Plugin / runtime failures.

`label()` returns the lower-case label string (`"suggestion"`, `"info"`, `"warning"`, `"error"`) for terminal output and log lines.

## 4. `FailureClass`

Lives at `kernel/diagnostics/src/failure_class.rs`. Five variants per PLAN §1.13:

```rust
pub enum FailureClass {
    Recoverable,
    SnapshotRecoverable,
    PluginFatal,
    SessionFatal,
    KernelFatal,
}
```

A failure class is a *tag* carried by a `Diagnostic` (via `failure_class: Option<FailureClass>`) that tells consumers how to respond. The substrate itself does NOT enforce recovery semantics — callers decide what to do with `SessionFatal` vs `Recoverable`.

Not every diagnostic carries a class; informational and suggestion diagnostics typically leave it `None`.

### Subsystem declaration via doc-comment

Subsystems declare their **crate-level** failure class via a `//! Failure class: <kind>` lib.rs doc-comment, where `<kind>` matches `FailureClass::label()` (e.g. `recoverable` / `snapshot-recoverable` / `plugin-fatal` / `session-fatal` / `kernel-fatal`). The `architecture-lints` `failure-class` lint enforces the declaration on every Tier-1 + Tier-2 crate; crates without a declaration must appear in the `failure-class` exemptions table at `tools/architecture-lints/exemptions.toml`.

The doc-comment declaration documents the class for the *crate as a whole* — individual `Diagnostic`s can carry any class via `with_failure_class(...)`. For example, `kernel/diagnostics` itself declares `recoverable` (it's a routing layer; a sink emit can never escalate the host into a fatal state) but its callers can construct `Diagnostic::error("...").with_failure_class(FailureClass::PluginFatal)` to mark a routed event as plugin-fatal.

## 5. `Span` + `SourceLoc`

Lives at `kernel/diagnostics/src/span.rs`. Where in the input space a diagnostic originates:

```rust
pub struct Span {
    pub source: Option<SourceLoc>,
    pub graph_node: Option<String>,
    pub script_line: Option<u32>,
    pub asset_path: Option<String>,
}

pub struct SourceLoc {
    pub file: String,
    pub line: u32,
    pub column: u32,
}
```

All four `Span` fields are optional; at least one should be populated in any concrete `Span` worth emitting. The builder methods make populating only the meaningful fields easy:

```rust
Span::at_file("foo.rs", 12, 5);
Span::at_graph_node("mat::albedo");
Span::at_script_line(42);
Span::at_asset("assets/textures/rock.png");
```

Each constructor returns a `Span` with one field populated; `with_*` chains layer additional fields. `is_empty()` reports whether all four are `None`. Optional for substrate-side diagnostics; populated for source-mapped diagnostics from compiled scripts / WASM hosts / asset loaders.

## 6. `Suggestion`

```rust
pub struct Suggestion {
    pub message: String,
    pub replacement: Option<String>,
}
```

Actionable fix proposals attached to a `Diagnostic` via `with_suggestion(...)`. The `replacement` field is for editor-applied automatic fixes; callers without an automatic patch use `Suggestion::new(message)` (replacement = `None`). Consumed by editor UI quick-fix surfaces; surfaced inline in CI logs.

## 7. `DiagnosticSink` trait

Lives at `kernel/diagnostics/src/sink.rs`. Object-safe emission target:

```rust
pub trait DiagnosticSink {
    fn emit(&mut self, diagnostic: Diagnostic);
}
```

Subsystems accept `&mut dyn DiagnosticSink` so the caller decides what happens — buffer, stream to `tracing`, discard — without forcing generics on every API. The blanket `impl DiagnosticSink for ()` makes `()` a no-op sink, useful for tests and stubs that don't care about output.

Implementors:

- `DiagnosticAggregator` — default; collects to `Vec<Diagnostic>` (§8).
- `()` — no-op (test fixtures).
- Future: streaming sinks for CI / replay / structured-log adapters.

## 8. `DiagnosticAggregator`

Lives at `kernel/diagnostics/src/aggregator.rs`. The default sink:

```rust
pub struct DiagnosticAggregator {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticAggregator {
    pub fn new() -> Self;
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic>;
    pub fn into_inner(self) -> Vec<Diagnostic>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn highest_severity(&self) -> Option<Severity>;
    pub fn has_errors(&self) -> bool;
    pub fn at_least(&self, min: Severity) -> impl Iterator<Item = &Diagnostic>;
}

impl DiagnosticSink for DiagnosticAggregator {
    fn emit(&mut self, diagnostic: Diagnostic);
}
```

Collects diagnostics in insertion order without interrupting the caller. Query methods support common patterns: `has_errors()` for "did anything fatal happen?", `at_least(Severity::Warning)` for filtering iteration, `highest_severity()` for status-bar display, `into_inner()` to drain.

The "never fail-fast" property is load-bearing — every batch operation accumulates diagnostics via the sink and only inspects them at completion, so callers can choose whether one error in a batch poisons the whole operation or just that one item.

## 9. Plugin-host auto-emit policy

Lives at `kernel/plugin-host/src/host.rs`. Per audit-2 Phase 0 + LOW #5 closure (HANDOFF.md, 2026-05-08), `PluginHost::init_all` / `tick_all` / `shutdown_all` auto-emit synthetic `Diagnostic`s on every plugin Err / Panic / leak path, with structured prefix `"plugin <id> {phase} failed: <err>"`.

The `emit_plugin_err_diagnostic` helper (lines 666-678 of host.rs) is the central dispatch:

```rust
fn emit_plugin_err_diagnostic(
    ctx: &mut PluginContext<'_>,
    id: &PluginId,
    phase: PluginPhase,
    plugin_err: &PluginError,
) {
    let msg = format!("plugin {id} {phase} failed: {plugin_err}");
    let diag = match plugin_err {
        PluginError::ContractViolation { .. } => Diagnostic::warning(msg),
        _ => Diagnostic::error(msg),
    };
    ctx.emit_diagnostic(diag);
}
```

### Severity policy

| `PluginError` variant | Auto-emit Severity | Rationale |
|---|---|---|
| `ContractViolation { resource_type }` | `Warning` | Caller misconfiguration; not a plugin bug. Avoids 60Hz error-spam from a misconfigured ctx. |
| `RuntimeFault { reason }` | `Error` | Genuine plugin-side failure. |
| `InitFailed { reason }` | `Error` | Genuine plugin-side init failure. |
| `Panic { phase, payload }` | `Error` | Host-classified; resources held by the panicking plugin are unrecoverable. |
| `ShutdownFailed { reason }` (lifecycle-driven) | `Error` | Plugin's own shutdown raised — real failure. |
| `ShutdownFailed { reason }` (host-initiated unregister) | `Warning` | Host explicitly invoked the unregister; teardown imperfection isn't an "engine is broken" signal. |
| Resource-leak on `Ok` return | `Error` | Disciplinary failure — plugin returned `Ok` but didn't put back what it took. Marks the plugin `Failed`. |
| Resource-leak on `Err` return | `Error` | Compounded failure on top of the underlying error. |
| Resource-leak on Panic | `Error` | Compounded — leak + panic. |
| Resource-leak on unregister-shutdown (any outcome) | `Warning` | Host-initiated; non-fatal by design (matches the unregister policy). |

The discrimination is enforced by the `tick_all_emits_warning_for_contract_violation` and `unregister_emits_warning_on_shutdown_failure` regression tests in `kernel/plugin-host/src/host.rs` test module (lines 1675 / 1726).

Cross-ref ADR-114 §"PluginError variant policy" for the design rationale; cross-ref `PLUGIN_API.md` §3 ("`PluginError` taxonomy") for the full per-variant constructor surface and additional context on the host-classified `Panic` variant.

## 10. Consumers across the workspace

The 13 crates depending on `rge-kernel-diagnostics` (per Cargo.toml deps):

- **`kernel/diagnostics`** — defines the substrate.
- **`kernel/plugin-host`** — auto-emit policy (§9); plugins route via `PluginContext::emit_diagnostic`.
- **`kernel/events`** — channel-overflow warnings.
- **`kernel/app`** — frame-stat anomalies, lifecycle milestones.
- **`kernel/schedule`** — scheduler diagnostics (out-of-order tasks, missed deadlines).
- **`crates/cad-projection`** — `ProjectionError` surfaces via `PluginContext::emit_diagnostic` through the plugin canary.
- **`crates/gfx`** — render-tier issues (pipeline-build failures, headless-target validation).
- **`crates/physics`** — physics-tier issues (joint validity, step-result auditing).
- **`crates/audio`** — mixer-tier issues (clip-load failures, manager errors).
- **`crates/editor-actions`** — action-failure diagnostics.
- **`crates/script-host`** — script-source-mapped diagnostics (uses `Span::at_script_line`).
- **`crates/ui-theme`** — theme-load diagnostics.
- **(workspace root `Cargo.toml`)** — declares the dep for workspace-wide use.

The consumer pattern is uniform: subsystems accept `&mut dyn DiagnosticSink` (or borrow one from `PluginContext::diagnostics()`) and call `sink.emit(Diagnostic::...(...))`. The sink decides downstream behaviour.

## 11. Failure class — recoverable

Per PLAN §1.13 and the `//! Failure class: recoverable` declaration on `kernel/diagnostics/src/lib.rs`. Every sub-module inherits the class.

`kernel/diagnostics` is a routing layer — it doesn't fail catastrophically. Sink emit is infallible (`fn emit(&mut self, Diagnostic)` returns `()`); aggregator append is infallible; severity comparison and label rendering are infallible. The only operations that can fail are the upstream serde deserialization paths (used by callers that load saved diagnostics from disk for replay / CI), which surface as `serde_json` / `ron` errors at the call site, not as `kernel/diagnostics`-emitted errors.

The `architecture-lints` `failure-class` lint enforces the declaration; `kernel/diagnostics` does not appear in the failure-class exemptions table.

## 12. References

- **PLAN.md §1.7** — unified diagnostic substrate.
- **PLAN.md §1.13** — failure-class taxonomy.
- **ADR-114** — auto-emit policy origin (§"Decision" + §"PluginError variant policy" + §"Implementation guidance").
- **`PLUGIN_API.md`** — sibling §18 doc; auto-emit consumer surface; `PluginError` taxonomy reference.
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; how plugin authors emit diagnostics through `PluginContext`.
- **`kernel/diagnostics/src/lib.rs`** — module roots + failure-class declaration + design goals.
- **`kernel/diagnostics/src/diagnostic.rs`** — `Diagnostic` + `Suggestion` + builder methods + `Display`.
- **`kernel/diagnostics/src/severity.rs`** — `Severity` + `label()` + `Ord` discipline.
- **`kernel/diagnostics/src/failure_class.rs`** — `FailureClass` + `label()` (matches doc-comment lint format).
- **`kernel/diagnostics/src/sink.rs`** — `DiagnosticSink` trait + no-op `()` impl.
- **`kernel/diagnostics/src/aggregator.rs`** — `DiagnosticAggregator` (default sink).
- **`kernel/diagnostics/src/span.rs`** — `Span` + `SourceLoc` + builder constructors.
- **`kernel/plugin-host/src/host.rs`** — auto-emit policy implementation (`emit_plugin_err_diagnostic`); per-phase severity discrimination; resource-leak classification.
- **`tools/architecture-lints/`** — `failure-class` lint enforcement on subsystem doc-comment declarations.
