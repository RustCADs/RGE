//! **`SnapshotParticipate` coverage lint** — supplementary v1.0 gate scaffold
//! for PLAN.md §13.2 ("all stateful Tier-2 has `SnapshotParticipate`").
//!
//! Failure class: recoverable
//!
//! # What this lint does
//!
//! Walks the workspace and checks: for every Tier-2 crate whose name appears
//! in [`STATEFUL_TIER2_CRATES`], does its source tree contain at least one
//! `impl SnapshotParticipate` block?
//!
//! - If yes: emit one `info: <crate> impl SnapshotParticipate (PLAN §13.2)` line
//!   to stdout.
//! - If no: emit one `info: <crate> is stateful Tier-2 but does not impl
//!   SnapshotParticipate (PLAN §13.2 v1.0 gate; tracked separately)` line to
//!   stderr.
//!
//! The lint then summarizes via [`LintReport`] as a PASS regardless. It NEVER
//! pushes a [`Violation`] — see "Why warning-level only" below — so its exit
//! code is always 0 and it does not affect the aggregate `all` outcome of the
//! 9 enforcement lints.
//!
//! # Why warning-level only
//!
//! Per PLAN.md §13.2 the gate "all stateful Tier-2 has `SnapshotParticipate`"
//! is a v1.0 milestone, not a today-blocker. Today three crates carry real
//! impls (`cad-core::CadGraph` + `cad-projection::CadProjection` +
//! `physics::World`). The two remaining list entries (`particles`, `sculpt`)
//! are forward-compat placeholders for crates that do not exist in the
//! workspace yet — when they land they get checked automatically.
//!
//! Promoting this lint to error-level today would fail every CI run when a
//! placeholder Tier-2 crate (e.g. `particles` once it lands) goes through its
//! initial stub phase. The warning-level posture lets the lint surface
//! coverage tracking without blocking inter-Phase landings.
//!
//! When a future dispatch wants to flip this to error-level (e.g. once each
//! crate in the canonical list has gained an impl), change [`run`] to push a
//! [`Violation`] in place of the missing-impl `eprintln!` and rely on the
//! existing `LintReport::print` to fail the lint with exit-code 1.
//!
//! # Heuristic for "stateful Tier-2"
//!
//! Two conditions must both hold for a crate to be checked:
//!
//! 1. Tier-2 (manifest under `crates/`, not `kernel/`).
//! 2. Crate name (without the `rge-` prefix) appears in
//!    [`STATEFUL_TIER2_CRATES`]. The list is forward-looking — `particles`
//!    and `sculpt` have no crate today but will when those subsystems land,
//!    and the lint will fire on them automatically.
//!
//! See [`STATEFUL_TIER2_CRATES`] for the per-crate inclusion / exclusion
//! rationale (audited 2026-05-09 — 4 crates removed from the list because
//! they DON'T own state that round-trips through PIE; cited PLAN / §18
//! sections per removal).
//!
//! Detection of `impl SnapshotParticipate` uses a string search across every
//! `.rs` file in the crate's `src/` tree. A `syn` AST walk would be more
//! precise but is unnecessary: the trait name is unique to this codebase
//! (no conflicting names elsewhere) and the string match is robust to the
//! three existing impl forms — `impl SnapshotParticipate for CadGraph`,
//! `impl SnapshotParticipate for CadProjection`, and `impl SnapshotParticipate
//! for World` (physics).
//!
//! # See also
//!
//! - [`crate::failure_class`] for the per-crate-walk pattern.
//! - [`crate::kernel_isolation`] for the warn-but-exit-0 pattern.
//! - `kernel/ecs/src/participate.rs` — the trait definition.
//! - `crates/cad-core/src/checkpoints/participate.rs` — first impl.
//! - `crates/cad-projection/src/lib.rs` — second impl.
//! - `crates/physics/src/participate.rs` — third impl.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::common::{cargo_metadata, classify, relativize, workspace_members, LintReport, Tier};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Lint identifier used in [`LintReport`] and in the exemptions registry.
const LINT_NAME: &str = "snapshot-participate";

