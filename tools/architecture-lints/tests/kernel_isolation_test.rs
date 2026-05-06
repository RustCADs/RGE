//! Integration tests for the `kernel-isolation` lint
//! (one-import-path-per-format, PLAN.md §1.6.4).
//!
//! Every test builds a minimal synthetic workspace inside a [`TempDir`], then
//! invokes the compiled binary with the `kernel-isolation` subcommand.
//!
//! Exit-code semantics:
//! - 0 — pass (no violations; missing-metadata is only a stderr warning).
//! - 1 — violations found (format claimed by ≥ 2 `io-*` crates).
//! - 2 — tool error.

use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled binary, injected by Cargo at test-link time.
fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_rge-tool-architecture-lints"))
}

/// Create `base/rel_path` (and any intermediate directories) with `content`.
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

/// Minimal crate `Cargo.toml` **without** `[package.metadata.rge]`.
fn pkg_toml_no_meta(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n")
}

/// Crate `Cargo.toml` with `[package.metadata.rge] formats = [...]`.
fn pkg_toml_with_formats(name: &str, formats: &[&str]) -> String {
    let fmt_list: String = formats.iter().map(|f| format!("\"{f}\", ")).collect();
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.0.1\"\n\
         edition = \"2021\"\n\
         \n\
         [package.metadata.rge]\n\
         formats = [{fmt_list}]\n"
    )
}

/// Run the `kernel-isolation` subcommand from `workspace_dir`.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("kernel-isolation")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Test 1 — Negative: two io-* crates with disjoint format sets → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_disjoint_formats_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["crates/io-foo", "crates/io-bar"]),
    );

    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_with_formats("io-foo", &["gltf"]),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    write(
        root,
        "crates/io-bar/Cargo.toml",
        &pkg_toml_with_formats("io-bar", &["png"]),
    );
    write(root, "crates/io-bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 0, "disjoint formats should exit 0; stdout:\n{stdout}");
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 2 — Negative: workspace with no io-* crates at all → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_no_io_crates_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["kernel/foo", "crates/bar"]),
    );

    write(root, "kernel/foo/Cargo.toml", &pkg_toml_no_meta("foo"));
    write(root, "kernel/foo/src/lib.rs", "");

    write(root, "crates/bar/Cargo.toml", &pkg_toml_no_meta("bar"));
    write(root, "crates/bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 0, "no io-* crates → exit 0; stdout:\n{stdout}");
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 3 — Negative: single io-* crate with formats, no overlap → pass.
// ---------------------------------------------------------------------------

#[test]
fn test_single_io_crate_passes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/io-foo"]));
    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_with_formats("io-foo", &["jpg"]),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "single io-* crate should exit 0; stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 4 — Positive: two io-* crates both claim "gltf" → violation.
// ---------------------------------------------------------------------------

#[test]
fn test_single_format_overlap_is_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["crates/io-foo", "crates/io-bar"]),
    );

    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_with_formats("io-foo", &["gltf"]),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    write(
        root,
        "crates/io-bar/Cargo.toml",
        &pkg_toml_with_formats("io-bar", &["gltf"]),
    );
    write(root, "crates/io-bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 1, "gltf overlap should exit 1; stdout:\n{stdout}");
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("gltf"),
        "expected 'gltf' in violation message; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("io-foo") && stdout.contains("io-bar"),
        "expected both crate names in message; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Positive: overlap on one format within multi-format sets.
//           io-foo = ["png", "jpg"], io-bar = ["jpg", "exr"] → jpg violation.
// ---------------------------------------------------------------------------

#[test]
fn test_multi_format_partial_overlap_is_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["crates/io-foo", "crates/io-bar"]),
    );

    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_with_formats("io-foo", &["png", "jpg"]),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    write(
        root,
        "crates/io-bar/Cargo.toml",
        &pkg_toml_with_formats("io-bar", &["jpg", "exr"]),
    );
    write(root, "crates/io-bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 1, "jpg overlap should exit 1; stdout:\n{stdout}");
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("jpg"),
        "expected 'jpg' in violation message; stdout:\n{stdout}"
    );
    // png and exr are not shared — only one violation (for jpg).
    let violation_count = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .count();
    assert_eq!(
        violation_count, 1,
        "expected exactly 1 violation; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive: rge-io-* (canonical workspace prefix) overlap → violation.
//          Audit-5 carryover regression: the name-prefix check
//          `pkg.name.starts_with("rge-io-")` must fire against real-workspace
//          names. Prior `starts_with("io-")` only path was dead code in
//          production; this test covers the rge-prefixed path explicitly.
// ---------------------------------------------------------------------------

#[test]
fn test_rge_prefixed_io_crates_overlap_detected_via_name_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Note: package directories are NOT named `io-*` here (they live under
    // `pkgs/`), so the manifest-path fallback can't catch this. Only the
    // name-prefix `starts_with("rge-io-")` check will fire.
    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["pkgs/foo", "pkgs/bar"]),
    );

    write(
        root,
        "pkgs/foo/Cargo.toml",
        &pkg_toml_with_formats("rge-io-foo", &["gltf"]),
    );
    write(root, "pkgs/foo/src/lib.rs", "");

    write(
        root,
        "pkgs/bar/Cargo.toml",
        &pkg_toml_with_formats("rge-io-bar", &["gltf"]),
    );
    write(root, "pkgs/bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "rge-io-* gltf overlap should exit 1 via name-path; stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("gltf"),
        "expected 'gltf' in violation message; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rge-io-foo") && stdout.contains("rge-io-bar"),
        "expected both rge-io- crate names in message; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Negative (Option B): io-* crate with NO metadata → exit 0, but
