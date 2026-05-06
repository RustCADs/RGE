//! Output format: JSON for CI ingestion + Markdown summary for `BASELINE.md`.
//!
//! The JSON schema is intentionally small and stable so the CI gate
//! described in [PLAN.md §13.3](../../plans/PLAN.md) can ratchet on it
//! without an upgrade dance. Field names match the keys produced by
//! [`crate::workloads::WorkloadId::as_str`].
//!
//! ## JSON shape (v1)
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "produced_by": "rge-script-bench/0.0.1",
//!   "host": { "os": "windows", "cpu_arch": "x86_64" },
//!   "results": [
//!     {
//!       "workload": "script_tick_1m_iters",
//!       "engine":   "native_rust",
//!       "metric":   "wall_time",
//!       "unit":     "nanoseconds_per_op",
//!       "value":    1234.5,
//!       "samples":  100
//!     }
//!   ]
//! }
//! ```
//!
//! Engines other than `native_rust` report `value: null` until W04 lands.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// Schema version embedded in every JSON document. Bump on breaking changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Workload identifier (string form, kebab-case). Mirrors
/// [`crate::workloads::WorkloadId::as_str`].
pub type Workload = String;

/// Engine identifier. v0.0.1 publishes only the native baseline; further
/// engines (`wasmtime_cranelift`, `wasmtime_singlepass`, `mlua`,
/// `wasmer_singlepass`, `bevy_extism`) come online post-W04.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// Pure-Rust reference implementation (the "1.5×" denominator).
    NativeRust,
    /// Wasmtime + Cranelift JIT — populated post-W04.
    WasmtimeCranelift,
    /// Wasmtime singlepass compiler — populated post-W04.
    WasmtimeSinglepass,
    /// Lua via mlua — comparison row, post-Phase-3.
    Mlua,
    /// Wasmer singlepass — comparison row, post-Phase-3.
    WasmerSinglepass,
    /// Bevy + extism — comparison row, post-Phase-3.
    BevyExtism,
}

impl Engine {
    /// Stable kebab-case string used in JSON and Markdown output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeRust => "native_rust",
            Self::WasmtimeCranelift => "wasmtime_cranelift",
            Self::WasmtimeSinglepass => "wasmtime_singlepass",
            Self::Mlua => "mlua",
            Self::WasmerSinglepass => "wasmer_singlepass",
            Self::BevyExtism => "bevy_extism",
        }
    }
}

/// One measurement row in the report. `value` is `None` when the engine is
/// not yet wired (post-W04 work).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    /// Workload identifier (kebab-case).
    pub workload: Workload,
    /// Which engine produced this row.
    pub engine: Engine,
    /// Metric name (`wall_time`, `peak_rss`, `swap_latency_p95`, ...).
    pub metric: String,
    /// Unit string. Stable values: `nanoseconds_per_op`, `bytes`,
    /// `nanoseconds_total`.
    pub unit: String,
    /// Measured value. `None` means "not yet implemented" (engine column
    /// not wired). Never `NaN`.
    pub value: Option<f64>,
    /// Number of criterion samples behind this number, when applicable.
    pub samples: Option<u32>,
}

impl BenchResult {
    /// Convenience: native-Rust row with a measured value.
    #[must_use]
    pub fn native(
        workload: impl Into<String>,
        metric: impl Into<String>,
        unit: impl Into<String>,
        value: f64,
        samples: u32,
    ) -> Self {
        Self {
            workload: workload.into(),
            engine: Engine::NativeRust,
            metric: metric.into(),
            unit: unit.into(),
            value: Some(value),
            samples: Some(samples),
        }
    }

    /// Placeholder row for a not-yet-wired engine.
    #[must_use]
    pub fn pending(
        workload: impl Into<String>,
        engine: Engine,
        metric: impl Into<String>,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            workload: workload.into(),
            engine,
            metric: metric.into(),
            unit: unit.into(),
            value: None,
            samples: None,
        }
    }
}

/// Host-machine fingerprint. Coarse on purpose: we want runs from the same
/// developer's box to deduplicate, not to track individual silicon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    /// `windows`, `linux`, `macos`, ...
    pub os: String,
    /// `x86_64`, `aarch64`, ...
    pub cpu_arch: String,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            cpu_arch: std::env::consts::ARCH.to_string(),
        }
    }
}

/// Top-level report. Serialised as the JSON document consumed by the CI gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    /// Schema version. See [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// `rge-script-bench/<version>` for downstream debugging.
    pub produced_by: String,
    /// Host fingerprint.
    pub host: Host,
    /// All measurements in this report.
    pub results: Vec<BenchResult>,
}

impl BenchReport {
    /// Construct an empty report tagged with the current crate version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            produced_by: format!("rge-script-bench/{}", env!("CARGO_PKG_VERSION")),
            host: Host::default(),
            results: Vec::new(),
        }
    }

    /// Append a result row.
    pub fn push(&mut self, r: BenchResult) {
        self.results.push(r);
    }

    /// Serialise to a pretty-printed JSON `String`. Stable ordering: row
    /// order matches `push` order.
    ///
    /// # Errors
    /// Propagates `serde_json` errors. None expected for the schema we own.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Render as a Markdown summary suitable for `BASELINE.md`.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "| workload | engine | metric | unit | value | samples |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|");
        for r in &self.results {
            let value = match r.value {
                Some(v) => format!("{v}"),
                None => "—".to_string(),
            };
            let samples = match r.samples {
                Some(n) => format!("{n}"),
                None => "—".to_string(),
            };
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                r.workload,
                r.engine.as_str(),
                r.metric,
                r.unit,
                value,
                samples
            );
        }
        out
    }
}

impl Default for BenchReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_round_trips_through_json() {
        let mut r = BenchReport::new();
        r.push(BenchResult::native(
            "script_tick_1m_iters",
            "wall_time",
            "nanoseconds_per_op",
            12.34,
            100,
        ));
        let s = r.to_json().expect("serialise");
        let back: BenchReport = serde_json::from_str(&s).expect("deserialise");
        assert_eq!(back.schema_version, SCHEMA_VERSION);
        assert_eq!(back.results.len(), 1);
        assert_eq!(back.results[0].engine, Engine::NativeRust);
    }

    #[test]
    fn pending_rows_serialise_with_null_value() {
        let mut r = BenchReport::new();
        r.push(BenchResult::pending(
            "cold_start",
            Engine::WasmtimeCranelift,
            "wall_time",
            "nanoseconds_total",
        ));
        let s = r.to_json().expect("serialise");
        assert!(
            s.contains("\"value\": null"),
            "expected null value, got: {s}"
        );
    }

    #[test]
    fn markdown_has_header_and_one_row_per_result() {
        let mut r = BenchReport::new();
        r.push(BenchResult::native(
            "script_tick_1m_iters",
            "wall_time",
            "nanoseconds_per_op",
            42.0,
            10,
        ));
        let md = r.to_markdown();
        assert!(md.contains("| workload |"));
        assert!(md.contains("script_tick_1m_iters"));
        assert!(md.contains("native_rust"));
    }
}