/// String literal searched for inside each crate's source tree.
///
/// A whitespace-tolerant match (e.g. tabs vs spaces) is unnecessary because
/// the workspace's rustfmt configuration normalizes the spacing of impl headers
/// and the trait name is unique to this codebase.
const IMPL_NEEDLE: &str = "impl SnapshotParticipate";

/// Bare crate names (no `rge-` prefix) that own state participating in PIE
/// snapshots. Maintained explicitly because we want a prescriptive list, not a
/// heuristic — the lint's purpose is to enforce a closed set.
///
/// When a new stateful Tier-2 subsystem lands, append it here. When a former
/// stateful crate becomes stateless, remove it.
///
/// # Inclusion criterion
///
/// "Stateful in the PIE sense" means the crate owns state that should
/// round-trip with `PieSnapshot::capture` / `PieSnapshot::restore`. The 3
/// existing impls (cad-core::CadGraph + cad-projection::CadProjection +
/// physics::World) all own substantive simulation / scene state that
/// participates in save/load AND in replay. Crates that do NOT own such state
/// MUST NOT be on this list — adding a `SnapshotParticipate` impl with an
/// empty / no-op payload would dilute the substrate's meaning per the
/// CONCRETE-STATE-THAT-ROUND-TRIPS doctrine.
///
/// # As of 2026-05-09 (post-discriminate-vs-implement audit):
///
/// - `cad-core` — has `impl SnapshotParticipate for CadGraph`.
/// - `cad-projection` — has `impl SnapshotParticipate for CadProjection`.
/// - `physics` — has `impl SnapshotParticipate for World`.
/// - `particles`, `sculpt` — listed for forward-compatibility; crates do not
///   exist yet but will be checked automatically when they land.
///
/// # Removed crates and rationale (post-2026-05-09 audit closure):
///
/// The 4 crates below were removed from the list after a per-crate audit
/// against the inclusion criterion above. Each removal cites the PLAN section
/// or §18 doc that frames the crate as session-scoped / coordination-not-
/// authority / reproducible-from-upstream / transient-by-class:
///
/// - `audio` (PLAN §6.13 list named audio, but RECOVERY_MODEL.md §4 / §9 +
///   EXECUTION_DOMAINS.md §4 declare its failure class `recoverable`
///   *because* audio state is transient and does NOT participate in PIE —
///   "audio is recoverable because its state is transient" / "Audio state
///   does NOT participate in PIE (it's transient: a paused-then-resumed
///   editor restarts the audio mixer from scratch)". Live `kira` backend
///   handles + per-entity `SourceState` / `ListenerState` are wall-clock-
///   coupled and rebuilt from upstream ECS scene state on next tick).
/// - `editor-actions` (PLAN §6.16.5 — "Stop restores pre-play snapshot";
///   undo stack is NOT modified during Play, so it does not need PIE round-
///   trip. PLAN §13.7 "history serialized + restored" is a project-file
///   persistence requirement, not a PIE-snapshot requirement. PIE_SNAPSHOT.md
///   §11 explicitly defers any `editor-actions.command-bus` participant to
///   "post-Phase-X command-bus stabilization" — `Action` is a `dyn Action`
///   trait object with no `Serialize` / `Deserialize`; PIE round-trip would
///   require a typetag-style registry that does not exist in v0.8).
/// - `editor-state` (PLAN §1.15 line 674 explicitly: "Editor-state persists
///   across Play/Stop (selection survives, tool persists); does NOT
///   participate in `WorldSnapshot`". EDITOR_STATE_MODEL.md §10 confirms "no
///   PIE-state at this level". Selection / Hover / ActiveTool are
///   coordination state referenced via IDs/handles — session-scoped UI
///   bookkeeping that lives across PIE Play/Stop precisely because it's not
///   part of the world snapshot).
/// - `gfx` (PLAN §1.5.2 + GFX_RENDER_TIER.md §11 — the `gfx.render-snapshot`
///   participant is "Pending Phase 6 work"; today's substrate is single-
///   threaded headless rendering with no §1.5.2 sim/render-thread split.
///   Today's gfx state is GPU resource state — wgpu device / queue /
///   pipelines / buffers — that is non-`Send`-serializable and reproducible
///   from upstream scene state on next render. The participant lands
///   alongside the future frame-graph + render-snapshot separation, not
///   today).
///
/// All 4 removals are tracked in `docs/§18/PIE_SNAPSHOT.md` §11 ("Future
/// participants") and §9 ("Current participants registry"). Re-add a crate
/// here only when its first concrete `SnapshotParticipate` impl ships — at
/// which point the crate already satisfies the inclusion criterion (state
/// that round-trips).
pub(crate) const STATEFUL_TIER2_CRATES: &[&str] = &[
    "cad-core",
    "cad-projection",
    "particles",
    "physics",
    "sculpt",
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the canonical `rge-` workspace prefix from a package name, returning
/// the bare crate name. Returns the original string when no prefix is present
/// (e.g. fixture crates that omit the prefix).
#[must_use]
fn bare_crate_name(pkg_name: &str) -> &str {
    pkg_name.strip_prefix("rge-").unwrap_or(pkg_name)
}

/// Return the directory containing the crate's source tree
/// (`<manifest_dir>/src`).
///
/// `pkg.manifest_path` is `<manifest_dir>/Cargo.toml`; we pop one segment.
#[must_use]
fn crate_src_dir(pkg: &cargo_metadata::Package) -> PathBuf {
    pkg.manifest_path
        .as_std_path()
        .parent()
        .expect("manifest_path always has a parent directory")
        .join("src")
}

/// Walk every `.rs` file under `src_dir` and return `true` as soon as one
/// contains [`IMPL_NEEDLE`]. Returns `false` if none do (or `src_dir` does
/// not exist).
fn crate_has_impl(src_dir: &Path) -> bool {
    if !src_dir.is_dir() {
        return false;
    }

    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        // `is_some_and` (stable since 1.70) is the workspace-MSRV-safe
        // analogue of `Option::is_none_or` (only stable since 1.82). Same
        // pattern is used in `common::iter_rust_files`.
        if !entry.path().extension().is_some_and(|e| e == "rs") {
            continue;
        }

        // Read; on read error, skip this file and continue. A parse error
        // here is not a lint failure — the failure_class lint will catch
        // truly missing files, and an actively malformed file would have
        // already failed `cargo build` long before this lint runs.
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if text.contains(IMPL_NEEDLE) {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Run the snapshot-participate coverage lint against the workspace at
/// `workspace_root`.
///
/// Always returns a [`LintReport`] with zero violations — this lint is
/// warning-level only and never fails the `all` aggregate. Per-crate findings
/// are emitted as `info:` lines (impl present → stdout; impl missing →
/// stderr) so the diagnostic shows up in CI logs without affecting exit code.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let report = LintReport::new(LINT_NAME);

    let meta = cargo_metadata(workspace_root)?;
    let members = workspace_members(&meta);

    let mut checked = 0usize;
    let mut have_impl = 0usize;
    let mut missing_impl = 0usize;

    // Sort by package name for deterministic output regardless of cargo's
    // ordering. Building a Vec of references is cheap.
    let mut sorted: Vec<&cargo_metadata::Package> = members.into_iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));

    for pkg in &sorted {
        // Only Tier-2.
        if classify(pkg, workspace_root) != Tier::Two {
            continue;
        }

        let bare = bare_crate_name(&pkg.name);
        if !STATEFUL_TIER2_CRATES.contains(&bare) {
            continue;
        }

        checked += 1;

        let src_dir = crate_src_dir(pkg);
        // Use the manifest path for the diagnostic location so the path
        // points at something stable (the `src/` directory may not exist
        // for every fixture crate).
        let manifest_rel = relativize(pkg.manifest_path.as_std_path(), workspace_root);

        if crate_has_impl(&src_dir) {
            have_impl += 1;
            println!(
                "info: {} impl SnapshotParticipate (PLAN §13.2; manifest {})",
                pkg.name,
                manifest_rel.display()
            );
        } else {
            missing_impl += 1;
            // Diagnostic lines go to stderr so the CI summary parsers that
            // grep stdout for `FAIL` aren't confused, while the line is
            // still visible in build logs.
            eprintln!(
                "info: crate {} is stateful Tier-2 but does not impl \
                 SnapshotParticipate (PLAN §13.2 v1.0 gate; tracked separately; \
                 manifest {})",
                pkg.name,
                manifest_rel.display()
            );
        }
    }

    // Print a one-line summary at info level so the message format mirrors
    // the other lints' final-line summary while making it obvious this
    // particular lint is warning-level. The aggregate `all` runner will
    // still call `LintReport::print` afterwards with the canonical
    // `[snapshot-participate] PASS (0 violations)` line.
    println!(
        "[{LINT_NAME}] supplementary: {checked} stateful Tier-2 crates checked; \
         {have_impl} impl SnapshotParticipate; {missing_impl} still missing \
         (warning-level — does not fail CI)"
    );

    Ok(report)
}

