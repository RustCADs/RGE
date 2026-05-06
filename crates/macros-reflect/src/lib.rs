//! `rge-macros-reflect` — `#[derive(Reflect)]` proc-macro.
//!
//! adapted from rustforge::macros::rcad-property on 2026-05-05 — generalized
//! away from the dental `#[rcad(unit=..)]` namespace to the generic
//! `#[reflect(...)]` namespace, with `UiHint` / `SchemaVersion` / `validate`
//! attribute support.
//!
//! # Surface
//!
//! ```ignore
//! use rge_macros_reflect::Reflect;
//!
//! #[derive(Reflect)]
//! #[reflect(version = "1.0.0")]
//! pub struct Foo {
//!     #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.01)]
//!     pub roughness: f32,
//!
//!     #[reflect(skip)]
//!     pub cache: Vec<u8>,
//! }
//! ```
//!
//! # Hand-rolled vs dep-pull (PLAN.md §1.10)
//!
//! Built on the workspace-pinned `syn` + `quote` + `proc-macro2` triplet only.
//! No `darling`, no `attribute-derive`, no `proc-macro-crate`. Every attribute
//! is parsed by hand using `syn::Meta::parse_nested_meta`. This matches the
//! rustforge precedent and keeps the dependency floor at the workspace minimum.
//!
//! # Compile-time budget
//!
//! Per `IMPLEMENTATION.md` Phase 1.1 abort condition: 5 pilot reflected types
//! must compile in <30s. The macro emits exactly one `impl Reflect` block per
//! type, no helper traits, no inventory submission, no global registry — so
//! the generated code is O(fields) tokens. Baseline numbers in
//! `kernel/types/BUDGET.md`.

extern crate proc_macro;

use proc_macro::TokenStream;

mod attrs;
mod codegen;
mod derive;

/// Derive macro emitting an `impl rge_kernel_types::Reflect`.
///
/// # Container-level attributes (placed on the struct)
///
/// - `#[reflect(version = "x.y.z")]` — schema version (default `0.0.0`,
///   warning on the CI lint).
/// - `#[reflect(crate = "rge_kernel_types")]` — re-route the kernel-types
///   crate path; useful when the macro is used from inside a re-export.
///
/// # Field-level attributes
///
/// - `#[reflect(skip)]` — exclude from reflection AND from serde walk.
/// - `#[reflect(ui = "Default" | "Slider" | "ColorRgb" | "ColorRgba" |
///    "FilePath" | "EnumDropdown" | "Multiline" | "Curve" | "Gradient" |
///    "Foldout" | "Inline" | "Hidden")]`
/// - `#[reflect(min = .., max = .., step = ..)]` — slider params.
/// - `#[reflect(extensions = ["png", "jpg"])]` — file-path filter.
/// - `#[reflect(lines = N)]` — multiline rows.
/// - `#[reflect(default_open = true)]` — foldout initial state.
/// - `#[reflect(validate = "fn_path")]` — symbol path of validation fn.
/// - `#[reflect(custom_drawer = "fn_path")]` — symbol path of custom drawer.
///
/// Fields without `#[reflect(...)]` use `UiHint::Default` and are NOT
/// skipped — they appear in the descriptor table with default metadata.
/// This differs from rustforge's `RcadProperty` (opt-in); we want every
/// field walked by default so RON round-trips don't silently drop data.
#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    derive::expand(input)
}
