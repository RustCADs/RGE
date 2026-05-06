//! Integration tests for the `graph-foundation` lint.
//!
//! Each test builds a minimal synthetic workspace in a [`tempfile::TempDir`],
//! then invokes the compiled binary with the `graph-foundation` subcommand.
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

/// Minimal workspace `Cargo.toml` so that `workspace_root()` can locate the root.
fn workspace_toml() -> &'static str {
    "[workspace]\nmembers = []\n"
}

/// Invoke the binary from `workspace_dir` with the `graph-foundation` subcommand.
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("graph-foundation")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Test 1 — Negative (clean): use statement + field usage, no definition.
//
// A file with `use some_crate::NodeId; struct Node { id: NodeId }` is clean:
// only an import and a field type reference — no forbidden definition.
// ---------------------------------------------------------------------------

#[test]
fn test_import_and_usage_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/material-graph/src/lib.rs",
        r#"
use some_crate::NodeId;

pub struct Node {
    pub id: NodeId,
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "import + field usage of NodeId should pass (no definition); stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Negative (definition inside graph-foundation is allowed).
//
// A file at `kernel/graph-foundation/src/ids.rs` containing
// `pub struct NodeId(u64);` must NOT be flagged.
// ---------------------------------------------------------------------------

#[test]
fn test_definition_inside_graph_foundation_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "kernel/graph-foundation/src/ids.rs",
        r#"
//! Authoritative primitive identifiers for all 8 graph systems.

/// Stable node identifier.
pub struct NodeId(u64);

/// Stable edge identifier.
pub struct EdgeId(u64);

/// Substrate trait for stable hashing.
pub trait StableHash {
    fn stable_hash(&self) -> u64;
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 0,
        "definitions inside kernel/graph-foundation should not be flagged; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Positive (struct redefinition in a domain crate).
//
// `crates/material-graph/src/lib.rs` containing `pub struct NodeId(u32);`
// must produce exactly one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_struct_redef_in_domain_crate_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/material-graph/src/lib.rs",
        r#"
/// A locally-redefined node ID — forbidden outside graph-foundation.
pub struct NodeId(u32);
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "struct NodeId in domain crate should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("NodeId"),
        "expected 'NodeId' in violation output:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.14"),
        "expected 'PLAN §1.14' in violation message:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Positive (trait redefinition in a domain crate).
//
// `crates/anim-graph/src/lib.rs` containing `pub trait StableHash {}` must
// produce exactly one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_trait_redef_in_domain_crate_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/anim-graph/src/lib.rs",
        r#"
/// Local re-declaration of the substrate trait — forbidden.
pub trait StableHash {
    fn stable_hash(&self) -> u64;
}
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "trait StableHash in domain crate should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("StableHash"),
        "expected 'StableHash' in violation output:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.14"),
        "expected 'PLAN §1.14' in violation message:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Positive (type alias in a domain crate).
//
// `crates/cad-core/src/lib.rs` containing `pub type EdgeId = u64;` must
// produce exactly one violation.
// ---------------------------------------------------------------------------

#[test]
fn test_type_alias_in_domain_crate_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "Cargo.toml", workspace_toml());
    write_file(
        root,
        "crates/cad-core/src/lib.rs",
        r#"
/// A locally-aliased edge ID — forbidden outside graph-foundation.
pub type EdgeId = u64;
"#,
    );

    let (code, stdout, stderr) = run_lint(root);
    assert_eq!(
        code, 1,
        "type EdgeId alias in domain crate should be a violation (exit 1); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("EdgeId"),
        "expected 'EdgeId' in violation output:\n{stdout}"
    );
    assert!(
        stdout.contains("PLAN §1.14"),
        "expected 'PLAN §1.14' in violation message:\n{stdout}"
    );
}
