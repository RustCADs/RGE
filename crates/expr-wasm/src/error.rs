//! Diagnostic types. Closed set — every failure point in expr-wasm
//! produces one of these.

use thiserror::Error;

/// Failure modes for [`crate::compile`] and [`crate::eval`].
///
/// All variants are `Send + Sync` and carry a human-readable message;
/// the upstream caller (e.g. material-graph, anim-graph) is expected to
/// surface them through the editor diagnostic span infrastructure.
#[derive(Debug, Error)]
pub enum ExprError {
    /// Tokenization failure (stray character, malformed number).
    #[error("lex error at byte {offset}: {msg}")]
    Lex {
        /// Byte offset within the source string where the lexer choked.
        offset: usize,
        /// Human-readable reason.
        msg: String,
    },
    /// Parser failure (unexpected token, missing paren).
    #[error("parse error: {0}")]
    Parse(String),
    /// Function name not in the closed stdlib whitelist.
    /// See [`crate::stdlib::STDLIB`] for the full list.
    #[error("unknown function `{name}` (not in expr-wasm stdlib whitelist)")]
    UnknownFunction {
        /// Offending function name.
        name: String,
    },
    /// Wrong number of arguments to a stdlib call.
    #[error("function `{name}` expects {expected} args, got {got}")]
    Arity {
        /// Function name.
        name: String,
        /// Arity required by [`crate::stdlib::STDLIB`].
        expected: usize,
        /// Actual arg count parsed from the source.
        got: usize,
    },
    /// `wasm-encoder` rejected a generated module — should not happen,
    /// indicates a codegen bug.
    #[error("wasm encoding error: {0}")]
    Encode(String),
    /// `wasmtime` rejected the module or the linker — should not happen
    /// outside of host-machine misconfiguration.
    #[error("wasmtime error: {0}")]
    Wasmtime(String),
    /// The env slice passed to `eval` is shorter than the variable schema.
    #[error("env slice too short: expected {expected} entries, got {got}")]
    ShortEnv {
        /// Required env length per [`crate::ExprHandle::vars`].
        expected: usize,
        /// Actual env length the caller passed.
        got: usize,
    },
}
