//! First-compile latency. Spec target: ≤ 2 ms.

#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rge_expr_wasm::Compiler;

fn bench_first_compile_signature(c: &mut Criterion) {
    c.bench_function("compile_first_signature_expr", |b| {
        b.iter_with_setup(Compiler::new, |compiler| {
            let h = compiler
                .compile(black_box("sin(time * 0.5) * 0.3 + 0.7"))
                .unwrap();
            black_box(h);
        });
    });
}

fn bench_first_compile_polynomial(c: &mut Criterion) {
    c.bench_function("compile_first_polynomial", |b| {
        b.iter_with_setup(Compiler::new, |compiler| {
            let h = compiler
                .compile(black_box("a*a*a + 3*a*a*b + 3*a*b*b + b*b*b"))
                .unwrap();
            black_box(h);
        });
    });
}

fn bench_cached_compile(c: &mut Criterion) {
    // Cache hit path — should be sub-microsecond.
    let compiler = Compiler::new();
    let _warm = compiler.compile("sin(time * 0.5) * 0.3 + 0.7").unwrap();
    c.bench_function("compile_cached_hit", |b| {
        b.iter(|| {
            let h = compiler
                .compile(black_box("sin(time * 0.5) * 0.3 + 0.7"))
                .unwrap();
            black_box(h);
        });
    });
}

criterion_group!(
    benches,
    bench_first_compile_signature,
    bench_first_compile_polynomial,
    bench_cached_compile
);
criterion_main!(benches);
