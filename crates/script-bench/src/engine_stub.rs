//! Local placeholder for `runtime-wasmtime-engine` (W04).
//!
//! W04 is not merged at v0.0.1; this module exists so the criterion benches
//! can import a *named* engine and emit "pending" rows in the JSON report
//! without dragging an unfinished crate into the dependency graph. Once W04
//! lands, the bench files swap their import from `engine_stub::*` to the
//! real engine and the rest of the harness keeps working.
//!
//! Every function here returns a hardcoded number. **Tests use these
//! numbers directly. Real bench runs must mark the engine row as
//! [`crate::output::BenchResult::pending`] regardless of what these
//! placeholders return.**

use std::time::Duration;

/// Placeholder for `Engine::new()`. Returns a unit struct.
#[must_use]
pub fn new_engine() -> EngineStub {
    EngineStub
}

/// Opaque placeholder type — the real version owns a `wasmtime::Engine`.
#[derive(Debug)]
pub struct EngineStub;

/// Placeholder cold-start latency (1 ms). Reported as **pending** in the JSON
/// output; only the type signature is real.
#[must_use]
pub fn cold_start_latency(_e: &EngineStub) -> Duration {
    Duration::from_millis(1)
}

/// Placeholder per-tick cost (1 ns).
#[must_use]
pub fn per_tick_ns(_e: &EngineStub) -> u64 {
    1
}

/// Placeholder hot-reload swap cost (10 µs).
#[must_use]
pub fn swap_latency(_e: &EngineStub) -> Duration {
    Duration::from_micros(10)
}

/// Placeholder memory overhead per loaded module (256 KiB).
#[must_use]
pub const fn memory_overhead_bytes() -> usize {
    256 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_return_nonzero_numbers() {
        let e = new_engine();
        assert!(cold_start_latency(&e) > Duration::ZERO);
        assert!(per_tick_ns(&e) > 0);
        assert!(swap_latency(&e) > Duration::ZERO);
        assert!(memory_overhead_bytes() > 0);
    }
}
