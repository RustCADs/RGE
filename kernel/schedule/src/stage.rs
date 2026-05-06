//! Execution stage definitions.

use serde::{Deserialize, Serialize};

/// Ordered execution stages. Iterated in declaration order.
///
/// Stages are iterated from lowest discriminant to highest, guaranteeing that
/// [`EarlyUpdate`][Stage::EarlyUpdate] always completes before
/// [`Update`][Stage::Update], and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// Runs first — input processing, pre-simulation setup.
    EarlyUpdate = 0,
    /// Fixed-timestep simulation (physics, deterministic logic).
    FixedUpdate = 1,
    /// General-purpose per-frame logic.
    Update = 2,
    /// Runs last — post-processing, rendering prep, UI layout.
    LateUpdate = 3,
}

impl Stage {
    /// All stages in ascending execution order.
    pub const ALL: &'static [Stage] = &[
        Stage::EarlyUpdate,
        Stage::FixedUpdate,
        Stage::Update,
        Stage::LateUpdate,
    ];

    /// Human-readable label for this stage.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Stage::EarlyUpdate => "EarlyUpdate",
            Stage::FixedUpdate => "FixedUpdate",
            Stage::Update => "Update",
            Stage::LateUpdate => "LateUpdate",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_sorted() {
        let stages = Stage::ALL;
        for window in stages.windows(2) {
            assert!(window[0] < window[1], "Stage::ALL must be sorted ascending");
        }
    }

    #[test]
    fn ordering_is_correct() {
        assert!(Stage::EarlyUpdate < Stage::FixedUpdate);
        assert!(Stage::FixedUpdate < Stage::Update);
        assert!(Stage::Update < Stage::LateUpdate);
    }

    #[test]
    fn label_is_non_empty() {
        for &stage in Stage::ALL {
            assert!(!stage.label().is_empty());
        }
    }
}
