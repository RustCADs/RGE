//! Whitelist enforcement (W19 spec exit criterion: `compile("system_call()")`
//! fails with diagnostic).
//!
//! The closed stdlib is defined in `src/stdlib.rs`. Adding entries here
//! also requires editing this test — pairing the change with a visible
//! audit point.

use rge_expr_wasm::{Compiler, ExprError};

#[test]
fn unknown_function_rejected() {
    let compiler = Compiler::new();
    let err = compiler
        .compile("system_call()")
        .expect_err("must reject non-whitelisted function");
    match err {
        ExprError::UnknownFunction { name } => {
            assert_eq!(name, "system_call");
        }
        other => panic!("expected UnknownFunction, got {other:?}"),
    }
}

#[test]
fn arity_mismatch_rejected() {
    let compiler = Compiler::new();
    let err = compiler
        .compile("sin(1, 2)")
        .expect_err("must reject wrong arity");
    match err {
        ExprError::Arity {
            name,
            expected,
            got,
        } => {
            assert_eq!(name, "sin");
            assert_eq!(expected, 1);
            assert_eq!(got, 2);
        }
        other => panic!("expected Arity, got {other:?}"),
    }
}

#[test]
fn diagnostic_message_contains_function_name() {
    let compiler = Compiler::new();
    let err = compiler.compile("malicious_call(x)").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("malicious_call"),
        "diagnostic must name offending function, got: {msg}"
    );
    assert!(
        msg.contains("whitelist"),
        "diagnostic must mention whitelist, got: {msg}"
    );
}

#[test]
fn no_statements_no_let_no_loops() {
    let compiler = Compiler::new();
    // `let` is treated as an identifier; `let x = 1` parses to `let` then `x`
    // which is a syntax error (no infix op between two idents).
    assert!(compiler.compile("let x = 1").is_err());
    assert!(compiler.compile("for i in 0..10").is_err());
    // Statements are syntax errors — semicolon is unknown token.
    assert!(compiler.compile("1; 2").is_err());
}

/// Snapshot the canonical whitelist size + names. If this test fails,
/// either an entry was added (then update this list AND add an ADR),
/// or an entry was removed (then update this list AND audit downstream
/// consumers).
#[test]
fn whitelist_canonical_set() {
    use rge_expr_wasm::stdlib::STDLIB;
    let names: Vec<&'static str> = STDLIB.iter().map(|e| e.name).collect();
    let expected: &[&str] = &[
        // imports
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "atan2",
        "pow",
        "exp",
        "log",
        "log2",
        // native
        "sqrt",
        "abs",
        "floor",
        "ceil",
        "round",
        "min",
        "max",
        "mod",
        // inline
        "clamp",
        "lerp",
        "smoothstep",
        "step",
    ];
    assert_eq!(
        names, expected,
        "whitelist drift detected — see ADR-076 / PLAN.md §1.4 (Rhai-test audit)"
    );
}
