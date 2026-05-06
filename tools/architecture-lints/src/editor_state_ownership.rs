//! Editor-state ownership + coordination-not-authority lint — PLAN.md §1.15.
//!
//! ## Part A — Ownership
//!
//! The five coordination-state types (`Selection`, `Hover`, `ActiveTool`,
//! `ModalState`, `DragDrop`) may only be **defined** inside
//! `crates/editor-state/`.  Any `struct`, `enum`, or `type` alias with one of
//! those names found in another crate is a violation.
//!
//! `use … ::Selection` (re-import) is explicitly **not** flagged — that is the
//! correct usage pattern.
//!
//! ## Part B — Coordination-not-authority
//!
//! `crates/editor-state/` may only import IDs and handles from the kernel
//! tier.  It must **not** import authoritative content types (component bodies,
//! CAD-core nodes, asset payloads) from the Tier-2 crate family.  Any `use`
//! whose leading path segment matches one of the forbidden crate names is a
//! violation.
//!
//! Exception: `kernel/*` crates (paths starting with `kernel_`) are freely
//! importable — they only expose IDs and primitive handles.

use std::path::Path;

use anyhow::{Context, Result};
use syn::visit::Visit;
use syn::{ItemEnum, ItemStruct, ItemType, ItemUse, UseTree};

use crate::common::{iter_rust_files, relativize, source_roots, Exemptions, LintReport, Violation};

// ---------------------------------------------------------------------------
// Line-number helper
// ---------------------------------------------------------------------------

/// Return the 1-based line number of the first occurrence of `needle` in `src`,
/// or `None` if not found. Used because `proc_macro2::Span::start()` requires
/// the `span-locations` feature which is not activated in this workspace.
fn find_line_in_src(src: &str, needle: &str) -> Option<usize> {
    src.find(needle)
        .map(|byte_offset| src[..byte_offset].chars().filter(|&c| c == '\n').count() + 1)
}

// ---------------------------------------------------------------------------
// Constants — Part A
// ---------------------------------------------------------------------------

/// Type names that may only be **defined** inside `crates/editor-state/`.
const FORBIDDEN_TYPE_NAMES: &[&str] =
    &["Selection", "Hover", "ActiveTool", "ModalState", "DragDrop"];

/// Path component that identifies the `crates/editor-state` crate directory.
/// A file is "inside editor-state" when any component of its absolute path
/// equals this string.
const EDITOR_STATE_DIR: &str = "editor-state";

/// Path component that identifies this tool's own directory. Files living
/// under it (tests, fixtures) are skipped entirely so fixtures can freely use
/// forbidden names for test purposes.
const ARCHITECTURE_LINTS_DIR: &str = "architecture-lints";

// ---------------------------------------------------------------------------
// Constants — Part B
// ---------------------------------------------------------------------------

/// Crate name prefixes (using `_` instead of `-`, matching Rust path syntax)
/// whose content types must NOT be imported by `crates/editor-state`.
///
/// These represent authoritative Tier-2 content crates; editor-state must
/// coordinate through IDs/handles only.
const FORBIDDEN_IMPORT_PREFIXES: &[&str] = &[
    "cad_core",
    "cad_native",
    "cad_occt",
    "components_animation",
    "components_audio",
    "components_editor",
    "components_identity",
    "components_interaction",
    "components_lifecycle",
    "components_networking",
    "components_physics",
    "components_render",
    "components_spatial",
    "components_visibility",
    "material_graph",
    "material_runtime",
    "anim_clip",
    "anim_graph",
    "anim_ik",
    "asset_store",
    "pak_format",
    "io_gltf",
    "io_image",
    "io_step",
    "io_stl",
    "io_obj",
    "io_audio",
    "physics",
    "audio",
    "input",
];

// ---------------------------------------------------------------------------
// Helpers — path classification
// ---------------------------------------------------------------------------

/// Returns `true` when `path` lives anywhere under `crates/editor-state/`.
fn is_inside_editor_state(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == EDITOR_STATE_DIR)
}

/// Returns `true` when `path` lives anywhere under `tools/architecture-lints/`.
fn is_inside_architecture_lints(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == ARCHITECTURE_LINTS_DIR)
}

// ---------------------------------------------------------------------------
// Part A visitor — forbidden type definitions outside editor-state
// ---------------------------------------------------------------------------

/// `syn` visitor that collects violations for Part A of the lint.
///
/// It flags any top-level (or nested) `struct`, `enum`, or type-alias whose
/// name is one of [`FORBIDDEN_TYPE_NAMES`].
struct ForbiddenDefVisitor<'a> {
    /// Workspace-relative path (for error messages).
    file: &'a Path,
    /// Raw source text — used to recover approximate line numbers because
    /// `proc_macro2::Span::start()` requires the `span-locations` feature.
    src: &'a str,
    /// Accumulated violations.
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

    /// Record a Part-A violation for the given type name. Line is recovered
    /// by scanning the source text for the identifier.
    fn record(&mut self, name: &str) {
        let line = find_line_in_src(self.src, name);
        self.violations.push(Violation {
            file: self.file.to_path_buf(),
            line,
            message: format!(
                "forbidden type definition `{name}` outside crates/editor-state \
                 (PLAN §1.15 — must use coordination substrate)"
            ),
        });
    }

    /// Check whether `ident` is in the forbidden set.
    fn is_forbidden(ident: &syn::Ident) -> bool {
        FORBIDDEN_TYPE_NAMES.contains(&ident.to_string().as_str())
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
}

