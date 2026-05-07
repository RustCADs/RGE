//! Integration tests for the `snapshot-participate` lint (PLAN.md §13.2).
//!
//! Each test builds a minimal synthetic workspace inside a [`tempfile::TempDir`]
//! and invokes the compiled binary with the `snapshot-participate` subcommand.
//!
//! The lint is **warning-level only** — it never produces violations and
//! always exits 0. The tests verify that:
//!
//! 1. The exit code stays 0 in every scenario (no false-fails for CI).
//! 2. Crates that match `STATEFUL_TIER2_CRATES` AND have an `impl
//!    SnapshotParticipate` produce an `info: <crate> impl
//!    SnapshotParticipate` line.
//! 3. Crates that match `STATEFUL_TIER2_CRATES` AND lack the impl produce an
//!    `info: <crate> is stateful Tier-2 but does not impl
//!    SnapshotParticipate` line.
//! 4. Crates outside `STATEFUL_TIER2_CRATES` (e.g. `editor-ui`) emit no
//!    per-crate info line for that crate.
//! 5. Tier-1 crates (under `kernel/`) are silently passed over even if they
//!    happen to share a name with a stateful Tier-2 entry.
//!
//! Exit-code semantics inherited from `LintReport::print`:
//! - 0 — pass (no violations) — always for this lint today.
//! - 1 — at least one violation — never produced by this lint today.
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

/// Minimal crate `Cargo.toml`. Names mirror real-workspace usage with the
/// `rge-` prefix so [`bare_crate_name`] in the lint correctly strips it.
fn pkg_toml(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n")
}

/// Run the `snapshot-participate` subcommand from `workspace_dir`.
///
/// Returns `(exit_code, stdout, stderr)`.
fn run_lint(workspace_dir: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .arg("snapshot-participate")
        .current_dir(workspace_dir)
        .output()
        .expect("failed to execute lint binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

/// Source body for a Tier-2 crate that implements `SnapshotParticipate`.
/// The string only needs to contain the literal `impl SnapshotParticipate`
/// substring — we don't need a syntactically complete impl block because
/// the lint uses a string match, not a `syn` AST walk.
fn lib_rs_with_impl() -> &'static str {
    "//! Stateful Tier-2 with an impl.\n\
     //!\n\
     //! Failure class: snapshot-recoverable\n\
     //!\n\
     //! impl SnapshotParticipate for SomeOwnedState { /* ... */ }\n"
}

/// Source body for a Tier-2 crate that does NOT implement
/// `SnapshotParticipate`. Just module-level docs, no trait impl text.
fn lib_rs_without_impl() -> &'static str {
    "//! Stateful Tier-2 with no impl yet.\n\
     //!\n\
     //! Failure class: recoverable\n"
}

// ---------------------------------------------------------------------------
// Test 1 — Stateful Tier-2 crate WITH `impl SnapshotParticipate` → info line
//          on stdout, no missing-impl line on stderr; exit 0.
// ---------------------------------------------------------------------------

