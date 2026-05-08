//! Bench: W3 - `cold_start`.
//!
//! Module load + ready-to-tick latency. Native-Rust baseline measures the
//! "empty closure call" floor; this is the timer-overhead floor any engine
//! must out-perform on initialisation. The `script_host_counter` row exercises
//! the shipped `rge-script-host` Counter fixture.

#![allow(missing_docs)] // criterion_group! generates an undocumented fn.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rge_script_bench::native_baseline::cold_start;
use rge_script_bench::script_host::ScriptHostBench;

fn bench_cold_start_native(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start");
    group.measurement_time(Duration::from_secs(2));
    // Cold-start is single-shot by definition: keep sample count modest.
    group.sample_size(50);

    group.bench_function("native_rust", |b| {
        b.iter(|| {
            black_box(cold_start());
        });
    });

    group.bench_function("script_host_counter", |b| {
        b.iter(|| {
            black_box(ScriptHostBench::cold_start_once().expect("script-host cold start"));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cold_start_native);
criterion_main!(benches);
