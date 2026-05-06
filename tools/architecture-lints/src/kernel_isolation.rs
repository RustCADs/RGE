//! **One-import-path-per-format lint** — enforces PLAN.md §1.6.4.
//!
//! # Naming mismatch
//!
//! This module is named `kernel_isolation` because that is the filename assigned
//! in `plans/fileandfolderstructure.md §12`.  The actual lint it implements is
//! the **one-import-path-per-format** rule from Status.md / PLAN.md §1.6.4,
//! which has nothing to do with kernel isolation per se.  The mismatch is
//! intentional — other lint modules (and `main.rs`) already reference this file
//! by name, so renaming it would break those callers.
//!
//! # Rule
//!
//! For each binary asset format (e.g. `gltf`, `glb`, `png`, `jpg`, `exr`,
//! `obj`, `stl`, `step`, `wav`, …) there must be **exactly one** workspace
//! `io-*` crate that handles it.  No two `io-*` crates may claim the same
//! format.
//!
//! # Format ownership declaration
//!
//! Each `io-*` crate opts in by adding to its own `Cargo.toml`:
//!
//! ```toml
//! [package.metadata.rge]
//! formats = ["gltf", "glb"]
//! ```
//!
//! The strings are extension names: lower-case, no leading dot.
//!
//! # Missing-metadata policy (Option B — pragmatic)
//!
//! If an `io-*` crate does **not** carry `[package.metadata.rge]` yet, the
//! lint emits a `warning:` to *stderr* and continues.  This allows the real
//! workspace (where no `io-*` crate has opted in yet) to exit 0, while still
//! catching overlaps the moment two crates declare conflicting formats.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::common::{cargo_metadata, relativize, workspace_members, LintReport, Violation};

/// Identify `io-*` packages: name starts with `io-`, or manifest lives under
/// `crates/io-*/Cargo.toml`.
fn is_io_crate(pkg: &cargo_metadata::Package, workspace_root: &Path) -> bool {
    if pkg.name.starts_with("io-") {
        return true;
    }
    // Fallback: check manifest path component.
    if let Ok(rel) = pkg.manifest_path.as_std_path().strip_prefix(workspace_root) {
        let mut comps = rel.components();
        // Skip leading `crates/`
        if comps.next().and_then(|c| c.as_os_str().to_str()) == Some("crates") {
            if let Some(dir) = comps.next().and_then(|c| c.as_os_str().to_str()) {
                if dir.starts_with("io-") {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract the `package.metadata.rge.formats` array from a package's metadata.
///
/// Returns `None` when the key is absent (not an error — see Option B above).
#[must_use]
fn extract_formats(pkg: &cargo_metadata::Package) -> Option<Vec<String>> {
    pkg.metadata
        .get("rge")
        .and_then(|rge| rge.get("formats"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_lowercase))
                .collect::<Vec<_>>()
        })
}

/// Run the one-import-path-per-format lint.
///
/// Exit semantics (propagated by `main.rs`):
/// - 0 — no violations (missing metadata only emits warnings to stderr).
/// - 1 — at least one format claimed by ≥ 2 `io-*` crates.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("kernel-isolation");

    let meta = cargo_metadata(workspace_root)?;
    let members = workspace_members(&meta);

    // Map: format_extension -> Vec<(crate_name, manifest_path)>
    let mut format_owners: HashMap<String, Vec<(String, PathBuf)>> = HashMap::new();

    for pkg in members {
        if !is_io_crate(pkg, workspace_root) {
            continue;
        }

        match extract_formats(pkg) {
            None => {
                // Option B: warn to stderr, not a violation.
                eprintln!(
                    "warning: io-* crate `{}` missing `package.metadata.rge.formats` declaration",
                    pkg.name
                );
            }
            Some(formats) => {
                let manifest = relativize(pkg.manifest_path.as_std_path(), workspace_root);
                for ext in formats {
                    format_owners
                        .entry(ext)
                        .or_default()
                        .push((pkg.name.clone(), manifest.clone()));
                }
            }
        }
    }

    // Report any format claimed by ≥ 2 crates.
    let mut overlapping: Vec<(String, Vec<(String, PathBuf)>)> = format_owners
        .into_iter()
        .filter(|(_, owners)| owners.len() >= 2)
        .collect();

    // Deterministic output order.
    overlapping.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (ext, mut owners) in overlapping {
        owners.sort_by(|(a, _), (b, _)| a.cmp(b));

        let crate_list = owners
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        // Point `file` at the alphabetically-first manifest.
        let file = owners[0].1.clone();

        report.push(Violation {
            file,
            line: None,
            message: format!(
                "format `{ext}` is claimed by multiple io-* crates: \
                 {crate_list} (PLAN §1.6.4 — one importer per format)"
            ),
        });
    }

    Ok(report)
}