#[test]
fn test_stateful_tier2_with_impl_emits_have_impl_info() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/cad-core"]));
    write(
        root,
        "crates/cad-core/Cargo.toml",
        &pkg_toml("rge-cad-core"),
    );
    write(root, "crates/cad-core/src/lib.rs", lib_rs_with_impl());

    let (code, stdout, stderr) = run_lint(root);

    assert_eq!(
        code, 0,
        "snapshot-participate must always exit 0 (warning-level); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected lint summary `PASS` (no violations); stdout:\n{stdout}"
    );
    // Per-crate info line on stdout for the impl-present case.
    assert!(
        stdout.contains("rge-cad-core impl SnapshotParticipate"),
        "expected info line confirming impl on stdout; stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("rge-cad-core is stateful Tier-2 but does not impl"),
        "must NOT emit missing-impl warning when impl is present; stderr:\n{stderr}"
    );
    // Summary line counts.
    assert!(
        stdout.contains("1 stateful Tier-2 crates checked"),
        "summary should report 1 checked crate; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("1 impl SnapshotParticipate"),
        "summary should report 1 impl; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("0 still missing"),
        "summary should report 0 missing; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Stateful Tier-2 crate WITHOUT `impl SnapshotParticipate` → warning
//          line on stderr, no impl-present line on stdout; exit still 0.
// ---------------------------------------------------------------------------

#[test]
fn test_stateful_tier2_without_impl_emits_missing_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // The real workspace's `crates/physics/` impls SnapshotParticipate
    // (since 2026-05-09). This integration test uses a synthetic mini-
    // workspace with a fixture `src/lib.rs` whose body has no `impl
    // SnapshotParticipate` substring — that's the path the lint should
    // surface as "missing impl". The fixture name `rge-physics` is on
    // STATEFUL_TIER2_CRATES so the lint actually checks it; the content
    // mismatch is what triggers the warning we want to assert against.
    write(root, "Cargo.toml", &workspace_toml(&["crates/physics"]));
    write(root, "crates/physics/Cargo.toml", &pkg_toml("rge-physics"));
    write(root, "crates/physics/src/lib.rs", lib_rs_without_impl());

    let (code, stdout, stderr) = run_lint(root);

    assert_eq!(
        code, 0,
        "snapshot-participate must always exit 0 even when impls are missing \
         (warning-level only); stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected lint summary `PASS`; stdout:\n{stdout}"
    );
    // Missing-impl line goes to stderr.
    assert!(
        stderr.contains("rge-physics is stateful Tier-2 but does not impl SnapshotParticipate"),
        "expected missing-impl warning on stderr for rge-physics; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("PLAN §13.2"),
        "expected PLAN §13.2 reference in warning; stderr:\n{stderr}"
    );
    // Summary should count the missing impl.
    assert!(
        stdout.contains("1 still missing"),
        "summary should report 1 missing; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — A Tier-2 crate NOT in the canonical list (e.g. `editor-ui`) is
//          silently skipped: no per-crate info line, summary shows 0 checked.
// ---------------------------------------------------------------------------

#[test]
fn test_tier2_not_in_stateful_list_is_silently_skipped() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(root, "Cargo.toml", &workspace_toml(&["crates/editor-ui"]));
    write(
        root,
        "crates/editor-ui/Cargo.toml",
        &pkg_toml("rge-editor-ui"),
    );
    // editor-ui is stateless coordination, not in STATEFUL_TIER2_CRATES.
    write(root, "crates/editor-ui/src/lib.rs", lib_rs_without_impl());

    let (code, stdout, stderr) = run_lint(root);

    assert_eq!(
        code, 0,
        "non-stateful Tier-2 crates do not affect exit code; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected lint summary `PASS`; stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("rge-editor-ui impl SnapshotParticipate"),
        "must NOT emit any per-crate info for crates outside the canonical list; \
         stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("rge-editor-ui is stateful Tier-2"),
        "must NOT emit any per-crate warning for crates outside the canonical list; \
         stderr:\n{stderr}"
    );
    // Summary: 0 crates checked when no Tier-2 stateful match exists.
    assert!(
        stdout.contains("0 stateful Tier-2 crates checked"),
        "summary should report 0 checked crates; stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — A Tier-1 crate (under `kernel/`) is silently passed over even
//          when its bare name happens to be in STATEFUL_TIER2_CRATES. The
//          tier classifier filters before the name match.
// ---------------------------------------------------------------------------

#[test]
fn test_tier1_crate_is_silently_skipped() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    // Use `kernel/asset` as a representative Tier-1 crate — it's not in
    // STATEFUL_TIER2_CRATES anyway, but the substantive guarantee is that
    // tier-1 always falls through to "skip" before name matching runs.
    write(root, "Cargo.toml", &workspace_toml(&["kernel/asset"]));
    write(
        root,
        "kernel/asset/Cargo.toml",
        &pkg_toml("rge-kernel-asset"),
    );
    write(root, "kernel/asset/src/lib.rs", lib_rs_without_impl());

    let (code, stdout, stderr) = run_lint(root);

    assert_eq!(
        code, 0,
        "Tier-1 crates do not affect exit code; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected lint summary `PASS`; stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("rge-kernel-asset impl SnapshotParticipate"),
        "must NOT emit any per-crate info for kernel crates; stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("rge-kernel-asset is stateful Tier-2"),
        "must NOT emit any per-crate warning for kernel crates; stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("0 stateful Tier-2 crates checked"),
        "summary should report 0 checked crates (Tier-1 never qualifies); \
         stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Mixed workspace: one impl-present, one impl-missing, one
//          non-stateful, one Tier-1. Verifies the summary aggregates the
//          three categories correctly and only the two stateful Tier-2
//          crates produce per-crate output.
// ---------------------------------------------------------------------------

#[test]
fn test_mixed_workspace_aggregates_summary_correctly() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();

    write(
        root,
        "Cargo.toml",
        &workspace_toml(&[
            "crates/cad-core",
            "crates/particles",
            "crates/editor-ui",
            "kernel/ecs",
        ]),
    );

    // Stateful Tier-2 with impl.
    write(
        root,
        "crates/cad-core/Cargo.toml",
        &pkg_toml("rge-cad-core"),
    );
    write(root, "crates/cad-core/src/lib.rs", lib_rs_with_impl());

    // Stateful Tier-2 without impl. `particles` is on STATEFUL_TIER2_CRATES
    // as a forward-compatibility entry (the actual crate doesn't exist in
    // the workspace yet, but the lint is wired so when it lands, it gets
    // surfaced automatically). We use it as the missing-impl fixture
    // because it's the only currently-listed bare crate name that has no
    // real impl in the workspace today (cad-core / cad-projection /
    // physics all impl SnapshotParticipate; sculpt has a fixture too but
    // particles sorts earlier alphabetically).
    write(
        root,
        "crates/particles/Cargo.toml",
        &pkg_toml("rge-particles"),
    );
    write(root, "crates/particles/src/lib.rs", lib_rs_without_impl());

    // Non-stateful Tier-2 — must not appear in any output.
    write(
        root,
        "crates/editor-ui/Cargo.toml",
        &pkg_toml("rge-editor-ui"),
    );
    write(root, "crates/editor-ui/src/lib.rs", lib_rs_without_impl());

    // Tier-1 — must not appear in any output.
    write(root, "kernel/ecs/Cargo.toml", &pkg_toml("rge-kernel-ecs"));
    write(root, "kernel/ecs/src/lib.rs", lib_rs_without_impl());

    let (code, stdout, stderr) = run_lint(root);

    assert_eq!(
        code, 0,
        "mixed workspace must still exit 0; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS"),
        "expected lint summary `PASS`; stdout:\n{stdout}"
    );

    // cad-core: impl-present line on stdout.
    assert!(
        stdout.contains("rge-cad-core impl SnapshotParticipate"),
        "stdout should confirm cad-core impl; stdout:\n{stdout}"
    );
    // particles: missing-impl warning on stderr.
    assert!(
        stderr.contains("rge-particles is stateful Tier-2 but does not impl"),
        "stderr should warn about particles' missing impl; stderr:\n{stderr}"
    );
    // editor-ui and kernel-ecs: silent.
    assert!(
        !stdout.contains("rge-editor-ui"),
        "editor-ui must not appear in stdout; stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("rge-editor-ui"),
        "editor-ui must not appear in stderr; stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("rge-kernel-ecs impl"),
        "kernel-ecs must not appear in stdout's per-crate findings; stdout:\n{stdout}"
    );
    assert!(
        !stderr.contains("rge-kernel-ecs is stateful Tier-2"),
        "kernel-ecs must not appear in stderr's per-crate findings; stderr:\n{stderr}"
    );

    // Summary aggregates: 2 checked, 1 impl, 1 missing.
    assert!(
        stdout.contains("2 stateful Tier-2 crates checked"),
        "summary should report 2 checked; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("1 impl SnapshotParticipate"),
        "summary should report 1 impl; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("1 still missing"),
        "summary should report 1 missing; stdout:\n{stdout}"
    );
}
