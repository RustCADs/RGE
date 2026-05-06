//! Graph-foundation usage lint — PLAN.md §1.14.
//!
//! No crate outside `kernel/graph-foundation/` may define its own `NodeId`,
//! `EdgeId`, or `StableHash` types or traits. All consumers must import the
//! substrate primitives from `kernel/graph-foundation`.
//!
//! The lint walks every `.rs` file in the workspace (excluding
//! `kernel/graph-foundation/` itself) and uses `syn` to look for top-level or
//! nested item definitions whose identifier matches the forbidden set.

use std::path::Path;

use anyhow::{Context, Result};
use syn::visit::Visit;
use syn::{ItemEnum, ItemStruct, ItemTrait, ItemType};

use crate::common::{iter_rust_files, relativize, source_roots, Exemptions, LintReport, Violation};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Identifiers that may only be defined inside `kernel/graph-foundation`.
const FORBIDDEN_NAMES: &[&str] = &["NodeId", "EdgeId", "StableHash"];

/// Path component that identifies the graph-foundation crate directory.
/// Any `.rs` file whose absolute path contains this component is skipped.
const GRAPH_FOUNDATION_SEGMENT: &str = "graph-foundation";

// ---------------------------------------------------------------------------
// Line-number helper
// ---------------------------------------------------------------------------

/// Return the 1-based line number of the first occurrence of `needle` in `src`,
/// or `None` if not found. Used because `proc_macro2::Span::start()` requires
/// the `span-locations` feature which is not activated in this workspace's
/// `proc-macro2` dependency.
fn find_line(src: &str, needle: &str) -> Option<usize> {
    src.find(needle).map(|byte_offset| {
        // Count newlines before the byte offset to get the 1-based line number.
        src[..byte_offset].chars().filter(|&c| c == '\n').count() + 1
    })
}

// ---------------------------------------------------------------------------
// Visitor
// ---------------------------------------------------------------------------

/// `syn` visitor that collects violations from a single parsed file.
struct ForbiddenDefVisitor<'a> {
    /// Workspace-relative path of the file being visited (for error messages).
    file: &'a Path,
    /// Raw source text — used to recover approximate line numbers.
    src: &'a str,
    /// Accumulated violations found in this file.
    violations: Vec<Violation>,
}

impl<'a> ForbiddenDefVisitor<'a> {
    fn new(file: &'a Path, src: &'a str) -> Self {
        Self {
            file,
            src,
            violations: Vec::new(),
        }
    }

    /// Record a violation for a forbidden definition.
    ///
    /// `name` is the exact identifier string; the line is located by scanning
    /// the source text for the identifier.
    fn record(&mut self, name: &str) {
        let line = find_line(self.src, name);
        self.violations.push(Violation {
            file: self.file.to_path_buf(),
            line,
            message: format!(
                "forbidden type definition `{name}` outside kernel/graph-foundation \
                 (PLAN §1.14 — must use substrate primitives)"
            ),
        });
    }

    /// Check whether `ident` is in the forbidden set.
    fn is_forbidden(ident: &syn::Ident) -> bool {
        FORBIDDEN_NAMES.contains(&ident.to_string().as_str())
    }
}

impl<'ast> Visit<'ast> for ForbiddenDefVisitor<'_> {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if Self::is_forbidden(&node.ident) {
            let name = node.ident.to_string();
            self.record(&name);
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if Self::is_forbidden(&node.ident) {
            let name = node.ident.to_string();
            self.record(&name);
        }
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        if Self::is_forbidden(&node.ident) {
            let name = node.ident.to_string();
            self.record(&name);
        }
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        if Self::is_forbidden(&node.ident) {
            let name = node.ident.to_string();
            self.record(&name);
        }
        syn::visit::visit_item_trait(self, node);
    }
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Run the graph-foundation usage lint against the workspace at `workspace_root`.
///
/// Returns a [`LintReport`] whose violations list is empty when no crate outside
/// `kernel/graph-foundation/` defines `NodeId`, `EdgeId`, or `StableHash`.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("graph-foundation");
    let exemptions = Exemptions::load(workspace_root)?;

    let roots = source_roots(workspace_root);
    for path in iter_rust_files(&roots) {
        // Skip files that live inside kernel/graph-foundation/ — those are the
        // authoritative definitions and are explicitly allowed.
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == GRAPH_FOUNDATION_SEGMENT)
        {
            continue;
        }

        let rel = relativize(&path, workspace_root);

        // Skip files explicitly exempted in `tools/architecture-lints/exemptions.toml`.
        if exemptions.is_exempt(report.lint, &rel) {
            continue;
        }

        let txt = match std::fs::read_to_string(&path).with_context(|| path.display().to_string()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!(
                    "graph-foundation lint: could not read {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let syntax = match syn::parse_file(&txt) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "graph-foundation lint: could not parse {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let mut visitor = ForbiddenDefVisitor::new(&rel, &txt);
        visitor.visit_file(&syntax);

        for v in visitor.violations {
            report.push(v);
        }
    }

    Ok(report)
}
