//! Graph-foundation usage lint — PLAN.md §1.14.
//!
//! Two checks, both rooted in the same substrate doctrine:
//!
//! **Check 1 — forbidden-name redefinition.** No crate outside
//! `kernel/graph-foundation/` may define its own `NodeId`, `EdgeId`, or
//! `StableHash` types or traits. All consumers must import the substrate
//! primitives from `kernel/graph-foundation`.
//!
//! **Check 2 — adjacency-map reinvention** (added 2026-05-09 per audit-5
//! deep-audit followup). No crate outside `kernel/graph-foundation/` may
//! define a struct field of shape `BTreeMap<K, BTreeSet<K>>` or
//! `HashMap<K, HashSet<K>>` where the outer key type equals the inner set's
//! element type. That shape is the canonical "I'm reinventing graph storage"
//! pattern (an adjacency map). The proper substrate is
//! `kernel/graph-foundation::Graph<N, E>`. Without this check, audit-1 found
//! `kernel/asset::DependencyGraph` had silently rolled its own graph via
//! `BTreeMap<AssetId, BTreeSet<AssetId>>` — Check 1 didn't catch it because
//! no NodeId / EdgeId / StableHash redefinition was involved.
//!
//! The lint walks every `.rs` file in the workspace (excluding
//! `kernel/graph-foundation/` itself) and uses `syn` to look for top-level or
//! nested item definitions whose identifier matches the forbidden set, plus
//! struct fields whose type matches the adjacency-map shape.

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

/// Map types whose `Map<K, Set<K>>` shape signals adjacency reinvention.
const ADJACENCY_MAP_TYPES: &[&str] = &["BTreeMap", "HashMap"];

/// Set types whose appearance as the value type completes the pattern.
const ADJACENCY_SET_TYPES: &[&str] = &["BTreeSet", "HashSet"];

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

    /// Record an adjacency-map-reinvention violation.
    fn record_adjacency(&mut self, field_name: &str, type_repr: &str) {
        // Locate the field by name; fall back to scanning the type repr if
        // the name search misses (anonymous tuple-struct fields etc.).
        let line = find_line(self.src, field_name).or_else(|| find_line(self.src, type_repr));
        self.violations.push(Violation {
            file: self.file.to_path_buf(),
            line,
            message: format!(
                "field `{field_name}: {type_repr}` is an adjacency-map shape \
                 (forbidden outside kernel/graph-foundation per PLAN §1.14 — \
                 use kernel/graph-foundation::Graph<N, E> instead)"
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Adjacency-map detection helpers
// ---------------------------------------------------------------------------

/// Last path-segment ident of a Rust path, e.g. `std::collections::BTreeMap` →
/// `Some("BTreeMap")`. Returns `None` if the path is empty.
fn last_segment_ident(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|s| s.ident.to_string())
}

/// Render the matched `Map<K, Set<K>>` shape for the violation message.
/// Uses `K` as a placeholder rather than the raw token stream — sufficient
/// for the user to recognise the pattern in source.
fn render_adjacency_shape(map_ident: &str, set_ident: &str) -> String {
    format!("{map_ident}<K, {set_ident}<K>>")
}

/// If `ty` is `BTreeMap<K, BTreeSet<K>>` or `HashMap<K, HashSet<K>>` (with K
/// matching between outer key and inner set's element), return
/// `Some(rendered_shape)`. Otherwise return `None`.
///
/// Returns `None` for any other shape, including `BTreeMap<K, V>` where V is
/// not a Set, or `BTreeMap<K, BTreeSet<L>>` where K != L (e.g. permissions
/// maps `BTreeMap<UserId, BTreeSet<Permission>>`).
///
/// Comparison uses `syn::Type`'s native `PartialEq` (enabled via syn's
/// `extra-traits` feature already in this crate's manifest).
fn detect_adjacency_map(ty: &syn::Type) -> Option<String> {
    let path = match ty {
        syn::Type::Path(tp) => &tp.path,
        _ => return None,
    };

    let last = last_segment_ident(path)?;
    if !ADJACENCY_MAP_TYPES.contains(&last.as_str()) {
        return None;
    }

    let args = match &path.segments.last()?.arguments {
        syn::PathArguments::AngleBracketed(a) => &a.args,
        _ => return None,
    };

    if args.len() != 2 {
        return None;
    }

    let key_ty = match &args[0] {
        syn::GenericArgument::Type(t) => t,
        _ => return None,
    };
    let val_ty = match &args[1] {
        syn::GenericArgument::Type(t) => t,
        _ => return None,
    };

    let val_path = match val_ty {
        syn::Type::Path(tp) => &tp.path,
        _ => return None,
    };
    let val_last = last_segment_ident(val_path)?;
    if !ADJACENCY_SET_TYPES.contains(&val_last.as_str()) {
        return None;
    }

    let val_args = match &val_path.segments.last()?.arguments {
        syn::PathArguments::AngleBracketed(a) => &a.args,
        _ => return None,
    };
    if val_args.len() != 1 {
        return None;
    }
    let elem_ty = match &val_args[0] {
        syn::GenericArgument::Type(t) => t,
        _ => return None,
    };

    if key_ty == elem_ty {
        Some(render_adjacency_shape(&last, &val_last))
    } else {
        None
    }
}

impl<'ast> Visit<'ast> for ForbiddenDefVisitor<'_> {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if Self::is_forbidden(&node.ident) {
            let name = node.ident.to_string();
            self.record(&name);
        }
        // Check 2: adjacency-map field shapes.
        for (i, field) in node.fields.iter().enumerate() {
            if let Some(repr) = detect_adjacency_map(&field.ty) {
                let field_name = field
                    .ident
                    .as_ref()
                    .map_or_else(|| format!("{i}"), syn::Ident::to_string);
                self.record_adjacency(&field_name, &repr);
            }
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
