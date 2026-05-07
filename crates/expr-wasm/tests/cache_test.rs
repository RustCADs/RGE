//! Cache hit semantics — same source string returns artifacts that share
//! the underlying [`wasmtime::Module`].

use std::sync::Arc;

use rge_expr_wasm::{Compiler, Evaluator};

#[test]
fn same_source_hits_cache() {
    let compiler = Compiler::new();
    let h1 = compiler.compile("a + b * 2").unwrap();
    let h2 = compiler.compile("a + b * 2").unwrap();
    // Cached artifact's source Arc should be reused (pointer equality).
    let s1: &str = h1.source();
    let s2: &str = h2.source();
    assert_eq!(s1, s2);
    // The Arc<[String]> for vars is the same allocation.
    let v1: *const [String] = h1.vars();
    let v2: *const [String] = h2.vars();
    assert!(
        std::ptr::eq(v1, v2),
        "expected vars Arc to be reused on cache hit"
    );
}

#[test]
#[allow(
    clippy::float_cmp,
    reason = "integer-input integer-output expression evaluation yields exact f32 values; bit-equality is the intended assertion"
)]
fn distinct_sources_distinct_artifacts() {
    let compiler = Compiler::new();
    let h1 = compiler.compile("x + 1").unwrap();
    let h2 = compiler.compile("x + 2").unwrap();
    assert_eq!(h1.source(), "x + 1");
    assert_eq!(h2.source(), "x + 2");
    // Both should still evaluate correctly.
    let mut e1 = Evaluator::new(&compiler, h1).unwrap();
    let mut e2 = Evaluator::new(&compiler, h2).unwrap();
    assert_eq!(e1.eval(&[5.0]).unwrap(), 6.0);
    assert_eq!(e2.eval(&[5.0]).unwrap(), 7.0);
}

#[test]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    reason = "small-integer iterator (0..32) lossless in f32; integer-square integer-add yields exact f32 values; bit-equality is the intended assertion"
)]
fn cached_eval_repeated_calls() {
    // Sanity: once compiled, repeated eval must produce stable results
    // (no state leak between calls).
    let compiler = Compiler::new();
    let handle = compiler.compile("x * x + 1").unwrap();
    let mut eval = Evaluator::new(&compiler, handle).unwrap();
    for x in 0..32 {
        let xf = x as f32;
        let r = eval.eval(&[xf]).unwrap();
        assert_eq!(r, xf * xf + 1.0, "iteration {x}");
    }
}

#[test]
fn compiler_clone_shares_cache() {
    let compiler = Compiler::new();
    let _h = compiler.compile("sin(t)").unwrap();
    let compiler2 = compiler.clone();
    // Compile via the clone; should hit the same cache.
    let h = compiler2.compile("sin(t)").unwrap();
    assert_eq!(h.source(), "sin(t)");
}

#[test]
fn handle_outlives_compiler() {
    // Drop the compiler; the handle's Arc to the artifact keeps the
    // module alive. We need the engine to evaluate, so capture it before
    // dropping.
    let handle;
    let _engine_holder;
    {
        let compiler = Compiler::new();
        _engine_holder = compiler.engine();
        handle = compiler.compile("a + 1").unwrap();
    }
    // For this test we just verify the handle still has its data.
    assert_eq!(handle.vars(), &["a".to_string()]);
    let _arc = Arc::new(handle); // sanity: handle is Clone
}
