//! Bench: W5 — `memory_overhead`.
//!
//! Resident bytes per loaded script module. The native-Rust baseline is the
//! cost of allocating one boxed closure (`Box<dyn Fn(...)>`) — the smallest
//! "loadable module" representation Rust offers. The criterion timer
//! measures *allocation+drop* wall time as a proxy; the **size in bytes**
//! reported to `BASELINE.md` comes from
//! [`rge_script_bench::native_baseline::memory_overhead_bytes_per_module`]
//! and is constant per architecture (one function pointer = 8 B on `x86_64`).
//!
//! A future revision will replace this with a real RSS-delta probe once
//! per platform — see `METHODOLOGY.md` §"W5".

#![allow(missing_docs)] // criterion_group! generates an undocumented fn.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rge_script_bench::native_baseline::memory_overhead_bytes_per_module;
use rge_script_bench::workloads::{Transform, FIXED_DT};

type LoadedModule = Box<dyn Fn(&mut Transform)>;

#[inline(never)]
fn load_module() -> LoadedModule {
    Box::new(|t: &mut Transform| t.integrate(FIXED_DT))
}

fn bench_memory_overhead_native(c: &mut Criterion) {
    // Sanity: the published byte count is fixed per architecture.
    let _ = black_box(memory_overhead_bytes_per_module());

    let mut group = c.benchmark_group("memory_overhead");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(50);

    group.bench_function("native_rust", |b| {
        b.iter(|| {
            // Allocate a "loaded module" and immediately drop it.
            // criterion measures the per-iteration wall time; the
            // per-module byte cost is published separately.
            let module: LoadedModule = load_module();
            drop(black_box(module));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_memory_overhead_native);
criterion_main!(benches);
