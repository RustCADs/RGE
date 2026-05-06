//! Integration tests for the `projection-modules` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `projection-modules` subcommand.
//!
//! Exit-code semantics: 0 = pass (no violations), 1 = violations found, 2 = tool error.
//!
//! Fixture layout inside every temp dir:
//! ```text
//! Cargo.toml                          ← [workspace] root
//! crates/cad-projection/src/…         ← files under test
//! ```

use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled binary — Cargo injects this env var for integration tests.
fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_rge-tool-architecture-lints"))
}

/// Write `content` to `base/rel_path`, creating all intermediate directories.
fn write_file(base: &Path, rel_path: &str, content: &str) {
    let abs = base.join(rel_path);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create_dir_all");
    }
    fs::write(&abs, content).unwrap_or_else(|e| panic!("write {}: {e}", abs.display()));
}

/// Minimal workspace `Cargo.toml` so that `workspace_root()` can locate the root.
fn workspace_toml() -> &'static str {
    "[workspace]\nmembers = []\n"
}

/// Invoke the binary against `workspace_dir` with the `projection-modules` subcommand.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("projection-modules")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Test 1 — Negative: no projection_structural module at all → exit 0 (PASS).
//
// When crates/cad-projection/src/ exists but contains only lib.rs with no
// projection_structural sub-tree, the lint has nothing to check and passes.
// ---------------------------------------------------------------------------

#[test]
fn test_no_projection_structural_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/lib.rs",
        "//! cad-projection stub.\n",
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "no projection_structural → should exit 0;\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("PASS"), "expected PASS;\nstdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 2 — Negative: clean projection_structural (geometry import) → exit 0.
//
// projection_structural/mod.rs may freely import projection_geometry —
// only runtime and editor are forbidden.
// ---------------------------------------------------------------------------

#[test]
fn test_clean_structural_geometry_import_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_structural/mod.rs",
        r#"//! Clean structural module — geometry import is allowed.
use crate::projection_geometry::Mesh;

pub struct StructuralNode {
    pub mesh: Mesh,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "geometry import in projection_structural should pass (exit 0);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("PASS"), "expected PASS;\nstdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 3 — Positive: structural imports projection_runtime → exit 1, 1 violation.
// ---------------------------------------------------------------------------

#[test]
fn test_structural_imports_runtime_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_structural/mod.rs",
        r#"//! Structural module that wrongly pulls in runtime.
use crate::projection_runtime::CollisionProxy;

pub struct Bad {
    pub proxy: CollisionProxy,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "projection_runtime import should be a violation (exit 1);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL;\nstdout:\n{stdout}");
    assert!(
        stdout.contains("projection_runtime"),
        "expected 'projection_runtime' in output;\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.6"),
        "expected 'PLAN §1.6' in message;\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Positive: structural sub-file imports projection_editor → exit 1.
//
// projection_structural/picker.rs is also inside the structural module.
// ---------------------------------------------------------------------------

#[test]
fn test_structural_subfile_imports_editor_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_structural/picker.rs",
        r#"//! Picker sub-module that wrongly pulls in the editor layer.
use crate::projection_editor::Gizmo;

pub struct PickerHandle {
    pub gizmo: Gizmo,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "projection_editor import in structural sub-file should be a violation (exit 1);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL;\nstdout:\n{stdout}");
    assert!(
        stdout.contains("projection_editor"),
        "expected 'projection_editor' in output;\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.6"),
        "expected 'PLAN §1.6' in message;\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Positive: single-file structural (`projection_structural.rs`) with
// `super::projection_runtime` → exit 1, 1 violation.
//
// `super::` from `src/projection_structural.rs` resolves to `src/` level, so
// `super::projection_runtime` would reach the runtime module — forbidden.
// The lint flags this conservatively regardless of nesting depth.
// ---------------------------------------------------------------------------

#[test]
fn test_single_file_structural_super_runtime_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_structural.rs",
        r#"//! Single-file form of projection_structural.
use super::projection_runtime::Foo;

pub struct Wrapper(Foo);
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "`super::projection_runtime` in projection_structural.rs should be a violation (exit 1);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL;\nstdout:\n{stdout}");
    assert!(
        stdout.contains("projection_runtime"),
        "expected 'projection_runtime' in output;\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.6"),
        "expected 'PLAN §1.6' in message;\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Negative: a *different* module (projection_geometry) importing
// projection_runtime is perfectly fine — only structural is restricted.
// ---------------------------------------------------------------------------

#[test]
fn test_other_module_imports_runtime_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_geometry/mod.rs",
        r#"//! Geometry module — may freely import runtime.
use crate::projection_runtime::Render;

pub struct GeomNode {
    pub render: Render,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "projection_geometry importing projection_runtime is allowed (exit 0);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("PASS"), "expected PASS;\nstdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Test 7 — Positive: `mod projection_runtime;` inside structural → exit 1.
//
// Re-declaring the forbidden module inside projection_structural is also
// flagged (likely a confusion, and violates the purity rule).
// ---------------------------------------------------------------------------

#[test]
fn test_structural_redeclares_runtime_mod_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-projection/src/projection_structural/mod.rs",
        r#"//! Structural module that re-declares runtime — forbidden.
mod projection_runtime;
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "`mod projection_runtime` inside structural should be a violation (exit 1);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("FAIL"), "expected FAIL;\nstdout:\n{stdout}");
    assert!(
        stdout.contains("projection_runtime"),
        "expected 'projection_runtime' in output;\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 8 — Negative: src/ does not exist at all → graceful no-op, exit 0.
//
// When the entire cad-projection/src directory is absent, the lint must not
// error — it returns an empty passing report (Phase-4 stub case).
// ---------------------------------------------------------------------------

#[test]
fn test_missing_cad_projection_src_is_noop() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    // Only a workspace Cargo.toml, no crates/ tree at all.
    write_file(root, "Cargo.toml", workspace_toml());

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "missing cad-projection/src should be a no-op (exit 0);\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("PASS"), "expected PASS;\nstdout:\n{stdout}");
}
