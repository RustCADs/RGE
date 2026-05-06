//! **Failure-class declaration lint** — enforces PLAN.md §1.13 / §13.12.
//!
//! Failure class: recoverable
//!
//! # Rule
//!
//! Every Tier-1 (`kernel/*`) and Tier-2 (`crates/*`) crate's `src/lib.rs` must
//! contain at least one `//! Failure class: <kind>` line in its module-level
//! doc-comment block.  Valid failure classes are:
//!
//! - `recoverable`
//! - `snapshot-recoverable`
//! - `plugin-fatal`
//! - `session-fatal`
//! - `kernel-fatal`
//!
//! Multiple values may appear on a single line, comma-separated:
//! ```text
//! //! Failure class: recoverable, snapshot-recoverable
//! ```
//!
//! # Missing declarations today
//!
//! As of the initial rollout every Tier-1 and Tier-2 crate currently lacks this
//! declaration, so running the lint against the live workspace will report a large
//! number of violations.  This is intentional — the lint is operating correctly.
//! The orchestrator (CI) will add per-crate exemptions to `exemptions.toml` while
//! the rollout progresses, and remove them as each crate gains a declaration.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::common::{
    cargo_metadata, classify, relativize, workspace_members, Exemptions, LintReport, Tier,
    Violation,
};

/// Lint identifier used in `LintReport` and in the exemptions registry.
const LINT_NAME: &str = "failure-class";

/// The closed set of valid failure-class values (case-sensitive).
const VALID_CLASSES: &[&str] = &[
    "recoverable",
    "snapshot-recoverable",
    "plugin-fatal",
    "session-fatal",
    "kernel-fatal",
];

/// Prefix that introduces a failure-class declaration line.
///
/// The regex-equivalent is `^//!\s*Failure class\s*:\s*(.+?)\s*$`; we implement
/// this as a manual string search to avoid pulling in the `regex` crate.
const DECL_PREFIX_RAW: &str = "//!";
const DECL_KEYWORD: &str = "Failure class";

/// Parse a single `//!` line and return the trimmed value list if it is a
/// failure-class declaration, or `None` otherwise.
///
/// Matching is case-sensitive on `"Failure class"` and on the class names.
/// Whitespace around the colon and around each comma-separated value is ignored.
#[must_use]
fn parse_declaration_line(line: &str) -> Option<Vec<&str>> {
    // Must start with `//!`
    let after_prefix = line.strip_prefix(DECL_PREFIX_RAW)?;
    // The remainder (trimmed) must start with "Failure class"
    let trimmed = after_prefix.trim_start();
    let after_keyword = trimmed.strip_prefix(DECL_KEYWORD)?;
    // After the keyword must come optional whitespace then ':'
    let after_colon = after_keyword.trim_start().strip_prefix(':')?;
    // Everything to the right of the colon is the value list.
    let values_raw = after_colon.trim();
    // Split on commas, trim each token.
    let values: Vec<&str> = values_raw.split(',').map(str::trim).collect();
    Some(values)
}

/// Find the `src/lib.rs` for a package.
///
/// Prefers the actual lib target's `src_path` when available; falls back to
/// `manifest_path.parent().join("src/lib.rs")`.
#[must_use]
fn find_lib_rs(pkg: &cargo_metadata::Package) -> PathBuf {
    // Walk targets to find the lib target.
    for target in &pkg.targets {
        if target.kind.iter().any(|k| k == "lib") {
            return target.src_path.as_std_path().to_path_buf();
        }
    }
    // Fallback: manifest sibling.
    pkg.manifest_path
        .as_std_path()
        .parent()
        .expect("manifest_path has parent")
        .join("src/lib.rs")
}

/// Run the failure-class declaration lint.
///
/// Returns a [`LintReport`] whose violations list is empty when every Tier-1 and
/// Tier-2 crate has a valid failure-class declaration in its `src/lib.rs`.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new(LINT_NAME);
    let exemptions = Exemptions::load(workspace_root)?;

    let meta = cargo_metadata(workspace_root)?;
    let members = workspace_members(&meta);

    for pkg in &members {
        let tier = classify(pkg, workspace_root);
        if !matches!(tier, Tier::One | Tier::Two) {
            continue;
        }

        let lib_rs = find_lib_rs(pkg);
        let manifest_rel = relativize(pkg.manifest_path.as_std_path(), workspace_root);

        // Use the manifest path for exemption lookup (matches the `file` we
        // store in Violation).
        if exemptions.is_exempt(LINT_NAME, &manifest_rel) {
            continue;
        }

        // If lib.rs does not exist we cannot check it — treat as missing.
        if !lib_rs.is_file() {
            report.push(Violation {
                file: manifest_rel.clone(),
                line: None,
                message: format!(
                    "crate `{}` has no `src/lib.rs`; cannot verify failure-class declaration \
                     (PLAN §1.13)",
                    pkg.name
                ),
            });
            continue;
        }

        let content = std::fs::read_to_string(&lib_rs)
            .with_context(|| format!("reading {}", lib_rs.display()))?;

        let lib_rs_rel = relativize(&lib_rs, workspace_root);
        check_lib_rs(&pkg.name, &manifest_rel, &lib_rs_rel, &content, &mut report);
    }

    Ok(report)
}

/// Inspect the content of a `lib.rs` for a valid failure-class declaration and
/// push any violations into `report`.
fn check_lib_rs(
    pkg_name: &str,
    manifest_rel: &Path,
    lib_rs: &Path,
    content: &str,
    report: &mut LintReport,
) {
    // Scan every line for a declaration (not just the leading block — this is
    // more forgiving and still meets the spec).
    let mut found_decl = false;
    let mut has_invalid = false;

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1; // 1-based

        let Some(values) = parse_declaration_line(line) else {
            continue;
        };

        found_decl = true;

        for value in values {
            if !VALID_CLASSES.contains(&value) {
                has_invalid = true;
                report.push(Violation {
                    file: lib_rs.to_path_buf(),
                    line: Some(line_no),
                    message: format!(
                        "crate `{pkg_name}` has invalid failure class `{value}`; \
                         valid: recoverable, snapshot-recoverable, plugin-fatal, \
                         session-fatal, kernel-fatal"
                    ),
                });
            }
        }
    }

    if !found_decl && !has_invalid {
        report.push(Violation {
            file: manifest_rel.to_path_buf(),
            line: None,
            message: format!(
                "crate `{pkg_name}` is missing required \
                 `//! Failure class: <kind>` declaration in lib.rs (PLAN §1.13)"
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Unit tests (internal helpers only)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parses_single_class() {
        let vals = parse_declaration_line("//! Failure class: recoverable").unwrap();
        assert_eq!(vals, ["recoverable"]);
    }

    #[test]
    fn parses_multiple_classes() {
        let vals =
            parse_declaration_line("//! Failure class: recoverable, snapshot-recoverable").unwrap();
        assert_eq!(vals, ["recoverable", "snapshot-recoverable"]);
    }

    #[test]
    fn parses_extra_whitespace() {
        let vals = parse_declaration_line("//!   Failure class  :  session-fatal  ").unwrap();
        assert_eq!(vals, ["session-fatal"]);
    }

    #[test]
    fn ignores_non_declaration_line() {
        assert!(parse_declaration_line("//! Some other doc line").is_none());
        assert!(parse_declaration_line("// Failure class: recoverable").is_none());
        assert!(parse_declaration_line("").is_none());
    }

    #[test]
    fn wrong_case_keyword_not_parsed() {
        // "failure class" lowercase must not match.
        assert!(parse_declaration_line("//! failure class: recoverable").is_none());
    }
}
