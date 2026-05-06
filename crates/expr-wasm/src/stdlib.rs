//! Closed-set stdlib whitelist for expr-wasm.
//!
//! Per ADR-076 + PLAN.md §5.3, this list is fixed at the language level.
//! Adding a function requires (a) editing this file, (b) editing
//! `tests/whitelist_test.rs`, and (c) the CI architecture-lints crate
//! verifying [`STDLIB`] hasn't grown without an ADR. **Do not add functions
//! ad-hoc.**
//!
//! Functions split into three lowering classes:
//!
//! 1. **Native WASM op** (`Lower::Native`) — single instruction, e.g.
//!    `sqrt → F32Sqrt`, `min → F32Min`. Zero call overhead.
//! 2. **Host import** (`Lower::Import`) — transcendentals (`sin`, `cos`,
//!    `pow`, …) that aren't in the WASM 1.0 instruction set. Resolved
//!    against `f32::sin` etc. via the wasmtime `Linker`. ~10-20 ns
//!    overhead per call.
//! 3. **Inline expansion** (`Lower::Inline`) — `clamp`, `lerp`,
//!    `smoothstep`, `step`, `round` lower to a fixed sequence of native
//!    ops; same 5 ns/call envelope as user-written equivalents.

/// Lowering strategy for a stdlib entry.
#[derive(Debug, Clone, Copy)]
pub enum Lower {
    /// Single native WASM float op (or 2-3 native ops).
    /// The codegen module pattern-matches on the function name.
    Native,
    /// Imported host function — resolved via the wasmtime linker.
    Import,
    /// Macro-expanded inline at codegen time using [`Expr`] rewrite rules.
    /// (Currently we expand directly inside codegen; this tag is
    /// informational so audit tools can categorize entries.)
    Inline,
}

/// Closed-set entry: name, arity, lowering class.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// Function name as it appears in source. Match-key for `lookup`.
    pub name: &'static str,
    /// Number of arguments. Mismatch → [`crate::error::ExprError::Arity`].
    pub arity: usize,
    /// How the function lowers to WASM. See [`Lower`] variants.
    pub lower: Lower,
}

/// **The whitelist.** Modifications require ADR review per ADR-076.
///
/// Pending audit triggers (PLAN.md §1.4) flag any growth here.
pub const STDLIB: &[Entry] = &[
    // --- transcendentals (host imports) ---
    Entry {
        name: "sin",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "cos",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "tan",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "asin",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "acos",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "atan",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "atan2",
        arity: 2,
        lower: Lower::Import,
    },
    Entry {
        name: "pow",
        arity: 2,
        lower: Lower::Import,
    },
    Entry {
        name: "exp",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "log",
        arity: 1,
        lower: Lower::Import,
    },
    Entry {
        name: "log2",
        arity: 1,
        lower: Lower::Import,
    },
    // --- native WASM ops ---
    Entry {
        name: "sqrt",
        arity: 1,
        lower: Lower::Native,
    },
    Entry {
        name: "abs",
        arity: 1,
        lower: Lower::Native,
    },
    Entry {
        name: "floor",
        arity: 1,
        lower: Lower::Native,
    },
    Entry {
        name: "ceil",
        arity: 1,
        lower: Lower::Native,
    },
    Entry {
        name: "round",
        arity: 1,
        lower: Lower::Native,
    },
    Entry {
        name: "min",
        arity: 2,
        lower: Lower::Native,
    },
    Entry {
        name: "max",
        arity: 2,
        lower: Lower::Native,
    },
    Entry {
        name: "mod",
        arity: 2,
        lower: Lower::Native,
    },
    // --- inline expansions ---
    Entry {
        name: "clamp",
        arity: 3,
        lower: Lower::Inline,
    },
    Entry {
        name: "lerp",
        arity: 3,
        lower: Lower::Inline,
    },
    Entry {
        name: "smoothstep",
        arity: 3,
        lower: Lower::Inline,
    },
    Entry {
        name: "step",
        arity: 2,
        lower: Lower::Inline,
    },
];

/// Look up a stdlib entry by name. Returns `None` for unknown / non-whitelisted
/// functions; the caller must report this as [`crate::error::ExprError::UnknownFunction`].
pub fn lookup(name: &str) -> Option<&'static Entry> {
    STDLIB.iter().find(|e| e.name == name)
}

/// All host-imported function names — used by codegen to declare imports
/// and by [`crate::cache`] to wire the wasmtime [`wasmtime::Linker`].
pub fn imports() -> impl Iterator<Item = &'static Entry> {
    STDLIB.iter().filter(|e| matches!(e.lower, Lower::Import))
}
