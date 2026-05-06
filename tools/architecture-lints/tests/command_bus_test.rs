//! Integration tests for the `command-bus` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `command-bus` subcommand.
//!
//! Exit-code semantics: 0 = pass (no violations), 1 = violations found, 2 = tool error.

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

/// Minimal workspace `Cargo.toml` — just enough for `workspace_root()` to
/// locate the root by finding the `[workspace]` table.
fn workspace_toml() -> &'static str {
    "[workspace]\nmembers = []\n"
}

/// Invoke the binary from `workspace_dir` with the `command-bus` subcommand.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("command-bus")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Test 1 — Negative (read-only access is fine)
//
// `crates/foo/src/lib.rs` imports `use kernel_ecs::Query;` — Query is a
// read-only access primitive, not on the forbidden list.  Must pass.
// ---------------------------------------------------------------------------

#[test]
fn test_readonly_access_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/foo/src/lib.rs",
        r#"
use kernel_ecs::Query;

pub fn count_things(q: Query<()>) -> usize {
    q.iter().count()
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "read-only `use kernel_ecs::Query` should pass; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Negative (bare crate import)
//
// `crates/foo/src/lib.rs` contains only `use kernel_ecs;` — we cannot
// determine what symbols will be used through the bare crate path, so this
// is conservatively allowed.  Must pass.
// ---------------------------------------------------------------------------

#[test]
fn test_bare_crate_import_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/foo/src/lib.rs",
        r#"
use kernel_ecs;

pub fn something() {
    let _ = kernel_ecs::Query::<()>::default();
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "bare `use kernel_ecs;` should pass; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Negative (inside editor-actions, the bus itself)
//
// `crates/editor-actions/src/lib.rs` importing `use kernel_ecs::Commands;`
// is explicitly allowed — editor-actions *is* the Command Bus.  Must pass.
// ---------------------------------------------------------------------------

#[test]
fn test_editor_actions_import_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/editor-actions/src/lib.rs",
        r#"
use kernel_ecs::Commands;

pub struct ActionBus {
    cmds: Commands,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "`use kernel_ecs::Commands` inside editor-actions should pass; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Negative (definition inside kernel/ecs is allowed)
//
// `kernel/ecs/src/world.rs` defines `pub struct Commands;` — this is the
// authoritative definition site, not a bypass.  Must pass.
// ---------------------------------------------------------------------------

#[test]
fn test_kernel_ecs_definition_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "kernel/ecs/src/world.rs",
        r#"
//! Authoritative ECS world types.

/// Deferred-mutation command buffer.
pub struct Commands;

/// Mutable entity handle.
pub struct EntityMut;

/// Component mutation guard.
pub struct Mut<T>(T);
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "definitions in kernel/ecs should not be flagged; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Positive (Commands import outside editor-actions)
//
// `crates/material-graph/src/lib.rs` containing `use kernel_ecs::Commands;`
// is a command-bus bypass.  Must produce exactly one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_commands_import_in_domain_crate_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/material-graph/src/lib.rs",
        r#"
use kernel_ecs::Commands;

pub fn mutate(mut cmds: Commands) {
    cmds.spawn(());
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "`use kernel_ecs::Commands` in a domain crate should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Commands"),
        "expected 'Commands' in violation output:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §6.16"),
        "expected 'PLAN §6.16' in violation message:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL (1 violations)"),
        "expected exactly 1 violation:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Positive (EntityMut import outside editor-actions)
//
// `crates/anim-graph/src/lib.rs` containing `use kernel_ecs::EntityMut;`
// is a command-bus bypass.  Must produce exactly one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_entity_mut_import_in_domain_crate_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/anim-graph/src/lib.rs",
        r#"
use kernel_ecs::EntityMut;

pub fn patch(mut entity: EntityMut) {
    let _ = entity;
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "`use kernel_ecs::EntityMut` in a domain crate should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("EntityMut"),
        "expected 'EntityMut' in violation output:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §6.16"),
        "expected 'PLAN §6.16' in violation message:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL (1 violations)"),
        "expected exactly 1 violation:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Positive (group import — partial flag)
//
// `crates/cad-core/src/lib.rs` containing `use kernel_ecs::{Query, Commands};`
// must produce **exactly one** violation: `Commands` is forbidden, `Query` is
// not.  This test verifies that `UseTree::Group` flattening works correctly.
// ---------------------------------------------------------------------------

#[test]
fn test_group_import_flags_only_forbidden_symbol() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-core/src/lib.rs",
        r#"
use kernel_ecs::{Query, Commands};

pub fn bad_actor(q: Query<()>, mut c: Commands) {
    let _ = (q, c);
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "group import with one forbidden symbol should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Commands"),
        "expected 'Commands' flagged in output:\n{stdout}"
    );
    assert!(
        !stdout.contains("kernel_ecs::Query"),
        "Query should NOT appear as a violation:\n{stdout}"
    );
    assert!(
        stdout.contains("FAIL (1 violations)"),
        "expected exactly 1 violation (Query is OK, Commands is not):\n{stdout}"
    );
}
