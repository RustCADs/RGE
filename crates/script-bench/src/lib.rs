//! `rge-script-bench` — scripting benchmark suite (v0.0.1 scaffold).
//!
//! Provides the **harness**, the **native-Rust baseline**, and the **output
//! format** for the "fastest script engine" pillar verification per
//! [PLAN.md §5.6](../../plans/PLAN.md). Real engine integration (wasmtime via
//! W04) lands once `runtime-wasmtime-engine` is more than a stub; until then
//! the engine column is wired through [`engine_stub`] and reports a placeholder
//! "not-yet-measured" sentinel.
//!
//! ## Why this crate exists
//!
//! The "1.5× of native" claim in §5.6 needs an unambiguous denominator.  This
//! crate publishes the methodology (see `METHODOLOGY.md`), the workload
//! sources (`workloads.rs`), and the native-Rust reference implementation
//! (`native_baseline.rs`). All numbers downstream of that — engine cold-start,
//! per-tick throughput, hot-reload swap latency, memory overhead — are
//! defined as ratios over the values produced here.
//!
//! ## v0.0.1 scope
//!
//! - Workload definitions: [`workloads`].
//! - Native-Rust baseline: [`native_baseline`].
//! - JSON + Markdown output: [`output`].
//! - Engine integration: stubbed at [`engine_stub`].
//!
//! Comparison vs. Lua/mlua/Wasmer-singlepass/Bevy-extism is **out of scope**
//! for v0.0.1 (post-Phase-3 work per `tasks/W20/PLAN.md`).

pub mod engine_stub;
pub mod native_baseline;
pub mod output;
pub mod workloads;

pub use output::{BenchReport, BenchResult, Engine, Workload};
pub use workloads::{Transform, Vec3, WorkloadId};
