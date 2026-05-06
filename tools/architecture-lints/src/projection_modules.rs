//! cad-projection module-split lint — PLAN.md §1.6 / §1.8.
//!
//! Inside `crates/cad-projection/src/`, the `projection_structural` module may
//! NOT import from `projection_runtime` or `projection_editor`.  The structural
//! projection feeds the others; depending on runtime/editor concerns would be
//! circular pollution and defeats the whole point of the v0.7 split.
//!
//! ## Scope
//!
//! Only files under `crates/cad-projection/src/` are examined.  If that
//! directory does not exist (the crate is a stub during early phases), the lint
//! returns an empty passing report immediately.
//!
//! ## What counts as `projection_structural`
//!
//! A file belongs to `projection_structural` when:
//! - Its path contains the directory component `projection_structural/`, **or**
//! - It is named `projection_structural.rs` directly under
//!   `crates/cad-projection/src/`.
//!
//! ## Flagged import patterns
//!
//! The following are violations:
//! - `use crate::projection_runtime::…`
//! - `use crate::projection_editor::…`
//! - `use super::projection_runtime::…`
//! - `use super::projection_editor::…`
//! - `use self::projection_runtime::…`
//! - `use self::projection_editor::…`
//! - `use cad_projection::projection_runtime::…`
//! - `use cad_projection::projection_editor::…`
//! - `mod projection_runtime;` (re-declaration confusion)
//! - `mod projection_editor;`  (re-declaration confusion)

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use syn::visit::Visit;
use syn::{ItemMod, ItemUse, UseTree};

use crate::common::{iter_rust_files, relativize, LintReport, Violation};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Forbidden module names that `projection_structural` must not reach into.
const FORBIDDEN_MODULES: &[&str] = &["projection_runtime", "projection_editor"];

/// Leading path segments that act as "transparent" namespace prefixes: after
/// stripping one of these we look at the *next* segment for the module name.
const TRANSPARENT_PREFIXES: &[&str] = &["crate", "super", "self", "cad_projection"];

/// Directory name of the cad-projection source tree (relative to workspace).
const CAD_PROJECTION_SRC: &[&str] = &["crates", "cad-projection", "src"];

/// Directory component that identifies the structural sub-module.
const STRUCTURAL_DIR: &str = "projection_structural";

/// Single-file variant of the structural module.
const STRUCTURAL_FILE: &str = "projection_structural.rs";

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Return the `crates/cad-projection/src` directory, or `None` if it does not
/// exist on disk.  The directory may not exist while the crate is a stub.
fn cad_projection_src(workspace_root: &Path) -> Option<PathBuf> {
    let dir = CAD_PROJECTION_SRC
        .iter()
        .fold(workspace_root.to_path_buf(), |acc, c| acc.join(c));
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// Return `true` when `path` belongs to the `projection_structural` module.
///
/// Two membership criteria (see module-level docs):
/// 1. The path has a directory component named `projection_structural`.
/// 2. The file is named `projection_structural.rs` (single-file module form).
fn is_structural(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_string_lossy().as_ref(),
            STRUCTURAL_DIR | STRUCTURAL_FILE
        )
    })
}

// ---------------------------------------------------------------------------
// Line-number helper
// ---------------------------------------------------------------------------

/// Return the 1-based line number of the first occurrence of `needle` in `src`,
/// or `None` if not found.
///
/// Used because `proc_macro2::Span::start()` requires the `span-locations`
/// feature, which is not enabled in this workspace's `proc-macro2` dependency.
fn find_line(src: &str, needle: &str) -> Option<usize> {
    src.find(needle)
        .map(|offset| src[..offset].chars().filter(|&c| c == '\n').count() + 1)
}

// ---------------------------------------------------------------------------
// Visitor
// ---------------------------------------------------------------------------

/// `syn` visitor that collects `projection_structural` purity violations from a
/// single parsed `.rs` file.
struct StructuralPurityVisitor<'a> {
    /// Workspace-relative path (for error messages).
    file: &'a Path,
    /// Raw source text — used to recover approximate line numbers.
    src: &'a str,
    /// Accumulated violations.
    pub(crate) violations: Vec<Violation>,
}

impl<'a> StructuralPurityVisitor<'a> {
    fn new(file: &'a Path, src: &'a str) -> Self {
        Self {
            file,
            src,
            violations: Vec::new(),
        }
    }

    /// Record one violation for the given path representation.
    fn record(&mut self, path_repr: &str) {
        let needle = path_repr.trim_end_matches("::…");
        let line = find_line(self.src, needle);
        self.violations.push(Violation {
            file: self.file.to_path_buf(),
            line,
            message: format!(
                "projection_structural cannot import projection_runtime/projection_editor \
                 (PLAN §1.6) — found `{path_repr}`"
            ),
        });
    }
}

