//! Steady-state eval throughput. Spec target: ≤ 50 ns / call (cached).

#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rge_expr_wasm::{Compiler, Evaluator};

fn bench_eval_simple(c: &mut Criterion) {
    let compiler = Compiler::new();
    let handle = compiler.compile("time * 0.5 + 0.7").unwrap();
    let mut eval = Evaluator::new(&compiler, handle).unwrap();
    c.bench_function("eval_simple", |b| {
        let mut t = 0.0_f32;
        b.iter(|| {
            t += 0.001;
            black_box(eval.eval(&[black_box(t)]).unwrap())
        });
    });
}

fn bench_eval_sin(c: &mut Criterion) {
    let compiler = Compiler::new();
    let handle = compiler.compile("sin(time * 0.5) * 0.3 + 0.7").unwrap();
    let mut eval = Evaluator::new(&compiler, handle).unwrap();
    c.bench_function("eval_sin_signature_expr", |b| {
        let mut t = 0.0_f32;
        b.iter(|| {
            t += 0.001;
            black_box(eval.eval(&[black_box(t)]).unwrap())
        });
    });
}

fn bench_eval_polynomial(c: &mut Criterion) {
    // No host calls — pure native ops. Closer to the 5 ns/call envelope.
    let compiler = Compiler::new();
    let handle = compiler
        .compile("a*a*a + 3*a*a*b + 3*a*b*b + b*b*b")
        .unwrap();
    let mut eval = Evaluator::new(&compiler, handle).unwrap();
    c.bench_function("eval_polynomial", |b| {
        let mut t = 0.0_f32;
        b.iter(|| {
            t += 0.001;
            black_box(eval.eval(&[black_box(t), black_box(0.5)]).unwrap())
        });
    });
}

criterion_group!(
    benches,
    bench_eval_simple,
    bench_eval_sin,
    bench_eval_polynomial
);
criterion_main!(benches);
