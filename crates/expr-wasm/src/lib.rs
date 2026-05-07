//! `rge-expr-wasm` — inline expression compiler.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: expression-compile/evaluate failures (parse error,
//! unknown stdlib function, codegen failure, wasmtime instantiation error)
//! are transient and recoverable in-place — the caller surfaces the error
//! to the user (e.g. "expression `sin(time *` has unbalanced parens") and
//! falls back to a default value or rejects the binding. The Compiler cache
//! holds reproducible WASM modules keyed by source string; nothing PIE
//! stateful. Matches pak-format + audio + gfx (transient validation /
//! compilation failures).
//!
//! W19 deliverable per [`tasks/W19/PLAN.md`](../../tasks/W19/PLAN.md) and
//! PLAN.md §5.3. Constitutional commitment: expression evaluation runs
//! through the same wasmtime engine as full-fat scripts (ADR-076), no
//! sibling interpreter.
//!
//! ## Pipeline
//!
//! ```text
//! "sin(time * 0.5) + 0.7"
//!     │
//!     ├── parser ─→ Expr (≤50 LOC Pratt parser)
//!     │              │
//!     │              ├── ast (Number | Var | BinOp | Unary | Call | Ternary)
//!     │              │
//!     │              └── stdlib whitelist enforcement (closed set)
//!     │
//!     ├── codegen ─→ WASM bytes (wasm-encoder)
//!     │
//!     ├── cache    ─→ wasmtime::Module (Cranelift JIT)
//!     │
//!     └── eval     ─→ f32  (~5 ns/call cached, after first ~1 ms compile)
//! ```
//!
//! ## Public surface
//!
//! - [`Compiler`] — owns the cache, produces [`ExprHandle`]s.
//! - [`Evaluator`] — bound to one handle; hosts the wasmtime instance.
//! - [`ExprError`] — closed-set diagnostic enum.
//!
//! ```ignore
//! use rge_expr_wasm::{Compiler, Evaluator};
//!
//! let compiler = Compiler::new();
//! let handle = compiler.compile("sin(time * 0.5) * 0.3 + 0.7")?;
//! let mut eval = Evaluator::new(&compiler, handle)?;
//! let y = eval.eval(&[1.5])?;            // env: [time]
//! ```
//!
//! ## Whitelist (closed)
//!
//! Trigonometry: `sin cos tan asin acos atan atan2`. Algebraic:
//! `sqrt pow exp log log2 abs floor ceil round mod`. Range:
//! `min max clamp lerp smoothstep step`. Comparison + logical operators
//! (`<`, `>`, `<=`, `>=`, `==`, `!=`, `&&`, `||`, `!`) and ternary
//! (`?:`) are language built-ins.
//!
//! Adding entries requires editing [`stdlib::STDLIB`] **and** an ADR — the
//! `architecture-lints` tool watches this list per the Rhai-test
//! anti-pattern audit (PLAN.md §1.4).

pub mod ast;
mod cache;
pub mod codegen;
pub mod error;
pub mod expr_handle;
pub mod parser;
pub mod stdlib;

pub use error::ExprError;
pub use expr_handle::{Compiler, Evaluator, ExprHandle};
