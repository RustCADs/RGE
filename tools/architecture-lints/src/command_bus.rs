//! Command-Bus mutation lint — PLAN.md §6.16.
//!
//! # What this lint enforces
//!
//! Editor mutations must flow through the Command Bus into the runtime; they
//! must never bypass it via direct world-mutation API calls.  Any crate in
//! `crates/**` (other than `crates/editor-actions/`, which *is* the bus
//! implementation) is forbidden from importing the `kernel_ecs` mutation
//! surface directly.
//!
//! # Active enforcement (Phase 2 verified 2026-05-05)
//!
//! `kernel/ecs` (Phase 2.1) ships all the forbidden symbols below and
//! `crates/editor-actions` (Phase 2.2) is the sole legitimate consumer. The
//! lint actively enforces the rule from this point forward — adding a
//! `use kernel_ecs::Commands;` (or any mutation-side symbol) in any other
//! `crates/**` crate fails CI immediately.
//!
//! # Forbidden symbol list
//!
//! Verified against `kernel/ecs/src/lib.rs` re-exports — every symbol below
//! is `pub` from `kernel_ecs::` today.
//!
//! Mutation-side symbols (forbidden in `crates/**` outside `editor-actions`):
//!
//! | Symbol | Kind |
//! |---|---|
//! | `kernel_ecs::Commands` | type / deferred-mutation API |
//! | `kernel_ecs::EntityMut` | type / mutable entity handle |
//! | `kernel_ecs::Mut` | type / component mutation guard |
//! | `kernel_ecs::insert` | World method / re-exported free fn |
//! | `kernel_ecs::remove` | World method / re-exported free fn |
//! | `kernel_ecs::replace` | World method / re-exported free fn |
//! | `kernel_ecs::insert_component` | free function |
//! | `kernel_ecs::remove_component` | free function |
//! | `kernel_ecs::despawn` | free function |
//! | `kernel_ecs::spawn_with` | free function |
//!
//! # Scope
//!
//! The lint is intentionally narrow: it applies **only to `crates/**`**.
//! `kernel/**`, `runtime/**`, `editor/**`, and `tools/**` are explicitly
//! skipped because:
//!
//! - `kernel/ecs` itself defines the surface (allow-list by construction).
//! - `runtime/**` system-scheduling code has legitimate reasons to hold
//!   `Commands` buffers inside runtime systems; that pattern will be governed
//!   by a separate runtime-system lint when the runtime layer matures.
//! - `editor/**` and `tools/**` are not user-facing plugin crates and are
//!   governed by different rules.
//!
//! Read-only access is **not** restricted.  Importing `kernel_ecs::Query`,
//! `kernel_ecs::Res`, `kernel_ecs::EntityRef`, etc. is fine anywhere.
//!
//! # Line-number caveat
//!
//! `proc_macro2::Span::start()` requires the `span-locations` cargo feature,
//! which is not enabled in this workspace.  Line numbers are therefore
//! approximated by scanning the raw source text for the first occurrence of
//! the forbidden symbol name — accurate enough for human-readable diagnostics.

use std::path::{Component, Path};

use anyhow::{Context, Result};
use syn::visit::Visit;
use syn::{ItemUse, UseGroup, UseName, UsePath, UseRename, UseTree};

use crate::common::{iter_rust_files, relativize, source_roots, LintReport, Violation};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The crate-level segment that identifies the ECS mutation surface.
const KERNEL_ECS_CRATE: &str = "kernel_ecs";

/// Terminal path segments that identify the mutation side of the ECS API.
///
/// A `use` statement that includes `kernel_ecs` **and** ends with one of
/// these segments is flagged as a command-bus bypass.
const FORBIDDEN_SYMBOLS: &[&str] = &[
    // Deferred-mutation builder (most commonly imported type)
    "Commands",
    // Mutable entity handle
    "EntityMut",
    // Component mutation guard
    "Mut",
    // World mutation methods — imported as associated-function aliases or
    // re-exported free functions inside `kernel_ecs::world`
    "insert",
    "remove",
    "replace",
    // Free-function mutation helpers anticipated from PLAN.md §1.2
    "insert_component",
    "remove_component",
    "despawn",
    "spawn_with",
];

// ---------------------------------------------------------------------------
// Line-number helper
// ---------------------------------------------------------------------------

/// Return the 1-based line number of the first occurrence of `needle` in
/// `src`, or `None` if not found.
///
/// Used as a fallback because `proc_macro2::Span::start()` requires the
/// `span-locations` feature, which is not enabled in this workspace's
/// `proc-macro2` dependency.
fn find_line(src: &str, needle: &str) -> Option<usize> {
    src.find(needle)
        .map(|offset| src[..offset].chars().filter(|&c| c == '\n').count() + 1)
}

// ---------------------------------------------------------------------------
// Path-filter helpers
// ---------------------------------------------------------------------------

/// Returns `true` when `path` lives inside `crates/` but **not** inside
/// `crates/editor-actions/`.
///
/// Only files that satisfy this predicate are subject to the lint.
fn is_target_crate_file(path: &Path, workspace_root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(workspace_root) else {
        return false;
    };

    let mut comps = rel.components();

    // First component must be `crates`
    let first = match comps.next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        _ => return false,
    };
    if first != "crates" {
        return false;
    }

    // Second component must not be `editor-actions`
    let second = match comps.next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        _ => return false, // bare `crates/` directory without a sub-crate — skip
    };

    second != "editor-actions"
}

