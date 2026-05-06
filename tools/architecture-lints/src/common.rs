//! Shared helpers for architecture lints.
//!
//! Each lint module is a free function `pub(crate) fn run(workspace_root: &Path) -> anyhow::Result<LintReport>`.
//! `main.rs` dispatches to the chosen lint(s) and prints the resulting reports.

// Helpers are reserved for use by lint modules; some may not be exercised yet
// while individual lints are still stubs.
#![allow(dead_code)]

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use serde::Deserialize;
use walkdir::WalkDir;

/// One concrete violation recorded by a lint.
#[derive(Debug, Clone)]
pub(crate) struct Violation {
    /// File the violation was found in (absolute or workspace-relative).
    pub(crate) file: PathBuf,
    /// 1-based line number, when known.
    pub(crate) line: Option<usize>,
    /// Human-readable message describing the rule that was broken.
    pub(crate) message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{}: {}", self.file.display(), line, self.message),
            None => write!(f, "{}: {}", self.file.display(), self.message),
        }
    }
}

/// Result bundle from running a single lint.
#[derive(Debug, Clone)]
pub(crate) struct LintReport {
    /// Stable identifier (kebab-case) — used in CLI subcommands and CI output.
    pub(crate) lint: &'static str,
    /// All violations found. Empty = pass.
    pub(crate) violations: Vec<Violation>,
}

impl LintReport {
    /// Construct an empty (passing) report.
    #[must_use]
    pub(crate) fn new(lint: &'static str) -> Self {
        Self {
            lint,
            violations: Vec::new(),
        }
    }

    /// Append a violation.
    pub(crate) fn push(&mut self, v: Violation) {
        self.violations.push(v);
    }

    /// `true` when there are zero violations.
    #[must_use]
    pub(crate) fn ok(&self) -> bool {
        self.violations.is_empty()
    }

    /// Pretty-print the report. One line per violation. Returns `true` on pass.
    pub(crate) fn print(&self) -> bool {
        if self.violations.is_empty() {
            println!("[{}] PASS (0 violations)", self.lint);
            return true;
        }
        println!(
            "[{}] FAIL ({} violations)",
            self.lint,
            self.violations.len()
        );
        for v in &self.violations {
            println!("  - {v}");
        }
        false
    }
}

/// Locate the workspace root by walking up from the current dir looking for
/// a `Cargo.toml` whose `[workspace]` table is present.
pub(crate) fn workspace_root() -> Result<PathBuf> {
    let start = std::env::current_dir().context("current_dir")?;
    let mut cur: &Path = &start;
    loop {
        let manifest = cur.join("Cargo.toml");
        if manifest.is_file() {
            let txt = std::fs::read_to_string(&manifest)
                .with_context(|| manifest.display().to_string())?;
            if txt.contains("[workspace]") {
                return Ok(cur.to_path_buf());
            }
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => anyhow::bail!("could not find workspace root from {}", start.display()),
        }
    }
}

/// Wrap `cargo metadata --no-deps` against the given workspace root.
pub(crate) fn cargo_metadata(workspace_root: &Path) -> Result<Metadata> {
    MetadataCommand::new()
        .manifest_path(workspace_root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .with_context(|| format!("cargo metadata at {}", workspace_root.display()))
}

/// Workspace-only members (filters out anything outside the workspace).
pub(crate) fn workspace_members(meta: &Metadata) -> Vec<&Package> {
    meta.workspace_packages()
}

/// Iterate every `.rs` file under any of the given roots, skipping `target/`,
/// `third_party/`, `.git/`, `examples/`, `golden-projects/`. Returns absolute
/// paths.
pub(crate) fn iter_rust_files(roots: &[PathBuf]) -> impl Iterator<Item = PathBuf> + '_ {
    roots.iter().flat_map(|root| {
        WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    "target"
                        | "third_party"
                        | ".git"
                        | "examples"
                        | "golden-projects"
                        | "node_modules"
                )
            })
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .map(|e| e.path().to_path_buf())
    })
}

/// Standard list of directory roots that contain workspace source code.
#[must_use]
pub(crate) fn source_roots(workspace_root: &Path) -> Vec<PathBuf> {
    ["kernel", "crates", "runtime", "editor", "tools"]
        .iter()
        .map(|d| workspace_root.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

/// Best-effort: turn an absolute path into a workspace-relative one for nicer
/// error messages. Falls back to the original absolute path on failure.
#[must_use]
pub(crate) fn relativize(path: &Path, workspace_root: &Path) -> PathBuf {
    path.strip_prefix(workspace_root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

/// Tier classification of a workspace crate.
///
/// Derived from manifest path:
/// - `kernel/*`              -> Tier 1
/// - `crates/*`              -> Tier 2 (privileged plugins)
/// - everything else (`runtime/*`, `editor/*`, `tools/*`) -> Tool / runtime; not tiered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    /// Kernel crate (`kernel/*`).
    One,
    /// Privileged plugin (`crates/*`).
    Two,
    /// Sandboxed plugin — not currently present as workspace crates (would be Tier 3).
    Three,
    /// Runtime target, editor app, or build tool — not part of the tier hierarchy.
    Other,
}

/// Classify a workspace package by manifest path.
#[must_use]
pub(crate) fn classify(pkg: &Package, workspace_root: &Path) -> Tier {
    let Ok(rel) = pkg.manifest_path.as_std_path().strip_prefix(workspace_root) else {
        return Tier::Other;
    };
    let mut comps = rel.components();
    match comps.next().and_then(|c| c.as_os_str().to_str()) {
        Some("kernel") => Tier::One,
        Some("crates") => Tier::Two,
        _ => Tier::Other,
    }
}

// ---------------------------------------------------------------------------
// Exemptions registry (`tools/architecture-lints/exemptions.toml`)
// ---------------------------------------------------------------------------

/// One row in the exemptions registry. See `exemptions.toml` in this crate.
#[derive(Debug, Deserialize)]
struct Exemption {
    /// Lint name, kebab-case, matching `LintReport::lint`.
    lint: String,
    /// Workspace-relative path with forward-slash separators.
    file: String,
    /// Free-text justification (recorded but not parsed).
    #[allow(dead_code)]
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ExemptionsFile {
    #[serde(default, rename = "exemption")]
    exemptions: Vec<Exemption>,
}

/// Loaded exemptions registry. Cheap to query; pass it through the lint.
#[derive(Debug, Default)]
pub(crate) struct Exemptions {
    rows: Vec<Exemption>,
}

impl Exemptions {
    /// Load `tools/architecture-lints/exemptions.toml` from the workspace.
    /// A missing file yields an empty registry (not an error).
    pub(crate) fn load(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root.join("tools/architecture-lints/exemptions.toml");
        if !path.is_file() {
            return Ok(Self::default());
        }
        let txt = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let parsed: ExemptionsFile =
            toml::from_str(&txt).with_context(|| path.display().to_string())?;
        Ok(Self {
            rows: parsed.exemptions,
        })
    }

    /// `true` when the given workspace-relative file is exempt from the
    /// given lint. The path comparison normalizes Windows backslashes to
    /// forward slashes so exemptions written in POSIX style match either
    /// host platform.
    #[must_use]
    pub(crate) fn is_exempt(&self, lint: &str, file: &Path) -> bool {
        let needle = file.to_string_lossy().replace('\\', "/");
        self.rows.iter().any(|e| e.lint == lint && e.file == needle)
    }
}
