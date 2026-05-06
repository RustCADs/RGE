//! Integration tests for the `no-utils` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `no-utils` subcommand.
//! Exit code semantics: 0 = pass (no violations), 1 = violations found, 2 = tool error.

use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled binary — Cargo sets this env var for integration tests.
fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_rge-tool-architecture-lints"))
}

/// Write `content` to `base/rel_path`, creating intermediate directories.
fn write_file(base: &Path, rel_path: &str, content: &str) {
    let abs = base.join(rel_path);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create_dir_all");
    }
    fs::write(&abs, content).unwrap_or_else(|e| panic!("write {}: {e}", abs.display()));
}

/// Invoke the binary against `workspace_dir` with the `no-utils` subcommand.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("no-utils")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

/// Minimal workspace root `Cargo.toml` — `[workspace]` present so `workspace_root()` finds it.
fn root_toml() -> &'static str {
    "[workspace]\nmembers = []\n"
}

// ---------------------------------------------------------------------------
// Test 1 — Negative: no forbidden filenames → exit 0.
// ---------------------------------------------------------------------------

#[test]
fn test_clean_workspace_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(root, "kernel/foo/src/lib.rs", "");
    write_file(root, "crates/bar/src/widget.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 0, "clean workspace should exit 0; stdout:\n{stdout}");
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Positive: utils.rs present → exit 1, one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_utils_rs_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(root, "kernel/foo/src/utils.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 1, "utils.rs should trigger exit 1; stdout:\n{stdout}");
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("utils.rs"),
        "expected filename in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Positive: helpers.rs present → exit 1, one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_helpers_rs_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(root, "crates/bar/src/helpers.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "helpers.rs should trigger exit 1; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("helpers.rs"),
        "expected filename in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Positive: case-insensitive match (Utils.rs) → exit 1.
// ---------------------------------------------------------------------------

#[test]
fn test_utils_rs_case_insensitive_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    // On case-insensitive file systems (Windows) this is the same as utils.rs,
    // but the lint's lower-case comparison must handle it regardless.
    write_file(root, "kernel/foo/src/Utils.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "Utils.rs (capital U) should trigger exit 1; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Negative: common.rs is NOT in the forbidden set → exit 0.
// ---------------------------------------------------------------------------

#[test]
fn test_common_rs_is_allowed() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    // The lint scaffolding itself uses common.rs; it must not be forbidden.
    write_file(root, "kernel/foo/src/common.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "common.rs should be allowed (exit 0); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive: util.rs (singular) → exit 1.
// ---------------------------------------------------------------------------

#[test]
fn test_util_rs_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(root, "kernel/foo/src/util.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 1, "util.rs should trigger exit 1; stdout:\n{stdout}");
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Positive: helper.rs (singular) → exit 1.
// ---------------------------------------------------------------------------

#[test]
fn test_helper_rs_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(root, "crates/bar/src/helper.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "helper.rs should trigger exit 1; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
}
