// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Variant axes for theme stacking. Per PLAN.md §6.2 themes compose
// along orthogonal axes:
//
//   1. scheme (dark / light)
//   2. accessibility (high-contrast / reduced-motion / large-text /
//      reduced-transparency)
//   3. color-blind (protanopia / deuteranopia / tritanopia)
//
// Plus a fourth implicit axis: per-user override. The `VariantStack`
// resolves a final `Theme` by walking the tree:
//
//   base → scheme → accessibility (multi-pick) → color-blind → user override
//
// Variant overlays are stored *as themes* with a `variants:` tag; the
// registry merges their tokens on top of the base. Accessibility
// transforms (`reduced-motion` zeroing animation tokens, etc.) are
// data-driven where possible and fall back to a small set of
// hard-coded post-resolution mutators for cases that can't be
// expressed purely as token overrides.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// One slot on the four-axis variant stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum VariantTag {
    /// Colour scheme axis.
    Scheme(Scheme),
    /// Accessibility axis (multi-select).
    Accessibility(Accessibility),
    /// Colour-blind axis (single-select).
    ColorBlind(ColorBlind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Scheme {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Accessibility {
    HighContrast,
    ReducedMotion,
    LargeText,
    ReducedTransparency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ColorBlind {
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

impl VariantTag {
    /// Stable axis index used to order variants when resolving. Lower
    /// values apply earlier (i.e. are more easily overridden).
    pub fn axis_priority(&self) -> u8 {
        match self {
            VariantTag::Scheme(_) => 0,
            VariantTag::Accessibility(_) => 1,
            VariantTag::ColorBlind(_) => 2,
        }
    }
}

/// User-selected variant stack. The registry walks this in priority
/// order to assemble the active theme. Stored as an ordered set so
/// equivalent stacks compare equal regardless of insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantStack {
    pub tags: BTreeSet<VariantTag>,
}

impl VariantStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, tag: VariantTag) -> Self {
        self.tags.insert(tag);
        self
    }

    pub fn add(&mut self, tag: VariantTag) {
        self.tags.insert(tag);
    }

    pub fn remove(&mut self, tag: &VariantTag) {
        self.tags.remove(tag);
    }

    pub fn contains(&self, tag: &VariantTag) -> bool {
        self.tags.contains(tag)
    }

    /// All tags in axis-priority order (scheme → a11y → color-blind).
    /// Within an axis, ordering is the `Ord` derivation, which is
    /// stable (and the registry treats them as commutative within an
    /// axis).
    pub fn ordered(&self) -> Vec<VariantTag> {
        let mut v: Vec<VariantTag> = self.tags.iter().copied().collect();
        v.sort_by_key(|t| (t.axis_priority(), *t));
        v
    }

    /// Convenience: does the stack request the reduced-motion
    /// transform? Used by post-resolution mutators.
    pub fn has_reduced_motion(&self) -> bool {
        self.contains(&VariantTag::Accessibility(Accessibility::ReducedMotion))
    }

    /// Convenience: does the stack request reduced-transparency?
    pub fn has_reduced_transparency(&self) -> bool {
        self.contains(&VariantTag::Accessibility(
            Accessibility::ReducedTransparency,
        ))
    }

    /// Convenience: does the stack request large-text?
    pub fn has_large_text(&self) -> bool {
        self.contains(&VariantTag::Accessibility(Accessibility::LargeText))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_priority_ordering() {
        let scheme = VariantTag::Scheme(Scheme::Dark);
        let a11y = VariantTag::Accessibility(Accessibility::HighContrast);
        let cb = VariantTag::ColorBlind(ColorBlind::Protanopia);
        assert!(scheme.axis_priority() < a11y.axis_priority());
        assert!(a11y.axis_priority() < cb.axis_priority());
    }

    #[test]
    fn stack_dedup() {
        let mut s = VariantStack::new();
        s.add(VariantTag::Scheme(Scheme::Dark));
        s.add(VariantTag::Scheme(Scheme::Dark));
        assert_eq!(s.tags.len(), 1);
    }

    #[test]
    fn stack_ordered_axis_priority() {
        let s = VariantStack::new()
            .with(VariantTag::ColorBlind(ColorBlind::Tritanopia))
            .with(VariantTag::Scheme(Scheme::Dark))
            .with(VariantTag::Accessibility(Accessibility::HighContrast));
        let ordered = s.ordered();
        assert_eq!(ordered[0].axis_priority(), 0);
        assert_eq!(ordered[1].axis_priority(), 1);
        assert_eq!(ordered[2].axis_priority(), 2);
    }

    #[test]
    fn convenience_flags() {
        let s = VariantStack::new()
            .with(VariantTag::Accessibility(Accessibility::ReducedMotion))
            .with(VariantTag::Accessibility(
                Accessibility::ReducedTransparency,
            ));
        assert!(s.has_reduced_motion());
        assert!(s.has_reduced_transparency());
        assert!(!s.has_large_text());
    }
}
