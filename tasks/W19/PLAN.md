# Wave W19 — expr-wasm

> Self-contained agent dispatch. Phase 3+ deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §5.3 (expr-wasm — inline expression compiler); ADR-076 (Rhai-test rejection of bytecode VM).

## Goal

String → AST → WASM bytes compiler for inline expressions. Whitelist stdlib. Cached compilation. Native-speed eval after first compile (~5ns/call).

## Crate owned

`crates/expr-wasm`.

## Files this wave touches

```
crates/expr-wasm/src/{lib.rs, parser.rs, ast.rs, codegen.rs, stdlib.rs, cache.rs, expr_handle.rs}
crates/expr-wasm/tests/{correctness_test.rs, whitelist_test.rs, cache_test.rs}
crates/expr-wasm/benches/eval_speed.rs           # criterion bench
crates/expr-wasm/benches/compile_speed.rs        # criterion bench
```

## Stubs needed

- `wasmtime` workspace dep — direct usage; this crate is the simplest wasmtime consumer in the workspace.
- No reflection / no ECS — expr-wasm is intentionally narrow.

## Implementation order

1. `parser.rs` — Pratt parser, ~50 LOC. Operators: + - * / % and unary -. Function calls: `name(arg1, arg2, ...)`. Variable lookup: `name`. Ternary: `cond ? then : else`. No statements, no `let`, no loops.
2. `ast.rs` — `Expr` enum: `Number(f32) | Var(String) | BinOp(Op, Box<Expr>, Box<Expr>) | UnaryOp(Op, Box<Expr>) | Call(String, Vec<Expr>) | Ternary(Box<Expr>, Box<Expr>, Box<Expr>)`.
3. `stdlib.rs` — whitelisted functions: `sin cos tan asin acos atan atan2 sqrt pow exp log log2 abs floor ceil round mod min max clamp lerp smoothstep step`. Plus comparison ops (`<`, `>`, `<=`, `>=`, `==`, `!=`) and logical (`&&`, `||`, `!`). Closed set; CI lint flags additions.
4. `codegen.rs` — AST → WASM bytes via `wasm-encoder` crate (or hand-rolled). Single function exporting `eval(env: *const f32) -> f32` (or `bool` / `vec3` for some uses).
5. `cache.rs` — `compile(&str) -> wasmtime::Module` cached by source hash; first-compile ~1ms; cached eval ~5ns.
6. `expr_handle.rs` — `ExprHandle` (cache key); `Compiler::compile(&str) -> ExprHandle`; `Evaluator::eval(handle, env) -> f32`.
7. Test: `compile("sin(time * 0.5) * 0.3 + 0.7")` evaluates correctly within 1e-6 of f64 reference.
8. Test: cached eval ≤ 50ns per call (criterion).
9. Test: first-compile ≤ 2ms (criterion).
10. Test: whitelist enforcement — `compile("system_call()")` fails with diagnostic.

## Rustforge prior art (steal-and-adapt)

(none specific — rustforge has no inline expression compiler). Greenfield.

## Exit criteria

- `compile("sin(time * 0.5) * 0.3 + 0.7").eval(env)` matches f64 reference within 1e-6.
- Cached eval ≤ 50ns/call.
- First-compile ≤ 2ms.
- Whitelist additions blocked at compile time without explicit code review.
- `cargo test -p rge-expr-wasm` passes; `cargo bench -p rge-expr-wasm` produces baseline numbers.

## Duration estimate

3 days. **Highest-novelty wave** — the constitutional commitment that expression evaluation runs through the same wasmtime engine, no sibling interpreter (per ADR-076).

## Anti-pattern check

PASS — uses wasmtime; no separate interpreter. Closed-set stdlib (CI lint). API surface: `compile(&str) -> ExprHandle`, nothing else.

## Handoff

After merge: material-graph (post-W19) uses for parameter formulas; anim-graph for transition conditions; editor-ui/menus + layout for hot-reloadable predicates.
