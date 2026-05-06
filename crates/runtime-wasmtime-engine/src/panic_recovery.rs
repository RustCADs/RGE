// adapted from rustforge::crates::runtime-wasmtime on 2026-05-05 — engine_wasmtime feature activated
//! WASM trap → diagnostic. After a trap the instance is quarantined
//! and the editor continues running. This is the W04 deliverable
//! "Panic recovery (trap → diagnostic; instance quarantined)".

/// One trap report recorded by the engine. Plugin-id is the blake3
/// of the original `.wasm` bytes — content-addressable so the editor
/// can deduplicate reports across hot-reload swaps.
#[derive(Debug, Clone)]
pub struct PanicReport {
    /// Plugin id (blake3 of the original `.wasm` bytes).
    pub plugin_id: blake3::Hash,
    /// Human-readable trap message (typically wasmtime's `Trap::to_string()`).
    pub message: String,
}

/// Per-engine registry of all traps observed since the engine was
/// constructed. The editor drains the registry once per frame to
/// surface diagnostics in the message log without blocking the wasm
/// execution loop.
#[derive(Default, Debug)]
pub struct PanicRegistry {
    reports: Vec<PanicReport>,
}

impl PanicRegistry {
    /// Append a trap report.
    pub fn push(&mut self, report: PanicReport) {
        self.reports.push(report);
    }

    /// Drain all currently-buffered reports, leaving the registry empty.
    pub fn drain(&mut self) -> Vec<PanicReport> {
        std::mem::take(&mut self.reports)
    }

    /// Number of currently-buffered reports.
    pub fn len(&self) -> usize {
        self.reports.len()
    }

    /// True iff no reports are buffered.
    pub fn is_empty(&self) -> bool {
        self.reports.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_buffers_reports() {
        let mut r = PanicRegistry::default();
        assert!(r.is_empty());
        r.push(PanicReport {
            plugin_id: blake3::hash(b"x"),
            message: "div by zero".into(),
        });
        assert_eq!(r.len(), 1);
        let drained = r.drain();
        assert_eq!(drained.len(), 1);
        assert!(r.is_empty());
    }
}
