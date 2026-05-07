//! `rge-ui-fonts` — UI font subsystem.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: font-subsystem failures (missing font file, cosmic-text
//! shaping error, glyph-cache atlas overflow, swap timeout) are transient
//! and recoverable in-place — the resolver falls back to the OS default,
//! the cache is rebuilt on next swap, or the editor surfaces a diagnostic.
//! No PIE state is owned; the glyph cache is reproducible from font sources.
//! Matches gfx + ui-icons + ui-theme (UI substrate classification).
//!
//! Wraps `cosmic-text` to provide:
//!
//! * [`FontRegistry`] — loads font files from a vendored `assets/` directory
//!   and registers them by family name on top of the system font database.
//! * [`Resolver`] — maps a family name (or generic class) to a concrete
//!   font file path, with a fallback chain that ends at the OS default.
//! * [`Measure`] — wraps the cosmic-text shaping API and exposes a small
//!   measurement surface (advance width / line height / glyph positions).
//! * [`GlyphCache`] — a coarse glyph-image atlas keyed by `(font, glyph,
//!   subpixel-key)`; invalidated wholesale on font swap so the swap budget
//!   stays under 100 ms.
//!
//! The crate ships vendored copies of [Inter] (Regular/Bold/Italic) and
//! [`JetBrainsMono`] (Regular/Bold) under the SIL Open Font License. See the
//! per-family `LICENSE-OFL.txt` next to each `.ttf` for the full text.
//!
//! [Inter]: https://github.com/rsms/inter
//! [`JetBrainsMono`]: https://github.com/JetBrains/JetBrainsMono
//!
//! # Architecture context
//!
//! Architecture frozen at v0.8 (PLAN §6.2). This crate is the implementation
//! deliverable for Wave W07 (PLAN §6.2.7). Sibling crates `ui-theme` (W05) and
//! `ui-icons` (W06) reference family names that resolve through this crate.

#![forbid(unsafe_code)]

pub mod glyph_cache;
pub mod measure;
pub mod registry;
pub mod resolver;

/// Re-export of the underlying `cosmic-text` crate so downstream callers can
/// reach raw [`cosmic_text::FontSystem`] / [`cosmic_text::Buffer`] when needed.
pub use cosmic_text;
pub use glyph_cache::{GlyphCache, GlyphImage, GlyphKey};
pub use measure::{FontSlant, FontWeightHint, GlyphPos, Measure, MeasuredText};
pub use registry::{FontFace, FontRegistry, FontRegistryError, RegisteredFamily};
pub use resolver::{GenericFamily, ResolveError, Resolver};