impl<'ast> Visit<'ast> for StructuralPurityVisitor<'_> {
    /// Check every `use` statement, including those inside inline `mod` blocks
    /// (the default `visit_item_use` recursion handles nested modules).
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        // Handle top-level group braces: `use crate::{projection_runtime, …}`.
        // We do this by manually decomposing the tree one level at a time.
        Self::check_use_tree_recursive(&node.tree, &mut |path_repr| {
            let needle = path_repr.trim_end_matches("::…");
            let line = find_line(self.src, needle);
            self.violations.push(Violation {
                file: self.file.to_path_buf(),
                line,
                message: format!(
                    "projection_structural cannot import \
                     projection_runtime/projection_editor (PLAN §1.6) — found `{path_repr}`"
                ),
            });
        });

        // Let syn recurse into any nested `mod` blocks automatically.
        syn::visit::visit_item_use(self, node);
    }

    /// Flag `mod projection_runtime;` / `mod projection_editor;` inside
    /// `projection_structural` — re-declaring the module is likely a confusion.
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let name = node.ident.to_string();
        if FORBIDDEN_MODULES.contains(&name.as_str()) {
            self.record(&format!("mod {name}"));
        }
        // Recurse into inline module bodies.
        syn::visit::visit_item_mod(self, node);
    }
}

impl StructuralPurityVisitor<'_> {
    /// Walk a [`UseTree`] recursively, calling `emit` for every forbidden leaf.
    ///
    /// This handles brace groups at any depth, e.g.
    /// `use crate::{projection_runtime::Foo, projection_geometry::Bar}`.
    fn check_use_tree_recursive<F>(tree: &UseTree, emit: &mut F)
    where
        F: FnMut(String),
    {
        match tree {
            UseTree::Path(p) => {
                let seg = p.ident.to_string();

                if FORBIDDEN_MODULES.contains(&seg.as_str()) {
                    emit(format!("{seg}::…"));
                    return; // Don't recurse further into forbidden subtree.
                }

                if TRANSPARENT_PREFIXES.contains(&seg.as_str()) {
                    // Check if the very next segment is forbidden.
                    match p.tree.as_ref() {
                        UseTree::Path(inner) => {
                            let inner_seg = inner.ident.to_string();
                            if FORBIDDEN_MODULES.contains(&inner_seg.as_str()) {
                                emit(format!("{seg}::{inner_seg}::…"));
                                return;
                            }
                            // Keep descending.
                            Self::check_use_tree_recursive(p.tree.as_ref(), emit);
                        }
                        UseTree::Name(n) => {
                            let inner_seg = n.ident.to_string();
                            if FORBIDDEN_MODULES.contains(&inner_seg.as_str()) {
                                emit(format!("{seg}::{inner_seg}"));
                            }
                        }
                        UseTree::Rename(r) => {
                            let inner_seg = r.ident.to_string();
                            if FORBIDDEN_MODULES.contains(&inner_seg.as_str()) {
                                emit(format!("{seg}::{inner_seg}"));
                            }
                        }
                        UseTree::Group(g) => {
                            for item in &g.items {
                                Self::check_use_tree_recursive(item, emit);
                            }
                        }
                        UseTree::Glob(_) => {}
                    }
                } else {
                    // Non-transparent, non-forbidden segment — recurse for groups.
                    Self::check_use_tree_recursive(p.tree.as_ref(), emit);
                }
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    Self::check_use_tree_recursive(item, emit);
                }
            }
            UseTree::Name(_) | UseTree::Rename(_) | UseTree::Glob(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Run the cad-projection module-split lint against the workspace at
/// `workspace_root`.
///
/// Only files under `crates/cad-projection/src/` are examined.  If that
/// directory does not exist (the crate is a Phase-4 stub), the lint returns an
/// empty passing report immediately — it is not an error.
///
/// Returns a [`LintReport`] with one [`Violation`] per forbidden import found
/// inside `projection_structural`.
// The `Result` wrapper is required by the dispatch pattern in `main.rs` (all
// lint `run` fns share the same `-> Result<LintReport>` signature).
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("projection-modules");

    let Some(src_dir) = cad_projection_src(workspace_root) else {
        // cad-projection/src does not yet exist — graceful no-op.
        return Ok(report);
    };

    for path in iter_rust_files(&[src_dir]) {
        // Only check files that belong to projection_structural.
        if !is_structural(&path) {
            continue;
        }

        let txt = match std::fs::read_to_string(&path).with_context(|| path.display().to_string()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "projection-modules lint: could not read {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let syntax = match syn::parse_file(&txt) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "projection-modules lint: could not parse {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let rel = relativize(&path, workspace_root);
        let mut visitor = StructuralPurityVisitor::new(&rel, &txt);
        visitor.visit_file(&syntax);

        for v in visitor.violations {
            report.push(v);
        }
    }

    Ok(report)
}
