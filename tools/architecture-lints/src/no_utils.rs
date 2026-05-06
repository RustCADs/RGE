//! `no utils.rs / helpers.rs` lint — PLAN.md §1.3 Rule 3.
//!
//! Forbids source files whose base name (case-insensitive) is one of:
//! `utils.rs`, `util.rs`, `helpers.rs`, `helper.rs`.
//!
//! Catch-all "bag of tricks" files are an architecture smell: code that would
//! otherwise live in one of these files should instead live in a module with a
//! descriptive, domain-specific name.

use std::path::Path;

use anyhow::Result;

use crate::common::{iter_rust_files, relativize, source_roots, LintReport, Violation};

/// The set of forbidden base-names (lower-cased for comparison).
const FORBIDDEN: &[&str] = &["utils.rs", "util.rs", "helpers.rs", "helper.rs"];

/// Run the no-utils/helpers filename lint against the workspace.
///
/// Returns a [`LintReport`] with one [`Violation`] per forbidden file found.
// The `Result` wrapper is required by the dispatch pattern in `main.rs` (all
// lint `run` fns share the same `-> Result<LintReport>` signature so the caller
// can use `?`). Clippy would suggest removing it, but we cannot change the
// interface without touching main.rs (which is out of scope for this module).
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("no-utils");

    let roots = source_roots(workspace_root);
    for path in iter_rust_files(&roots) {
        if let Some(name) = path.file_name() {
            let lower = name.to_string_lossy().to_lowercase();
            if FORBIDDEN.contains(&lower.as_str()) {
                report.push(Violation {
                    file: relativize(&path, workspace_root),
                    line: None,
                    message: "forbidden filename per PLAN.md §1.3 Rule 3 (no utils/helpers files)"
                        .to_owned(),
                });
            }
        }
    }

    Ok(report)
}
