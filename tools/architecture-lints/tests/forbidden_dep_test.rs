//! Integration tests for the `forbidden-dep` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `forbidden-dep` subcommand.
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

/// Invoke the binary against `workspace_dir` with the `forbidden-dep` subcommand.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("forbidden-dep")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

/// Build a workspace root `Cargo.toml` listing the given member paths.
fn root_toml(members: &[&str]) -> String {
    let member_list: Vec<String> = members.iter().map(|m| format!("    \"{m}\"")).collect();
    format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\n{}\n]\n",
        member_list.join(",\n")
    )
}

/// Minimal `[package]` Cargo.toml with optional `[dependencies]` section.
fn pkg_toml(name: &str, deps: &[(&str, &str)]) -> String {
    let mut s = format!("[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n");
    if !deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        for (dep_name, dep_path) in deps {
            s.push_str(&format!("{dep_name} = {{ path = \"{dep_path}\" }}\n"));
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Test 1 — Negative (clean workspace): kernel/foo with no Tier-2 dep,
//           crates/bar with no Tier-2→Tier-3 dep.
// ---------------------------------------------------------------------------

#[test]
fn test_clean_workspace_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["kernel/foo", "crates/bar"]),
    );
    write_file(root, "kernel/foo/Cargo.toml", &pkg_toml("foo", &[]));
    write_file(root, "kernel/foo/src/lib.rs", "");
    write_file(root, "crates/bar/Cargo.toml", &pkg_toml("bar", &[]));
    write_file(root, "crates/bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(code, 0, "clean workspace should exit 0; stdout:\n{stdout}");
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Positive (Tier-1 → Tier-2): kernel/foo depends on crates/bar.
// ---------------------------------------------------------------------------

#[test]
fn test_tier1_depends_on_tier2_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["kernel/foo", "crates/bar"]),
    );
    write_file(
        root,
        "kernel/foo/Cargo.toml",
        &pkg_toml("foo", &[("bar", "../../crates/bar")]),
    );
    write_file(root, "kernel/foo/src/lib.rs", "");
    write_file(root, "crates/bar/Cargo.toml", &pkg_toml("bar", &[]));
    write_file(root, "crates/bar/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "Tier-1→Tier-2 dep should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rule 1"),
        "expected rule 1 message in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Positive (cad-core stands alone): crates/cad-core depends on
//           crates/material-graph.
// ---------------------------------------------------------------------------

#[test]
fn test_cad_core_depends_on_tier2_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["crates/cad-core", "crates/material-graph"]),
    );
    write_file(
        root,
        "crates/cad-core/Cargo.toml",
        &pkg_toml("cad-core", &[("material-graph", "../material-graph")]),
    );
    write_file(root, "crates/cad-core/src/lib.rs", "");
    write_file(
        root,
        "crates/material-graph/Cargo.toml",
        &pkg_toml("material-graph", &[]),
    );
    write_file(root, "crates/material-graph/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "cad-core dep on Tier-2 should be a violation; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rule 3"),
        "expected rule 3 message in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Positive (editor-ui → physics): crates/editor-ui depends on
//           crates/physics.
// ---------------------------------------------------------------------------

#[test]
fn test_editor_ui_depends_on_physics_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["crates/editor-ui", "crates/physics"]),
    );
    write_file(
        root,
        "crates/editor-ui/Cargo.toml",
        &pkg_toml("editor-ui", &[("physics", "../physics")]),
    );
    write_file(root, "crates/editor-ui/src/lib.rs", "");
    write_file(root, "crates/physics/Cargo.toml", &pkg_toml("physics", &[]));
    write_file(root, "crates/physics/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "editor-ui→physics should be a violation; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rule 4"),
        "expected rule 4 message in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Positive (physics → script-host).
// ---------------------------------------------------------------------------

#[test]
fn test_physics_depends_on_script_host_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["crates/physics", "crates/script-host"]),
    );
    write_file(
        root,
        "crates/physics/Cargo.toml",
        &pkg_toml("physics", &[("script-host", "../script-host")]),
    );
    write_file(root, "crates/physics/src/lib.rs", "");
    write_file(
        root,
        "crates/script-host/Cargo.toml",
        &pkg_toml("script-host", &[]),
    );
    write_file(root, "crates/script-host/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "physics→script-host should be a violation; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rule 5"),
        "expected rule 5 message in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive (renderer → game-domain): crates/gfx depends on
//           crates/cad-core.
// ---------------------------------------------------------------------------

#[test]
fn test_renderer_depends_on_game_domain_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        &root_toml(&["crates/gfx", "crates/cad-core"]),
    );
    write_file(
        root,
        "crates/gfx/Cargo.toml",
        &pkg_toml("gfx", &[("cad-core", "../cad-core")]),
    );
    write_file(root, "crates/gfx/src/lib.rs", "");
    write_file(
        root,
        "crates/cad-core/Cargo.toml",
        &pkg_toml("cad-core", &[]),
    );
    write_file(root, "crates/cad-core/src/lib.rs", "");

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "renderer→game-domain should be a violation; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("rule 6"),
        "expected rule 6 message in output:\n{stdout}"
    );
}
