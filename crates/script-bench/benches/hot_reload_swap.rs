//! Bench: W4 - `hot_reload_swap`.
//!
//! Measures the latency of swapping one loaded module for another. The
//! native-Rust baseline replaces a `Box<dyn Fn>` and drops the old one: the
//! lower bound on what an engine swap must do at minimum. The
//! `script_host_counter_1000x100` row runs the formal Phase 3 gate: 1000
//! Counter-bearing entities preserved across 100 consecutive hot-reload cycles.

#![allow(missing_docs)] // criterion_group! generates an undocumented fn.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rge_script_bench::native_baseline::hot_reload_swap;
use rge_script_bench::script_host::{HotReloadConfig, ScriptHostBench};
use rge_script_bench::workloads::HOT_RELOAD_CYCLES;

fn bench_hot_reload_native(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_reload_swap");
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(50);

    group.bench_function("native_rust", |b| {
        b.iter(|| {
            black_box(hot_reload_swap(black_box(HOT_RELOAD_CYCLES)));
        });
    });

    let script_host = ScriptHostBench::new().expect("script-host fixtures compile");
    let config = HotReloadConfig::formal();
    group.bench_function("script_host_counter_1000x100", |b| {
        b.iter(|| {
            black_box(
                script_host
                    .hot_reload_preservation(black_box(config))
                    .expect("formal hot-reload preservation"),
            );
        });
    });

    group.finish();
}

criterion_group!(benches, bench_hot_reload_native);
criterion_main!(benches);