//           a warning is printed to stderr.
// ---------------------------------------------------------------------------

#[test]
fn test_missing_metadata_is_warning_not_violation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/io-foo"]));
    // No [package.metadata.rge] section at all.
    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_no_meta("io-foo"),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "missing metadata should exit 0 (Option B); stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("PASS"), "expected PASS; stdout:\n{stdout}");
    assert!(
        stderr.contains("warning:") && stderr.contains("io-foo"),
        "expected a warning mentioning io-foo in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("package.metadata.rge.formats"),
        "expected the missing-metadata key name in stderr:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 8 — Positive: manifest-path fallback ALONE detects io-* crates.
//          Audit-2 carryover: every prior fixture used bare `io-*` package
//          names, so both the name-prefix AND manifest-path branches fired
//          simultaneously. A regression in the manifest-path-only branch
//          (lines 59-69 of `is_io_crate`) could go undetected without this
//          test. Here the package names (`rge-loader`, `rge-saver`) start
//          with neither `io-` NOR `rge-io-`, so ONLY the manifest-path
//          fallback can identify these as io-crates.
// ---------------------------------------------------------------------------

#[test]
fn test_manifest_path_fallback_alone_detects_io_crate() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Note: package directories ARE under `crates/io-*/` (so the manifest
    // path-component fallback can fire), but the crate NAMES intentionally
    // do NOT start with `io-` or `rge-io-`. Only the manifest-path fallback
    // can identify these as io-crates.
    write(
        root,
        "Cargo.toml",
        &workspace_toml(&["crates/io-foo", "crates/io-bar"]),
    );

    write(
        root,
        "crates/io-foo/Cargo.toml",
        &pkg_toml_with_formats("rge-loader", &["gltf"]),
    );
    write(root, "crates/io-foo/src/lib.rs", "");

    write(
        root,
        "crates/io-bar/Cargo.toml",
        &pkg_toml_with_formats("rge-saver", &["gltf"]),
    );
    write(root, "crates/io-bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "gltf overlap detected via manifest-path fallback alone should exit 1; \
         stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL; stdout:\n{stdout}");
    assert!(
        stdout.contains("gltf"),
        "expected 'gltf' in violation message; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rge-loader") && stdout.contains("rge-saver"),
        "expected both crate names in message (manifest-path identification); \
         stdout:\n{stdout}"
    );
}
