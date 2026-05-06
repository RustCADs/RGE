// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
//! `rge-ui-theme` — token registry, RON-backed themes, inheritance,
//! variant stacking, and hot-reload for the RGE editor.
//!
//! Failure class: recoverable
//!
//! ## Module map
//!
//! * [`token`]       — leaf value vocabulary (Color, Length, Font, …).
//! * [`theme`]       — the RON-deserialisable [`Theme`] document.
//! * [`style`]       — per-widget [`Style`] + [`Slot`] resolution.
//! * [`variant`]     — variant axes and the user [`VariantStack`].
//! * [`registry`]    — [`ThemeRegistry`]: load + flatten + resolve.
//! * [`migration`]   — schema migrations on load.
//! * [`contrast`]    — WCAG AA contrast lint.
//! * [`hot_reload`]  — `notify`-backed file watcher.
//!
//! Diagnostics: imported directly from `rge-kernel-diagnostics` at use-sites;
//! the prior `crate::diagnostics` re-export module was dropped 2026-05-06
//! (post-Phase-1.2 indirection collapse — see deep reaudit findings).
//!
//! See `crates/ui-theme/assets/themes/*.theme.ron` for the four
//! vendored base themes (`dark-default`, `light-default`,
//! `studio-pro`, `daylight`).

#![warn(missing_debug_implementations)]

pub mod contrast;
pub mod hot_reload;
pub mod migration;
pub mod registry;
pub mod style;
pub mod theme;
pub mod token;
pub mod variant;

pub use registry::{RegistryError, Scope, ThemeRegistry};
pub use style::{ResolvedStyle, Slot, Style};
pub use theme::{Theme, CURRENT_THEME_VERSION, MAX_INHERITANCE_DEPTH};
pub use token::{
    AnimationToken, Color, Curve, EdgeInsets, FontToken, FontWeight, Length, ShadowToken, Token,
};
pub use variant::{Accessibility, ColorBlind, Scheme, VariantStack, VariantTag};

/// Convenience helper: directory containing the in-tree vendored
/// themes (relative to the crate manifest).
pub const VENDORED_THEME_DIR: &str = "assets/themes";
