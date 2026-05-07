//! [`UiHint`] — closed-set vocabulary for inspector binding (§6.15).
//!
//! adapted from rustforge's egui property-grid behaviour on 2026-05-05 —
//! generalized away from dental-domain `RcadUnit` to a UI-binding hint.
//!
//! # Closed-set rule
//!
//! Per `PLAN.md` §6.15: the UI-hint enum is intentionally small. New variants
//! require explicit review (CI lint planned in Phase 4 once `tools/architecture-lints`
//! lands). Adding `Custom(String)` is forbidden — that would defeat the lint.
//! Custom drawers are wired through `#[reflect(custom_drawer = "fn_path")]`,
//! NOT through this enum.
//!
//! # Why an enum (not a flag bag)
//!
//! Each variant carries its own payload (slider min/max, file-extension
//! whitelist, multiline row count). A bag of bool flags would force the
//! inspector to look up payload from a side table, which fights the
//! "`FieldDescriptor` is `&'static`" invariant.

use serde::Serialize;

/// Inspector-binding hint per reflected field.
///
/// Variants intentionally mirror what an egui-based property grid (W08
/// `editor-ui/menus`) needs for a non-trivial inspector. The order is
/// stable; serde uses externally-tagged form (default for serde + RON),
/// so adding variants at the end is forward-compatible.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub enum UiHint {
    /// No hint — inspector picks a default widget for the field type
    /// (e.g. text-edit for strings, drag-value for f32).
    #[default]
    Default,

    /// Bounded numeric — inspector renders a slider. Min/max in the source
    /// numeric type, coerced to f64 for storage. `step` is the increment;
    /// `0.0` means "let widget choose".
    Slider {
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
        /// Increment per drag tick. `0.0` ⇒ widget default.
        step: f64,
    },

    /// 3-channel sRGB color (linear values 0..=1 in the field; widget
    /// converts to sRGB for display).
    ColorRgb,

    /// 4-channel sRGB color with alpha.
    ColorRgba,

    /// File-path picker. `extensions` is the whitelist (without the dot,
    /// e.g. `["png", "jpg"]`); empty slice ⇒ any file. The list is
    /// `&'static [&'static str]` because the macro emits a const slice.
    FilePath {
        /// Allowed extensions (lowercase, no leading dot). Empty = any.
        extensions: &'static [&'static str],
    },

    /// Enum dropdown — inspector shows variant names. Variant list is
    /// pulled by the inspector from the enum's `Reflect::variants()`
    /// (not yet implemented in Phase 1.1; placeholder hint).
    EnumDropdown,

    /// Multiline text edit. `lines` is the suggested visible row count.
    Multiline {
        /// Suggested visible rows (widget may grow on demand).
        lines: u16,
    },

    /// Animation curve — inspector renders the W11 / W18 curve editor
    /// (not yet built; this is a forward-declared hint).
    Curve,

    /// Color or scalar gradient — inspector renders the gradient editor.
    Gradient,

    /// Foldout group header. Children render inside the foldout; the
    /// `default_open` flag drives initial UI state.
    Foldout {
        /// Whether the foldout starts expanded.
        default_open: bool,
    },

    /// Render the field inline (no row label) — useful for newtype-shaped
    /// wrapper fields where the inner type already provides labels.
    Inline,

    /// Hide from inspector entirely (still serialized — use
    /// `#[reflect(skip)]` to also skip serde).
    Hidden,
}

impl UiHint {
    /// True if this hint signals the inspector to omit the field from the UI.
    #[must_use]
    pub const fn hides_in_inspector(&self) -> bool {
        matches!(self, UiHint::Hidden)
    }

    /// True if this hint expects a numeric field type. Drives the W08 lint
    /// that flags `Slider` on a `String` field.
    #[must_use]
    pub const fn expects_numeric(&self) -> bool {
        matches!(self, UiHint::Slider { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_default_variant() {
        assert!(matches!(UiHint::default(), UiHint::Default));
    }

    #[test]
    fn hidden_predicate() {
        assert!(UiHint::Hidden.hides_in_inspector());
        assert!(!UiHint::Default.hides_in_inspector());
        assert!(!UiHint::ColorRgb.hides_in_inspector());
    }

    #[test]
    fn numeric_predicate() {
        assert!(UiHint::Slider {
            min: 0.0,
            max: 1.0,
            step: 0.0
        }
        .expects_numeric());
        assert!(!UiHint::ColorRgb.expects_numeric());
        assert!(!UiHint::Multiline { lines: 4 }.expects_numeric());
    }

    #[test]
    fn slider_serializes_to_ron() {
        let hint = UiHint::Slider {
            min: 0.0,
            max: 1.0,
            step: 0.01,
        };
        let s = ron::to_string(&hint).unwrap();
        // UiHint is Serialize-only — it carries `&'static` slices that
        // cannot round-trip through Deserialize. Diagnostic / wire output
        // only.
        assert!(s.contains("Slider"));
    }

    #[test]
    fn file_path_with_extensions() {
        let hint = UiHint::FilePath {
            extensions: &["png", "jpg", "gltf"],
        };
        // Round-trip via JSON (RON struggles with `&'static` slices on
        // deserialize, which is fine — the hint is constructed by the
        // derive macro, not loaded from disk).
        let s = serde_json::to_string(&hint).unwrap();
        assert!(s.contains("\"png\""));
    }
}
