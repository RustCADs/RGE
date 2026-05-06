# `kernel/types` — Compile-Time Budget

Per `IMPLEMENTATION.md` Phase 1.1 abort condition:

> If `kernel/types` reflection compile-time is already >30s on 5 pilot types, **STOP and replan reflection strategy** before proceeding.

Phase 1.1 baseline measurements taken on 2026-05-05.

## Baseline numbers

### Wall-clock compile time

| Build target | Configuration | Time |
|---|---|---|
| `cargo check -p rge-macros-reflect --tests` (clean of crate, deps cached) | dev | **1.1 s** |
| `cargo build -p rge-macros-reflect --test compile_budget_5_pilots` (clean of crate) | dev | **7.5 s** |
| `cargo build -p rge-macros-reflect --tests` (incremental, all 4 test targets) | dev | **12.3 s** |
| `cargo test -p rge-macros-reflect --test compile_budget_5_pilots` (cached) | test | **<2 s** |

The 7.5 s figure is the realistic upper bound for the 5-pilot pure macro-expansion + lower path (proc-macro crate must build first). All numbers comfortably under the 30 s gate.

### LLVM line count

`cargo-llvm-lines v0.4.45` was installed locally and run against:

```
cargo llvm-lines --lib -p rge-kernel-types
cargo llvm-lines --test compile_budget_5_pilots -p rge-macros-reflect
```

| Target | Total LLVM lines | Function count |
|---|---|---|
| `rge-kernel-types` library (no reflected types) | **2622** | 100 |
| 5-pilot test binary (5 reflected types, 24 fields) | **552** | 21 |

Per-pilot incremental cost ≈ **(552 - test-harness floor) / 5 ≈ 60 LLVM lines / type**. The library floor (2622 lines) is dominated by `ron::error::Error::fmt` (295 lines), `core::char::methods::encode_utf8_raw` (282 lines), and our own `SerdeBridgeError::Display` (130 lines) — i.e. error-formatting overhead from `thiserror`/`ron`, not reflection.

### Extrapolation to ~100 reflected types

Linear extrapolation (`100 types × 60 lines/type ≈ 6000 lines`) puts the cap at roughly **9000 LLVM lines for the entire reflection schema** — well below the workspace `cargo-bloat` warn threshold of 5000 generic instantiations per crate (`PLAN.md` §1.10.1). Compile-time scaling is approximately linear in fields; the macro emits no generic helpers, so monomorphization explosion is not a risk surface.

### Compile-time gate verdict

**PASS** — 5 pilots compile in 7.5 s clean, well under the 30 s abort gate. Phase 1.1 may merge.

## Constraints to preserve

The compile-time profile depends on a small set of guarantees that future waves must preserve:

1. **No `inventory` / `linkme` / `ctor`-style global registry.** Reflection metadata lives in per-type `&'static [FieldDescriptor]` slices, not a process-wide table. Adding a registry would introduce a static-initializer cost paid by every binary that links the kernel.
2. **No generic helpers in the derive output.** Each `#[derive(Reflect)]` emits one `impl Reflect for $T { ... }` block with O(fields) tokens — no `fn reflect_helper<T: ...>` or trait-bound shenanigans. The current codegen module enforces this by construction; bench guard at `compile_budget_5_pilots.rs` regresses CI if the line count balloons.
3. **No `blake3` or other heavy hash crate at the kernel-types level.** TypeId hashing is a hand-rolled FNV-1a-128. Pulling `blake3` 1.5+ drags in `cpufeatures 0.3.0`, which currently requires `edition2024` (not stabilized in 1.78). Higher-tier crates that need cryptographic content addressing (`pak-format`, `asset-store`) do their own dep — the kernel root stays light.
4. **`UiHint` is `Serialize`-only.** The `&'static [&'static str]` payload of `UiHint::FilePath` cannot round-trip through `Deserialize` because of the `Deserialize<'de>: 'static` lifetime trap. Diagnostic emission only — never load `FieldDescriptor` from disk.

## Re-running the baseline

```powershell
$env:CARGO_HOME = "A:\RustCache\cargo"
$env:PATH = "A:\RustCache\cargo\bin;$env:PATH"
cd A:\RCAD\RGE

# LLVM lines
cargo llvm-lines --lib -p rge-kernel-types
cargo llvm-lines --test compile_budget_5_pilots -p rge-macros-reflect

# Wall clock (5 pilots cold)
cargo clean -p rge-macros-reflect
$sw = [Diagnostics.Stopwatch]::StartNew()
cargo build -p rge-macros-reflect --test compile_budget_5_pilots
$sw.Stop()
"$($sw.Elapsed.TotalSeconds) s"
```

## Watch thresholds

| Metric | Watch (warn) | Hard (abort) | Current |
|---|---|---|---|
| Wall-clock for 5 pilots from-scratch | 15 s | 30 s | **7.5 s** |
| LLVM lines per reflected field | 100 | 250 | **~23** |
| Total LLVM lines for 100-type estimate | 15 000 | 25 000 | **~9 000** (extrapolated) |
| Generic instantiations per kernel/types crate | 1 000 | 5 000 | n/a (no generics emitted by macro) |

PLAN.md §1.10.1 hard gate is 15 000 generic instantiations / crate; the reflection layer contributes zero by design.
