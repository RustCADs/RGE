//! Forbidden-dependency DAG lint. See PLAN.md §1.8.
//!
//! Enforces six dependency rules using the cargo metadata dep graph (direct
//! workspace-internal deps only; external registry deps are ignored):
//!
//! 1. Tier 1 (`kernel/*`) cannot depend on Tier 2 (`crates/*`).
//! 2. Tier 2 cannot depend on Tier 3 (no Tier-3 crates today; encoded defensively).
//! 3. `cad-core` stands alone — cannot depend on any other Tier-2 crate.
//! 4. `editor-ui` cannot depend on `physics`, `audio`, or `input` directly.
//! 5. `physics` cannot depend on `script-host`.
//! 6. Renderer crates (`gfx`, `gfx-ir`, `brep-render`) cannot depend on
//!    game-domain crates.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use cargo_metadata::Package;

use crate::common::{
    cargo_metadata, classify, relativize, workspace_members, LintReport, Tier, Violation,
};

// ---------------------------------------------------------------------------
// Constants — package name sets
// ---------------------------------------------------------------------------

/// Renderer crate package names that must not touch game-domain crates.
const RENDERER_CRATES: &[&str] = &["gfx", "gfx-ir", "brep-render"];

/// Package names that are considered "game-domain" for rule 6.
///
/// Renderer crates may only depend on Tier-1 kernel crates plus
/// `math`, `errors`, `resources`, and `macros-reflect`.
const GAME_DOMAIN_PREFIXES: &[&str] = &[
    "components-",
    "cad-",
    "anim-",
    "material-",
    "script-",
    "editor-",
    "io-",
];

const GAME_DOMAIN_EXACT: &[&str] = &["physics", "audio", "input", "asset-store", "pak-format"];

/// Returns `true` when `name` is a game-domain crate per rule 6.
#[must_use]
fn is_game_domain(name: &str) -> bool {
    if GAME_DOMAIN_EXACT.contains(&name) {
        return true;
    }
    GAME_DOMAIN_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect the set of workspace package names from `meta` for fast lookup.
#[must_use]
fn workspace_pkg_names(pkgs: &[&Package]) -> HashSet<String> {
    pkgs.iter().map(|p| p.name.clone()).collect()
}

/// Return the direct workspace dependencies of `pkg` as a `Vec` of package
/// names (only deps whose resolved `id` appears in `workspace_ids`).
#[must_use]
fn direct_workspace_deps<'a>(pkg: &'a Package, workspace_names: &HashSet<String>) -> Vec<&'a str> {
    pkg.dependencies
        .iter()
        .filter(|d| workspace_names.contains(&d.name))
        .map(|d| d.name.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the forbidden-dependency DAG lint against the workspace at `workspace_root`.
///
/// Returns a [`LintReport`] whose violations list is empty on a clean workspace.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("forbidden-dep");

    let meta = cargo_metadata(workspace_root)?;
    let members = workspace_members(&meta);
    let ws_names = workspace_pkg_names(&members);

    for pkg in &members {
        let tier = classify(pkg, workspace_root);
        let manifest_rel = relativize(pkg.manifest_path.as_std_path(), workspace_root);
        let deps = direct_workspace_deps(pkg, &ws_names);

        for dep_name in &deps {
            let dep_pkg = members.iter().find(|p| p.name == *dep_name);
            let dep_tier = dep_pkg.map_or(Tier::Other, |d| classify(d, workspace_root));

            // Rule 1: Tier 1 cannot depend on Tier 2.
            if tier == Tier::One && dep_tier == Tier::Two {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: format!(
                        "rule 1 (Tier 1 cannot depend on Tier 2) — `{}` depends on `{}`",
                        pkg.name, dep_name
                    ),
                });
            }

            // Rule 2: Tier 2 cannot depend on Tier 3 (defensive; no Tier-3 today).
            if tier == Tier::Two && dep_tier == Tier::Three {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: format!(
                        "rule 2 (Tier 2 cannot depend on Tier 3) — `{}` depends on `{}`",
                        pkg.name, dep_name
                    ),
                });
            }

            // Rule 3: `cad-core` stands alone — no Tier-2 deps allowed.
            if pkg.name == "cad-core" && dep_tier == Tier::Two {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: format!("rule 3 (cad-core stands alone) — depends on `{dep_name}`"),
                });
            }

            // Rule 4: `editor-ui` cannot depend on `physics`, `audio`, or `input`.
            if pkg.name == "editor-ui" && matches!(*dep_name, "physics" | "audio" | "input") {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: format!(
                        "rule 4 (editor-ui cannot depend on physics/audio/input) — depends on \
                         `{dep_name}`"
                    ),
                });
            }

            // Rule 5: `physics` cannot depend on `script-host`.
            if pkg.name == "physics" && *dep_name == "script-host" {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: "rule 5 (physics cannot depend on script-host) — depends on \
                               `script-host`"
                        .to_owned(),
                });
            }

            // Rule 6: Renderer crates cannot depend on game-domain crates.
            if RENDERER_CRATES.contains(&pkg.name.as_str()) && is_game_domain(dep_name) {
                report.push(Violation {
                    file: manifest_rel.clone(),
                    line: None,
                    message: format!(
                        "rule 6 (renderer cannot depend on game-domain crates) — `{}` depends on \
                         `{dep_name}`",
                        pkg.name
                    ),
                });
            }
        }
    }

    Ok(report)
}