// ---------------------------------------------------------------------------
// Unit tests (helper functions only — integration tests live in
// `tests/snapshot_participate_test.rs`).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_crate_name_strips_rge_prefix() {
        assert_eq!(bare_crate_name("rge-physics"), "physics");
        assert_eq!(bare_crate_name("rge-cad-core"), "cad-core");
        // No prefix → unchanged.
        assert_eq!(bare_crate_name("editor-state"), "editor-state");
        // Substring match must NOT trip the strip — `prefix` is anchored.
        assert_eq!(bare_crate_name("not-rge-something"), "not-rge-something");
    }

    #[test]
    fn stateful_tier2_list_contains_known_impls() {
        // `cad-core`, `cad-projection`, and `physics` are the three crates
        // that already impl SnapshotParticipate today; all three must appear
        // in the canonical list so the lint actually covers them.
        assert!(STATEFUL_TIER2_CRATES.contains(&"cad-core"));
        assert!(STATEFUL_TIER2_CRATES.contains(&"cad-projection"));
        assert!(STATEFUL_TIER2_CRATES.contains(&"physics"));
    }

    #[test]
    fn stateful_tier2_list_does_not_contain_audited_removals() {
        // Per the post-2026-05-09 discriminate-vs-implement audit (closing
        // the H3 v1.0-gate scaffold), 4 crates were removed from the list
        // because each does NOT own state that round-trips through PIE. The
        // module-level `STATEFUL_TIER2_CRATES` doc-comment cites the PLAN /
        // §18 sections that frame each removal. Pin the removals here so a
        // future re-add is a deliberate edit (not an accidental alphabetised
        // restoration).
        assert!(!STATEFUL_TIER2_CRATES.contains(&"audio"));
        assert!(!STATEFUL_TIER2_CRATES.contains(&"editor-actions"));
        assert!(!STATEFUL_TIER2_CRATES.contains(&"editor-state"));
        assert!(!STATEFUL_TIER2_CRATES.contains(&"gfx"));
    }

    #[test]
    fn stateful_tier2_list_is_sorted_for_deterministic_review() {
        // Keep the list sorted alphabetically so future additions go in a
        // predictable place. Easy to enforce mechanically; cheap diff hygiene.
        let mut sorted = STATEFUL_TIER2_CRATES.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted.as_slice(),
            STATEFUL_TIER2_CRATES,
            "STATEFUL_TIER2_CRATES must be kept sorted"
        );
    }

    #[test]
    fn crate_has_impl_returns_false_for_nonexistent_dir() {
        let bogus = Path::new("definitely-does-not-exist-anywhere");
        assert!(!crate_has_impl(bogus));
    }
}
