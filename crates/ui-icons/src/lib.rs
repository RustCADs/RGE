//! `rge-ui-icons` — tintable SVG icon registry.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: icon-set failures (manifest parse error, malformed SVG
//! path, tint colour out of range, hot-reload swap exceeds 50ms SLO) are
//! transient and recoverable in-place — the registry retains the previous
//! set, surfaces a diagnostic, or skips the offending icon. No PIE state
//! is owned; the SVG cache is reproducible from manifest sources. Matches
//! ui-fonts + ui-theme + gfx (UI substrate classification).
//!
//! Phase-5 deliverable per `IMPLEMENTATION.md` §6.2.6 / Wave W06. This
//! crate is the substrate for editor toolbars, menus, panels, and any
//! other UI surface that needs vector icons recoloured at theme-swap
//! time.
//!
//! # Architecture
//!
//! - [`IconHandle`] is an opaque `(IconSetId, IconName)` tuple — the
//!   address by which UI code names an icon.
//! - [`IconRegistry`] owns one or more [`loader::LoadedIconSet`]s and
//!   knows which is currently active. Lookups by name return handles
//!   that resolve back to SVG bytes via the registry's lazy cache.
//! - [`tint::apply_tint`] / [`tint::rasterize`] together convert a
//!   monochrome Lucide-style SVG to RGBA pixels at a requested size,
//!   substituting the requested theme color for `currentColor`.
//!
//! Why a separate crate from `rge-ui-theme`? Per ADR-034 the icon-set
//! lifecycle is independent of the theme token lifecycle: a user may
//! swap from Lucide to Phosphor without touching colours, or change the
//! accent palette without re-loading icon SVGs.
//!
//! # Hot-reload
//!
//! [`IconRegistry::reload_set`] re-parses the manifest and drops the
//! SVG cache, returning the elapsed [`std::time::Duration`]. Tests
//! assert this stays under the 50 ms SLO from the spec.
//!
//! # Tinting model
//!
//! We deliberately avoid a full SVG renderer (`resvg`/`usvg`): Lucide
//! ships a narrow path-based subset that we render with a hand-rolled
//! parser on top of `tiny_skia`. This keeps the crate self-contained,
//! quick to build, and offline-buildable. If RGE ever needs to render
//! arbitrary user SVG, that's a separate problem with a separate
//! crate.

#![allow(
    clippy::result_large_err,
    reason = "loader / registry I/O is a cold-path file-IO operation; the rich `LoaderError` / `RegistryError` (thiserror-wrapped `ron::SpannedError` + `PathBuf`, ~136 bytes) is intentional for editor-side authoring diagnostics — boxing would force `Box<{Loader,Registry}Error>` callers without measurable benefit"
)]

pub mod icon_handle;
pub mod loader;
pub mod registry;
pub mod tint;
mod ui_theme_stub;

pub use icon_handle::{IconHandle, IconName, IconSetId, IdError};
pub use loader::{IconSetManifest, LoadedIconSet, LoaderError};
pub use registry::{IconRegistry, RegistryError};
pub use tint::{RasterIcon, TintError};
pub use ui_theme_stub::Color;
