//! Integration tests for the `editor-state-ownership` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `editor-state-ownership`
//! subcommand.
//!
//! Exit-code semantics:
//! - `0` — pass (no violations)
//! - `1` — violations found
//! - `2` — tool error (e.g. workspace not found)

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

/// Invoke the binary against `workspace_dir` with the `editor-state-ownership`
/// subcommand.  Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("editor-state-ownership")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

/// Minimal workspace root `Cargo.toml` — `[workspace]` present so
/// `workspace_root()` can find it.
fn root_toml() -> &'static str {
    "[workspace]\nmembers = []\n"
}

// ---------------------------------------------------------------------------
// Test 1 — Negative (Part A): importing Selection via `use` is fine.
//
// `crates/editor-ui/src/lib.rs` contains:
//   use editor_state::Selection;
//   struct Foo { sel: Selection }
// No violation expected (import, not definition).
// ---------------------------------------------------------------------------

#[test]
fn test_part_a_import_is_fine() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-ui/src/lib.rs",
        r#"
use editor_state::Selection;
struct Foo { sel: Selection }
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "import of Selection should pass (exit 0); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Positive (Part A): struct Selection defined outside editor-state.
//
// `crates/editor-ui/src/lib.rs` contains:
//   pub struct Selection { entities: Vec<u64> }
// Exactly one violation expected.
// ---------------------------------------------------------------------------

#[test]
fn test_part_a_struct_selection_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-ui/src/lib.rs",
        r#"
pub struct Selection { entities: Vec<u64> }
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "struct Selection outside editor-state should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("Selection"),
        "expected 'Selection' in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Positive (Part A): enum ActiveTool defined outside editor-state.
//
// `crates/anim-graph-editor/src/lib.rs` contains:
//   pub enum ActiveTool { Move, Rotate }
// Exactly one violation expected.
// ---------------------------------------------------------------------------

#[test]
fn test_part_a_enum_active_tool_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/anim-graph-editor/src/lib.rs",
        r#"
pub enum ActiveTool { Move, Rotate }
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "enum ActiveTool outside editor-state should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("ActiveTool"),
        "expected 'ActiveTool' in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Negative (Part A): definition INSIDE editor-state is allowed.
//
// `crates/editor-state/src/lib.rs` contains:
//   pub struct Selection { entities: Vec<u64> }
// The substrate crate is allowed to define these types — no violation.
// ---------------------------------------------------------------------------

#[test]
fn test_part_a_definition_inside_editor_state_is_fine() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-state/src/lib.rs",
        r#"
pub struct Selection { entities: Vec<u64> }
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "defining Selection inside editor-state should pass (exit 0); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Negative (Part B): kernel import inside editor-state is fine.
//
// `crates/editor-state/src/lib.rs` contains:
//   use kernel_types::EntityId;
// Kernel crates are the approved source for IDs/handles — no violation.
// ---------------------------------------------------------------------------

#[test]
fn test_part_b_kernel_import_is_fine() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-state/src/lib.rs",
        r#"
use kernel_types::EntityId;
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "kernel import inside editor-state should pass (exit 0); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive (Part B): cad-core import inside editor-state.
//
// `crates/editor-state/src/lib.rs` contains:
//   use cad_core::BRepNode;
// Exactly one violation expected.
// ---------------------------------------------------------------------------

#[test]
fn test_part_b_cad_core_import_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-state/src/lib.rs",
        r#"
use cad_core::BRepNode;
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "cad_core import inside editor-state should be a violation (exit 1); stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("cad_core"),
        "expected 'cad_core' in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Positive (Part B): components-render import inside editor-state.
//
// `crates/editor-state/src/something.rs` contains:
//   use components_render::MeshRenderer;
// Exactly one violation expected.
// ---------------------------------------------------------------------------

#[test]
fn test_part_b_components_render_import_is_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", root_toml());
    write_file(
        root,
        "crates/editor-state/src/something.rs",
        r#"
use components_render::MeshRenderer;
"#,
    );

    let (code, stdout, _stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "components_render import inside editor-state should be a violation (exit 1); \
         stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("components_render"),
        "expected 'components_render' in output:\n{stdout}"
    );
}
