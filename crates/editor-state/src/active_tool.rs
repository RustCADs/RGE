//! `editor_state::active_tool` — the currently active editor tool.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15).

use serde::{Deserialize, Serialize};

/// The currently active editor tool. Drives gizmo + cursor + interaction
/// behavior. Single global state for now; per-viewport tool stacks are a
/// Phase 6 concern.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActiveTool {
    /// Default cursor / select-mode (no gizmo).
    #[default]
    Select,
    /// Translation gizmo (W key in Maya/Blender muscle memory).
    Translate,
    /// Rotation gizmo.
    Rotate,
    /// Scale gizmo.
    Scale,
    /// Brush / sculpt tool — applies a brush stroke at cursor position.
    Brush,
}

/// All variants in declaration order, for inspector dropdowns.
static ALL_VARIANTS: &[ActiveTool] = &[
    ActiveTool::Select,
    ActiveTool::Translate,
    ActiveTool::Rotate,
    ActiveTool::Scale,
    ActiveTool::Brush,
];

impl ActiveTool {
    /// Stable text label, primarily for the placeholder viewport overlay
    /// and audit-log records.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Translate => "Translate",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Brush => "Brush",
        }
    }

    /// Iterator over all variants in declaration order. Useful for
    /// inspector dropdowns.
    #[must_use]
    pub fn all() -> &'static [ActiveTool] {
        ALL_VARIANTS
    }
}

impl std::fmt::Display for ActiveTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_select() {
        assert_eq!(ActiveTool::default(), ActiveTool::Select);
    }

    #[test]
    fn label_matches_variant_name() {
        assert_eq!(ActiveTool::Select.label(), "Select");
        assert_eq!(ActiveTool::Translate.label(), "Translate");
        assert_eq!(ActiveTool::Rotate.label(), "Rotate");
        assert_eq!(ActiveTool::Scale.label(), "Scale");
        assert_eq!(ActiveTool::Brush.label(), "Brush");
    }

    #[test]
    fn all_returns_all_five_in_declaration_order() {
        let all = ActiveTool::all();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0], ActiveTool::Select);
        assert_eq!(all[1], ActiveTool::Translate);
        assert_eq!(all[2], ActiveTool::Rotate);
        assert_eq!(all[3], ActiveTool::Scale);
        assert_eq!(all[4], ActiveTool::Brush);
    }

    #[test]
    fn display_writes_label() {
        assert_eq!(ActiveTool::Translate.to_string(), "Translate");
        assert_eq!(ActiveTool::Brush.to_string(), "Brush");
    }
}
