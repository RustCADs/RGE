//! `// SPLIT-EXEMPTION` lint. See PLAN.md §1.3 Rule 3.
//!
//! Walks every `.rs` file under `kernel/`, `crates/`, `runtime/`, `editor/`,
//! and `tools/`. Any file whose line count exceeds the hard cap of 1000 lines
//! must contain a `// SPLIT-EXEMPTION: <reason>` annotation somewhere in its
//! text. If the annotation is absent the file is reported as a violation.
//!
//! The annotation is case-sensitive and must include the colon (`:`). A bare
//! `// SPLIT-EXEMPTION` without a colon is NOT accepted.

use std::path::Path;

use anyhow::{Context, Result};

use crate::common::{iter_rust_files, relativize, source_roots, LintReport, Violation};

/// Hard line-count cap. Files strictly above this value must carry an
/// exemption annotation.
const HARD_CAP: usize = 1000;

/// The annotation substring that exempts a file from the hard cap.
///
/// Must be present verbatim (case-sensitive) anywhere in the file. The
/// colon is required so that a bare `// SPLIT-EXEMPTION` without a reason
/// does not inadvertently exempt a file.
const EXEMPTION_MARKER: &str = "// SPLIT-EXEMPTION:";

/// Check a single Rust source file against the hard-cap rule.
///
/// Returns `Ok(Some(Violation))` when the file exceeds [`HARD_CAP`] lines and
/// does **not** contain [`EXEMPTION_MARKER`]. Returns `Ok(None)` when the file
/// is within the cap or is properly exempted. Propagates I/O errors.
fn check_file(path: &Path, workspace_root: &Path) -> Result<Option<Violation>> {
    let txt =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    let line_count = txt.lines().count();

    if line_count <= HARD_CAP {
        return Ok(None);
    }

    // File exceeds the hard cap — look for an exemption annotation anywhere in
    // the file text.
    if txt.contains(EXEMPTION_MARKER) {
        return Ok(None);
    }

    Ok(Some(Violation {
        file: relativize(path, workspace_root),
        // Point the reader to the first line past the cap so they can jump
        // directly to where the violation begins.
        line: Some(HARD_CAP + 1),
        message: format!(
            "file is {line_count} lines (>1000) without `// SPLIT-EXEMPTION:` annotation"
        ),
    }))
}

/// Run the split-exemption lint against the workspace at `workspace_root`.
///
/// Returns a [`LintReport`] whose violations list is empty when every `.rs`
/// file either fits within the 1000-line hard cap or carries a
/// `// SPLIT-EXEMPTION: <reason>` annotation. Returns an error only on
/// unrecoverable I/O failures.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("split-exemption");

    let roots = source_roots(workspace_root);
    for path in iter_rust_files(&roots) {
        if let Some(violation) = check_file(&path, workspace_root)? {
            report.push(violation);
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Unit tests — exercise `check_file` logic without the full binary.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;

    use super::{check_file, HARD_CAP};

    /// Write `n` lines to a temp file, optionally prefixing the first line
    /// with `// SPLIT-EXEMPTION: test`.
    fn make_temp(n: usize, with_annotation: bool) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("tempfile");
        if with_annotation {
            writeln!(f, "// SPLIT-EXEMPTION: test exemption reason").expect("write");
            for i in 2..=n {
                writeln!(f, "// line {i}").expect("write");
            }
        } else {
            for i in 1..=n {
                writeln!(f, "// line {i}").expect("write");
            }
        }
        f
    }

    #[test]
    fn small_file_passes() {
        let f = make_temp(100, false);
        let root = f.path().parent().unwrap();
        assert!(check_file(f.path(), root).unwrap().is_none());
    }

    #[test]
    fn large_file_with_exemption_passes() {
        let f = make_temp(1500, true);
        let root = f.path().parent().unwrap();
        assert!(check_file(f.path(), root).unwrap().is_none());
    }

    #[test]
    fn large_file_no_exemption_fails() {
        let f = make_temp(1500, false);
        let root = f.path().parent().unwrap();
        let v = check_file(f.path(), root)
            .unwrap()
            .expect("should have violation");
        assert!(v.message.contains("1500 lines"), "message: {}", v.message);
        assert_eq!(v.line, Some(HARD_CAP + 1));
    }

    #[test]
    fn exactly_cap_passes() {
        let f = make_temp(HARD_CAP, false);
        let root = f.path().parent().unwrap();
        assert!(
            check_file(f.path(), root).unwrap().is_none(),
            "exactly {HARD_CAP} lines should pass"
        );
    }

    #[test]
    fn one_over_cap_fails() {
        let f = make_temp(HARD_CAP + 1, false);
        let root = f.path().parent().unwrap();
        assert!(
            check_file(f.path(), root).unwrap().is_some(),
            "{} lines should fail",
            HARD_CAP + 1
        );
    }

    #[test]
    fn annotation_without_colon_does_not_exempt() {
        // A comment that says SPLIT-EXEMPTION but lacks the colon must not exempt.
        let mut f = NamedTempFile::new().expect("tempfile");
        writeln!(f, "// SPLIT-EXEMPTION no colon here").expect("write");
        for i in 2..=(HARD_CAP + 1) {
            writeln!(f, "// line {i}").expect("write");
        }
        let root = f.path().parent().unwrap();
        assert!(
            check_file(f.path(), root).unwrap().is_some(),
            "missing colon must not exempt the file"
        );
    }
}