// ---------------------------------------------------------------------------
// Part B visitor — forbidden imports inside editor-state
// ---------------------------------------------------------------------------

/// `syn` visitor that collects violations for Part B of the lint.
///
/// It flags any `use` statement whose leading path segment matches one of the
/// [`FORBIDDEN_IMPORT_PREFIXES`].  `extern crate` declarations with forbidden
/// names are also flagged.
struct ForbiddenImportVisitor<'a> {
    /// Workspace-relative path (for error messages).
    file: &'a Path,
    /// Raw source text — used to recover approximate line numbers because
    /// `proc_macro2::Span::start()` requires the `span-locations` feature.
    src: &'a str,
    /// Accumulated violations.
    violations: Vec<Violation>,
}

impl<'a> ForbiddenImportVisitor<'a> {
    fn new(file: &'a Path, src: &'a str) -> Self {
        Self {
            file,
            src,
            violations: Vec::new(),
        }
    }

    /// Record a Part-B violation for the given crate name. Line is recovered
    /// by scanning the source text for the identifier.
    fn record(&mut self, crate_name: &str) {
        let line = find_line_in_src(self.src, crate_name);
        self.violations.push(Violation {
            file: self.file.to_path_buf(),
            line,
            message: format!(
                "editor-state imports authoritative content from `{crate_name}` \
                 (PLAN §1.15 — coordination-not-authority; only IDs/handles allowed)"
            ),
        });
    }

    /// Check whether `segment` is a forbidden import root.
    fn is_forbidden_crate(segment: &str) -> bool {
        FORBIDDEN_IMPORT_PREFIXES.contains(&segment)
    }

    /// Extract the leading path segment from a [`UseTree`], returning it as a
    /// `String`.
    ///
    /// Returns `None` for glob / braced trees whose root cannot be determined
    /// at this level (they are handled by recursion inside the visitor).
    fn leading_segment(tree: &UseTree) -> Option<String> {
        match tree {
            UseTree::Path(p) => Some(p.ident.to_string()),
            UseTree::Name(n) => Some(n.ident.to_string()),
            UseTree::Rename(r) => Some(r.ident.to_string()),
            // Glob / Group at the root level have no single leading segment.
            UseTree::Glob(_) | UseTree::Group(_) => None,
        }
    }
}

impl<'ast> Visit<'ast> for ForbiddenImportVisitor<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        if let Some(seg) = Self::leading_segment(&node.tree) {
            if Self::is_forbidden_crate(&seg) {
                self.record(&seg);
            }
        }
        // Do NOT recurse into the tree — the leading segment is all we need.
    }

    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        let name = node.ident.to_string();
        if Self::is_forbidden_crate(&name) {
            self.record(&name);
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Run the editor-state ownership + coordination-not-authority lint.
///
/// ## Part A
/// Every `.rs` file **outside** `crates/editor-state/` is checked for
/// definitions of [`FORBIDDEN_TYPE_NAMES`].  Finding one is a violation.
///
/// ## Part B
/// Every `.rs` file **inside** `crates/editor-state/` is checked for `use`
/// statements whose first path segment is one of [`FORBIDDEN_IMPORT_PREFIXES`].
/// Finding one is a violation.
///
/// Files inside `tools/architecture-lints/` are skipped entirely so that test
/// fixtures can freely use forbidden names.
///
/// # Errors
/// Returns an error if the workspace root is unreadable (e.g. permission
/// denied).  Individual unparseable files are silently skipped with a
/// diagnostic on stderr.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("editor-state-ownership");
    let exemptions = Exemptions::load(workspace_root)?;

    let roots = source_roots(workspace_root);
    for path in iter_rust_files(&roots) {
        // Skip this tool's own sources and test fixtures.
        if is_inside_architecture_lints(&path) {
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
                    "editor-state-ownership lint: could not read {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let syntax = match syn::parse_file(&txt) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "editor-state-ownership lint: could not parse {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        if is_inside_editor_state(&path) {
            // Part B — check for forbidden imports inside editor-state.
            let mut visitor = ForbiddenImportVisitor::new(&rel, &txt);
            visitor.visit_file(&syntax);
            for v in visitor.violations {
                report.push(v);
            }
        } else {
            // Part A — check for forbidden type definitions outside editor-state.
            let mut visitor = ForbiddenDefVisitor::new(&rel, &txt);
            visitor.visit_file(&syntax);
            for v in visitor.violations {
                report.push(v);
            }
        }
    }

    Ok(report)
}
