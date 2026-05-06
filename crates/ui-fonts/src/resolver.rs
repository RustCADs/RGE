//! Family-name → font-file path resolution and a system-font fallback chain.
//!
//! [`Resolver`] is intentionally split out from [`crate::FontRegistry`] so
//! that callers can configure the fallback chain (e.g. theme-driven family
//! preference order) without touching the registry's loaded set.

use std::path::PathBuf;

use cosmic_text::fontdb;

use crate::registry::FontRegistry;

/// Generic CSS-like font classes used as terminal fallbacks when a named
/// family cannot be matched.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GenericFamily {
    /// Proportional sans-serif (Inter / Segoe UI / system default).
    SansSerif,
    /// Proportional serif (Source Serif / `DejaVu` Serif / system default).
    Serif,
    /// Monospaced (`JetBrainsMono` / Consolas / system default).
    Monospace,
}

/// Resolution failure modes.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// No face anywhere in the registry — including system fonts and the
    /// generic fallback — matched the requested family.
    #[error("no font face matched any requested family in chain {tried:?}")]
    NoFaceForFamily {
        /// Family names that were attempted, in the order tried.
        tried: Vec<String>,
    },
}

/// Result of a successful resolution.
#[derive(Clone, Debug)]
pub struct ResolvedFace {
    /// Family name actually selected (may differ from the request when a
    /// fallback was used).
    pub family: String,
    /// Font ID inside `fontdb`.
    pub id: fontdb::ID,
    /// File path of the source, if available. `None` when the face was
    /// loaded from in-memory bytes.
    pub path: Option<PathBuf>,
    /// CSS weight (100..900).
    pub weight: u16,
    /// True iff italic / oblique.
    pub italic: bool,
}

/// Resolves a logical family name into a concrete loaded face.
#[derive(Clone, Debug)]
pub struct Resolver {
    /// Ordered fallback chain consulted when the requested family is not
    /// available. Built from [`Resolver::default_chain`] by default.
    fallback_chain: Vec<String>,
}