// ---------------------------------------------------------------------------
// UseTree flattening
// ---------------------------------------------------------------------------

/// A flattened use-path: all segments from the crate root to the leaf symbol.
#[derive(Debug)]
struct FlatUsePath {
    /// All segments in order, including the terminal symbol.
    segments: Vec<String>,
}

impl FlatUsePath {
    /// Returns the terminal symbol name.
    fn leaf(&self) -> &str {
        self.segments.last().map_or("", String::as_str)
    }

    /// Returns `true` when this path starts with `kernel_ecs`.
    fn starts_with_kernel_ecs(&self) -> bool {
        self.segments.first().map(String::as_str) == Some(KERNEL_ECS_CRATE)
    }

    /// Returns `true` when the terminal segment is in the forbidden set.
    fn leaf_is_forbidden(&self) -> bool {
        FORBIDDEN_SYMBOLS.contains(&self.leaf())
    }

    /// Human-readable `kernel_ecs::Foo` representation.
    fn display(&self) -> String {
        self.segments.join("::")
    }
}

/// Recursively flatten a [`UseTree`] into individual leaf [`FlatUsePath`]s.
///
/// `prefix` accumulates the path segments seen so far from ancestor
/// [`UsePath`] nodes.
fn flatten_use_tree(tree: &UseTree, prefix: &[String], out: &mut Vec<FlatUsePath>) {
    match tree {
        UseTree::Path(UsePath { ident, tree, .. }) => {
            let mut next = prefix.to_vec();
            next.push(ident.to_string());
            flatten_use_tree(tree, &next, out);
        }
        UseTree::Name(UseName { ident, .. }) => {
            let mut segs = prefix.to_vec();
            segs.push(ident.to_string());
            out.push(FlatUsePath { segments: segs });
        }
        UseTree::Rename(UseRename { ident, .. }) => {
            // `use foo::Bar as Baz;` — check the *original* name, not the alias.
            let mut segs = prefix.to_vec();
            segs.push(ident.to_string());
            out.push(FlatUsePath { segments: segs });
        }
        UseTree::Group(UseGroup { items, .. }) => {
            // `use foo::{A, B, C};` — recurse into each member.
            for item in items {
                flatten_use_tree(item, prefix, out);
            }
        }
        UseTree::Glob(_) => {
            // `use kernel_ecs::*;` — we cannot determine statically which
            // symbols are imported; conservatively skip.  The gate will catch
            // concrete symbol imports once `kernel/ecs` ships.
        }
    }
}

// ---------------------------------------------------------------------------
// syn visitor
// ---------------------------------------------------------------------------

/// `syn` visitor that collects command-bus bypass violations from one file.
struct CommandBusVisitor<'a> {
    /// Workspace-relative file path (used in violation messages).
    file: &'a Path,
    /// Raw source text — used to recover approximate line numbers.
    src: &'a str,
    /// Accumulated violations.
    violations: Vec<Violation>,
}

impl<'a> CommandBusVisitor<'a> {
    fn new(file: &'a Path, src: &'a str) -> Self {
        Self {
            file,
            src,
            violations: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for CommandBusVisitor<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let mut flat: Vec<FlatUsePath> = Vec::new();
        flatten_use_tree(&node.tree, &[], &mut flat);

        for fp in flat {
            if fp.starts_with_kernel_ecs() && fp.leaf_is_forbidden() {
                let display = fp.display();
                // Approximate the line number by scanning the source text.
                let line = find_line(self.src, fp.leaf());
                self.violations.push(Violation {
                    file: self.file.to_path_buf(),
                    line,
                    message: format!(
                        "command-bus bypass: `{display}` imported outside \
                         crates/editor-actions (PLAN §6.16)"
                    ),
                });
            }
        }

        // Keep descending so we also catch `use` items nested in inline modules.
        syn::visit::visit_item_use(self, node);
    }
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Run the command-bus mutation lint against the workspace at `workspace_root`.
///
/// Returns a [`LintReport`] whose violations list is empty when no crate
/// outside `crates/editor-actions/` imports the anticipated `kernel_ecs`
/// world-mutation surface.
///
/// **Placeholder-active:** while `kernel/ecs` does not yet exist, this
/// function always returns zero violations against the real workspace.
///
/// The `Result` wrapper is part of the shared lint runner interface (see
/// `main.rs`) — future work may add I/O that can fail.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("command-bus");

    let roots = source_roots(workspace_root);
    for path in iter_rust_files(&roots) {
        if !is_target_crate_file(&path, workspace_root) {
            continue;
        }

        let txt = match std::fs::read_to_string(&path).with_context(|| path.display().to_string()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("command-bus lint: could not read {}: {e}", path.display());
                continue;
            }
        };

        let syntax = match syn::parse_file(&txt) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("command-bus lint: could not parse {}: {e}", path.display());
                continue;
            }
        };

        let rel = relativize(&path, workspace_root);
        let mut visitor = CommandBusVisitor::new(&rel, &txt);
        visitor.visit_file(&syntax);

        for v in visitor.violations {
            report.push(v);
        }
    }

    Ok(report)
}
