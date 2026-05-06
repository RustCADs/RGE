//! Bench: W1 — `script_tick_1m_iters` and W2 — `per_frame_tick_10k_entities`.
//!
//! Both workloads exercise the same integration kernel:
//! `Transform.translation += dt * Transform.velocity`. W1 measures
//! single-entity throughput over 1M iterations; W2 measures one frame's
//! worth of work over 10k entities. Together they cover the two failure
//! modes for an engine claim of "1.5× of native": per-call overhead and
//! per-iteration overhead.
//!
//! v0.0.1 records only the native-Rust baseline. Engine columns are
//! reported as `pending` in `BASELINE.md`.

#![allow(missing_docs)] // criterion_group! generates an undocumented fn.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rge_script_bench::native_baseline::{per_frame_tick, script_tick};
use rge_script_bench::workloads::{
    generate_entities, Transform, Vec3, ENTITY_SEED, PER_FRAME_ENTITY_COUNT, SCRIPT_TICK_ITERATIONS,
};

fn bench_script_tick_1m(c: &mut Criterion) {
    let mut group = c.benchmark_group("script_tick_1m_iters");
    group.throughput(Throughput::Elements(u64::from(SCRIPT_TICK_ITERATIONS)));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function(
        BenchmarkId::new("native_rust", SCRIPT_TICK_ITERATIONS),
        |b| {
            b.iter(|| {
                let t = Transform {
                    translation: Vec3::new(0.0, 0.0, 0.0),
                    velocity: Vec3::new(1.0, 2.0, 3.0),
                };
                black_box(script_tick(black_box(t), black_box(SCRIPT_TICK_ITERATIONS)))
            });
        },
    );

    group.finish();
}

fn bench_per_frame_tick_10k(c: &mut Criterion) {
    let mut group = c.benchmark_group("per_frame_tick_10k_entities");
    group.throughput(Throughput::Elements(u64::from(PER_FRAME_ENTITY_COUNT)));
    group.measurement_time(Duration::from_secs(5));

    let buffer = generate_entities(PER_FRAME_ENTITY_COUNT, ENTITY_SEED);

    group.bench_function(
        BenchmarkId::new("native_rust", PER_FRAME_ENTITY_COUNT),
        |b| {
            b.iter_batched(
                || buffer.clone(),
                |mut frame| {
                    per_frame_tick(black_box(&mut frame));
                    black_box(frame);
                },
                criterion::BatchSize::SmallInput,
            );
        },
    );

    group.finish();
}

criterion_group!(benches, bench_script_tick_1m, bench_per_frame_tick_10k);
criterion_main!(benches);