impl Resolver {
    /// Build a resolver with the default cross-platform fallback chain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fallback_chain: Self::default_chain(),
        }
    }

    /// Build a resolver with an explicit fallback chain, in priority order.
    #[must_use]
    pub fn with_fallback(chain: Vec<String>) -> Self {
        Self {
            fallback_chain: chain,
        }
    }

    /// The fallback families this resolver consults, in order.
    #[must_use]
    pub fn fallback_chain(&self) -> &[String] {
        &self.fallback_chain
    }

    /// Override the fallback chain.
    pub fn set_fallback(&mut self, chain: Vec<String>) {
        self.fallback_chain = chain;
    }

    /// Default fallback chain. Mixes vendored families, common Windows /
    /// macOS / Linux UI defaults, and a final terminal "Sans" placeholder.
    #[must_use]
    pub fn default_chain() -> Vec<String> {
        vec![
            "Inter".to_string(),
            "Segoe UI".to_string(),
            "SF Pro Text".to_string(),
            "Helvetica Neue".to_string(),
            "Helvetica".to_string(),
            "Arial".to_string(),
            "DejaVu Sans".to_string(),
            "Liberation Sans".to_string(),
            "Sans".to_string(),
        ]
    }

    /// Family chain corresponding to a [`GenericFamily`] class.
    #[must_use]
    pub fn generic_chain(class: GenericFamily) -> Vec<String> {
        match class {
            GenericFamily::SansSerif => Self::default_chain(),
            GenericFamily::Serif => vec![
                "Source Serif Pro".to_string(),
                "Cambria".to_string(),
                "Georgia".to_string(),
                "DejaVu Serif".to_string(),
                "Liberation Serif".to_string(),
                "Times New Roman".to_string(),
                "Serif".to_string(),
            ],
            GenericFamily::Monospace => vec![
                "JetBrains Mono".to_string(),
                "JetBrainsMono".to_string(),
                "Cascadia Mono".to_string(),
                "Consolas".to_string(),
                "Menlo".to_string(),
                "DejaVu Sans Mono".to_string(),
                "Liberation Mono".to_string(),
                "Courier New".to_string(),
                "Monospace".to_string(),
            ],
        }
    }

    /// Resolve a family name. Tries the requested name first, then walks
    /// [`Resolver::fallback_chain`].
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::NoFaceForFamily`] if neither the requested
    /// family nor any fallback could be matched in the registry's
    /// [`cosmic_text::FontSystem`] database.
    pub fn resolve(
        &self,
        registry: &FontRegistry,
        family: &str,
    ) -> Result<ResolvedFace, ResolveError> {
        let mut tried: Vec<String> = Vec::with_capacity(self.fallback_chain.len() + 1);
        tried.push(family.to_string());
        if let Some(face) = lookup_family(registry, family) {
            return Ok(face);
        }
        for fallback in &self.fallback_chain {
            if fallback.eq_ignore_ascii_case(family) {
                continue;
            }
            tried.push(fallback.clone());
            if let Some(face) = lookup_family(registry, fallback) {
                return Ok(face);
            }
        }
        Err(ResolveError::NoFaceForFamily { tried })
    }

    /// Resolve a [`GenericFamily`] class — chains through
    /// [`Self::generic_chain`] and the user-configured fallback.
    ///
    /// # Errors
    ///
    /// As [`Self::resolve`].
    pub fn resolve_generic(
        &self,
        registry: &FontRegistry,
        class: GenericFamily,
    ) -> Result<ResolvedFace, ResolveError> {
        let chain = Self::generic_chain(class);
        let mut tried: Vec<String> = Vec::with_capacity(chain.len() + self.fallback_chain.len());
        for family in &chain {
            tried.push(family.clone());
            if let Some(face) = lookup_family(registry, family) {
                return Ok(face);
            }
        }
        // Final pass through the user-configured fallback chain.
        for family in &self.fallback_chain {
            if chain.iter().any(|c| c.eq_ignore_ascii_case(family)) {
                continue;
            }
            tried.push(family.clone());
            if let Some(face) = lookup_family(registry, family) {
                return Ok(face);
            }
        }
        Err(ResolveError::NoFaceForFamily { tried })
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Look up a single family name in the registry's fontdb. Returns the
/// best-ranking face (preferring weight 400 / non-italic when both exist).
fn lookup_family(registry: &FontRegistry, family: &str) -> Option<ResolvedFace> {
    let db = registry.font_system().db();
    // Collect every face whose family list contains an exact match for
    // `family` (case-insensitive). Localized variants are accepted.
    let mut candidates: Vec<&fontdb::FaceInfo> = db
        .faces()
        .filter(|face| {
            face.families
                .iter()
                .any(|(name, _lang)| name.eq_ignore_ascii_case(family))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    // Rank: prefer weight closest to 400, prefer non-italic.
    candidates.sort_by_key(|face| {
        let weight_diff = i32::from(face.weight.0).abs_diff(400);
        let italic_penalty = u32::from(!matches!(face.style, fontdb::Style::Normal));
        (italic_penalty, weight_diff)
    });
    let info = candidates[0];
    let path = match &info.source {
        fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _) => Some(p.clone()),
        fontdb::Source::Binary(_) => None,
    };
    let chosen_family = info
        .families
        .iter()
        .find(|(name, _lang)| name.eq_ignore_ascii_case(family))
        .map_or_else(
            || {
                info.families
                    .first()
                    .map(|(n, _l)| n.clone())
                    .unwrap_or_default()
            },
            |(n, _l)| n.clone(),
        );
    Some(ResolvedFace {
        family: chosen_family,
        id: info.id,
        path,
        weight: info.weight.0,
        italic: !matches!(info.style, fontdb::Style::Normal),
    })
}
