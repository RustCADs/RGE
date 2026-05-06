//! Integration tests for the `split-exemption` lint.
//!
//! Each test materialises a minimal synthetic workspace inside a [`TempDir`],
//! runs the binary with the `split-exemption` subcommand, and asserts the
//! expected exit code (0 = pass, 1 = violations).

use std::fs;
use std::path::Path;
use std::process::Command;

/// Path to the compiled binary, injected by Cargo at test time.
const BIN: &str = env!("CARGO_BIN_EXE_rge-tool-architecture-lints");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a minimal `Cargo.toml` at the workspace root so that
/// `common::workspace_root()` can find it.
fn write_workspace_toml(root: &Path) {
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").expect("write Cargo.toml");
}

/// Write a `.rs` file under `kernel/stub/src/` inside `root`.
///
/// The subdirectory is one of the roots scanned by `source_roots`.
fn write_rs_file(root: &Path, filename: &str, content: &str) {
    let dir = root.join("kernel").join("stub").join("src");
    fs::create_dir_all(&dir).expect("create dirs");
    fs::write(dir.join(filename), content).expect("write rs file");
}

/// Build a string that is exactly `n` lines long.
///
/// The lines are syntactically valid (but trivially empty) Rust comments so
/// that the file is unambiguously a `.rs` source file.
fn make_lines(n: usize) -> String {
    (1..=n).map(|i| format!("// line {i}\n")).collect()
}

/// Build a string that is `n` lines long and contains a
/// `// SPLIT-EXEMPTION: <reason>` annotation on line 1.
fn make_lines_with_exemption(n: usize) -> String {
    let mut out = String::from(
        "// SPLIT-EXEMPTION: hand-rolled parser — splitting adds interface friction\n",
    );
    for i in 2..=n {
        out.push_str(&format!("// line {i}\n"));
    }
    out
}

/// Run the `split-exemption` subcommand from `workspace_root` and return the
/// process exit code.
fn run_lint(workspace_root: &Path) -> i32 {
    Command::new(BIN)
        .arg("split-exemption")
        .current_dir(workspace_root)
        .output()
        .expect("spawn lint binary")
        .status
        .code()
        .unwrap_or(-1)
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

/// Case 1 — Negative: a small (100-line) file with no annotation passes.
#[test]
fn small_file_no_annotation_passes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_workspace_toml(root);
    write_rs_file(root, "small.rs", &make_lines(100));

    assert_eq!(
        run_lint(root),
        0,
        "100-line file should pass without annotation"
    );
}

/// Case 2 — Negative: a large (1500-line) file WITH a `// SPLIT-EXEMPTION:`
/// annotation passes.
#[test]
fn large_file_with_annotation_passes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_workspace_toml(root);
    write_rs_file(root, "large_exempt.rs", &make_lines_with_exemption(1500));

    assert_eq!(
        run_lint(root),
        0,
        "1500-line file with exemption annotation should pass"
    );
}

/// Case 3 — Positive: a large (1500-line) file with NO annotation triggers
/// exactly one violation (exit code 1).
#[test]
fn large_file_no_annotation_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_workspace_toml(root);
    write_rs_file(root, "large_noexempt.rs", &make_lines(1500));

    assert_eq!(
        run_lint(root),
        1,
        "1500-line file without annotation should fail"
    );
}

/// Case 4 — Edge: a file of exactly 1000 lines with no annotation passes
/// (the rule is strictly >1000).
#[test]
fn exactly_1000_lines_no_annotation_passes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_workspace_toml(root);
    write_rs_file(root, "exactly_1000.rs", &make_lines(1000));

    assert_eq!(
        run_lint(root),
        0,
        "exactly-1000-line file should pass (cap is >1000)"
    );
}

/// Case 5 — Edge: a file of exactly 1001 lines with no annotation fails.
#[test]
fn exactly_1001_lines_no_annotation_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_workspace_toml(root);
    write_rs_file(root, "exactly_1001.rs", &make_lines(1001));

    assert_eq!(
        run_lint(root),
        1,
        "1001-line file without annotation should fail"
    );
}
