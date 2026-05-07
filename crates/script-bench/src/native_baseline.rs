//! Pure-native-Rust reference implementation of every workload.
//!
//! This module is the **denominator** for the "1.5× of native" target in
//! [PLAN.md §5.6](../../plans/PLAN.md). Anything an engine does is compared
//! against the numbers these functions produce on the same machine, in the
//! same `cargo bench` invocation. Every function here is deliberately
//! **straightforward Rust** — no `unsafe`, no SIMD intrinsics, no
//! `#[inline(always)]` cheats. The point is to publish a fair denominator
//! that any reasonable Rustacean would recognise as "native code".
//!
//! Every entry point takes a `&mut` slice or owns a `Vec` so that the
//! optimiser cannot constant-fold the work away; all benches use
//! `criterion::black_box` on inputs and outputs to seal the same boundary.
//!
//! ## What is *not* here
//!
//! - W3 `cold_start` — there is no native analogue (Rust is AOT). The bench
//!   instead measures the wall time to construct an empty engine context;
//!   for the native baseline we record `0 µs` (instantaneous) so the JSON
//!   schema stays uniform. See `METHODOLOGY.md` §"Cold-start denominator".
//! - W4 `hot_reload_swap` — same reasoning; for native code the swap is a
//!   process restart.  We model it as `mem::swap` on a `Box<dyn Fn>` to
//!   establish a "lower bound — anything an engine does is at least this
//!   much work" baseline.
//! - W5 `memory_overhead` — the "module" is a function pointer; baseline is
//!   `size_of::<fn()>()`. Engine numbers are reported as `RSS_after - RSS_before`.

use std::time::Instant;

use crate::workloads::{Transform, FIXED_DT, SCRIPT_TICK_ITERATIONS};

/// W1 — `script_tick_1m_iters` native baseline.
///
/// Runs the integration kernel `iters` times on a single Transform.
/// Returns the final transform so callers can pin it through `black_box`.
#[must_use]
pub fn script_tick(mut t: Transform, iters: u32) -> Transform {
    for _ in 0..iters {
        t.integrate(FIXED_DT);
    }
    t
}

/// Convenience: run W1 with the canonical iteration count.
#[must_use]
pub fn script_tick_1m(t: Transform) -> Transform {
    script_tick(t, SCRIPT_TICK_ITERATIONS)
}

/// W2 — `per_frame_tick_10k_entities` native baseline.
///
/// One frame = one pass over the buffer applying the integration kernel
/// to every entry. The buffer is mutated in place; the function returns
/// nothing so the caller is forced to `black_box` the slice itself.
pub fn per_frame_tick(entities: &mut [Transform]) {
    for t in entities.iter_mut() {
        t.integrate(FIXED_DT);
    }
}

/// W3 — `cold_start` native baseline.
///
/// Native Rust has no module-load step; the function exists so the JSON
/// row is present and non-NaN. Returns the wall-clock duration of an
/// empty closure call (effectively the timer overhead).
#[must_use]
pub fn cold_start() -> std::time::Duration {
    let start = Instant::now();
    // No-op: this *is* the baseline. We're measuring the overhead of the
    // measurement itself so engine numbers are quoted relative to a real
    // floor and not dressed-up timer noise.
    std::hint::black_box(());
    start.elapsed()
}

/// W4 — `hot_reload_swap` native baseline.
///
/// Native equivalent of swapping a script module: replace one boxed
/// function pointer with another. Run `cycles` times and return the
/// total elapsed duration.
#[must_use]
pub fn hot_reload_swap(cycles: u32) -> std::time::Duration {
    type Tick = Box<dyn Fn(&mut Transform)>;

    let mut current: Tick = Box::new(|t: &mut Transform| t.integrate(FIXED_DT));

    let start = Instant::now();
    for i in 0..cycles {
        // Build "the new module".
        let next: Tick = if i & 1 == 0 {
            Box::new(|t: &mut Transform| t.integrate(FIXED_DT))
        } else {
            Box::new(|t: &mut Transform| t.integrate(FIXED_DT * 0.999))
        };
        // Swap and drop the old.
        let old = std::mem::replace(&mut current, next);
        drop(old);
    }
    let elapsed = start.elapsed();
    // Force the loop body to outlive `start.elapsed()` without tripping
    // `unused_must_use` on the boxed closure.
    drop(std::hint::black_box(current));
    elapsed
}

/// W5 — `memory_overhead` native baseline.
///
/// For native code a "loaded module" is just a function pointer. Returns
/// the bytes of resident state (`size_of::<fn(_)>`).
#[must_use]
pub const fn memory_overhead_bytes_per_module() -> usize {
    std::mem::size_of::<fn(&mut Transform)>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workloads::{generate_entities, Vec3, ENTITY_SEED, PER_FRAME_ENTITY_COUNT};

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "asserting y/z were untouched: input 0.0 + integration of zero velocity must equal exact 0.0; bit-equality is the intended contract"
    )]
    fn script_tick_makes_progress() {
        let t = Transform {
            translation: Vec3::new(0.0, 0.0, 0.0),
            velocity: Vec3::new(1.0, 0.0, 0.0),
        };
        let after = script_tick(t, 1000);
        // 1000 iters at 1/60 dt with v=(1,0,0) ≈ 16.6m on x.
        assert!(after.translation.x > 16.0 && after.translation.x < 17.0);
        assert_eq!(after.translation.y, 0.0);
        assert_eq!(after.translation.z, 0.0);
    }

    #[test]
    fn per_frame_tick_updates_all_entities() {
        let mut buf = generate_entities(PER_FRAME_ENTITY_COUNT, ENTITY_SEED);
        let before: Vec<_> = buf.iter().map(|t| t.translation).collect();
        per_frame_tick(&mut buf);
        for (a, b) in buf.iter().zip(before.iter()) {
            // translation must have moved by velocity*dt unless v == 0.
            let dx = a.translation.x - b.x;
            let expected = a.velocity.x * FIXED_DT;
            assert!((dx - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn hot_reload_swap_runs() {
        let d = hot_reload_swap(10);
        assert!(d.as_nanos() > 0, "swap should take some measurable time");
    }

    #[test]
    fn memory_overhead_is_pointer_sized() {
        assert!(memory_overhead_bytes_per_module() <= 16);
    }
}
