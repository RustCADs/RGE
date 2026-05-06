//! Integration tests for the `failure-class` lint (PLAN.md §1.13 / §13.12).
//!
//! Each test builds a minimal synthetic workspace inside a [`tempfile::TempDir`]
//! and invokes the compiled binary with the `failure-class` subcommand.
//!
//! Exit-code semantics:
//! - 0 — pass (no violations).
//! - 1 — at least one violation.
//! - 2 — tool error.

use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Absolute path to the compiled binary, injected by Cargo at link time.
fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_rge-tool-architecture-lints"))
}

/// Create `base/rel` (and all intermediate directories) with `content`.
fn write(base: &Path, rel: &str, content: &str) {
    let abs = base.join(rel);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("create_dir_all {}: {e}", abs.display()));
    }
    fs::write(&abs, content).unwrap_or_else(|e| panic!("write {}: {e}", abs.display()));
}

/// Workspace-root `Cargo.toml` listing the given member paths.
fn workspace_toml(members: &[&str]) -> String {
    let list: String = members.iter().map(|m| format!("    \"{m}\",\n")).collect();
    format!("[workspace]\nresolver = \"2\"\nmembers = [\n{list}]\n")
}

/// Minimal crate `Cargo.toml`.
fn pkg_toml(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n")
}

/// Run the `failure-class` subcommand from `workspace_dir`.
///
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("failure-class")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Test 1 — Negative: kernel crate with a valid declaration → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_kernel_crate_valid_declaration_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["kernel/foo"]));
    write(root, "kernel/foo/Cargo.toml", &pkg_toml("foo"));
    write(
        root,
        "kernel/foo/src/lib.rs",
        "//! Foo kernel crate.\n//!\n//! Failure class: recoverable\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "kernel crate with valid declaration should pass; stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 2 — Negative: crates crate with a valid declaration → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_crates_crate_valid_declaration_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/bar"]));
    write(root, "crates/bar/Cargo.toml", &pkg_toml("bar"));
    write(
        root,
        "crates/bar/src/lib.rs",
        "//! Bar crate.\n//!\n//! Failure class: plugin-fatal\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "crates crate with valid declaration should pass; stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 3 — Negative: comma-separated multi-value declaration → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_valid_classes_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["kernel/multi"]));
    write(root, "kernel/multi/Cargo.toml", &pkg_toml("multi"));
    write(
        root,
        "kernel/multi/src/lib.rs",
        "//! Multi crate.\n//!\n//! Failure class: recoverable, snapshot-recoverable\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "multi-value valid declaration should pass; stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 4 — Negative: Tier::Other crate (tools/*) with no declaration → pass
//           (tools are not Tier 1 or 2 and are not checked).
// ---------------------------------------------------------------------------

#[test]
fn test_tools_crate_without_declaration_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["tools/helper"]));
    write(root, "tools/helper/Cargo.toml", &pkg_toml("helper"));
    write(
        root,
        "tools/helper/src/lib.rs",
        "//! Helper tool — intentionally has no Failure class line.\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "tools crate not in Tier 1/2 should not be checked; stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 5 — Positive: kernel crate lib.rs with no failure-class line → violation.
// ---------------------------------------------------------------------------

#[test]
fn test_missing_declaration_is_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["kernel/nodecl"]));
    write(root, "kernel/nodecl/Cargo.toml", &pkg_toml("nodecl"));
    // lib.rs has doc comments but no Failure class line.
    write(
        root,
        "kernel/nodecl/src/lib.rs",
        "//! Some doc.\n//!\n//! More docs without the required declaration.\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "missing declaration should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("nodecl"),
        "expected crate name in violation message; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.13"),
        "expected PLAN reference in message; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive: lib.rs has an invalid class value → violation.
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_class_value_is_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/badval"]));
    write(root, "crates/badval/Cargo.toml", &pkg_toml("badval"));
    write(
        root,
        "crates/badval/src/lib.rs",
        "//! Badval crate.\n//!\n//! Failure class: catastrophic\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "invalid failure class should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("catastrophic"),
        "expected the bad value in violation message; stdout:\n{stdout}"
    );
    // The violation message must name the valid set.
    assert!(
        stdout.contains("recoverable"),
        "expected valid classes listed in message; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Positive: comma list with one invalid value → violation.
// ---------------------------------------------------------------------------

#[test]
fn test_mixed_valid_and_invalid_class_is_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["kernel/mixed"]));
    write(root, "kernel/mixed/Cargo.toml", &pkg_toml("mixed"));
    write(
        root,
        "kernel/mixed/src/lib.rs",
        "//! Mixed crate.\n//!\n//! Failure class: recoverable, oops\n",
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "one bad value in comma list should be a violation; stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("oops"),
        "expected the bad value `oops` in violation message; stdout:\n{stdout}"
    );
    // Exactly one violation (only `oops` is bad; `recoverable` is fine).
    let violation_lines = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .count();
    assert_eq!(
        violation_lines, 1,
        "expected exactly 1 violation; stdout:\n{stdout}"
    );
}
