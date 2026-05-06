//! [`FramePhase`] — canonical ordered frame phases.

use serde::{Deserialize, Serialize};

/// Ordered frame phases. Iteration via [`FramePhase::ALL`] is canonical.
///
/// Discriminant values are stable; do not renumber them. New phases must be
/// inserted with a fresh, strictly increasing discriminant and added to
/// [`FramePhase::ALL`] in the correct position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum FramePhase {
    /// Drain input and queued events.
    Input = 0,
    /// Fixed-timestep sim. May run 0..N times per frame.
    FixedSim = 1,
    /// Variable-rate update (per-frame logic, animation interpolation).
    Update = 2,
    /// Late update (camera follow, post-physics anchoring).
    LateUpdate = 3,
    /// Render-snapshot staging (Phase-5 placeholder; emits a single Info
    /// diagnostic on each invocation).
    StageRender = 4,
    /// Frame end — diagnostics flush, frame counter advance.
    EndFrame = 5,
}

impl FramePhase {
    /// The canonical, fully-ordered sequence of all frame phases.
    ///
    /// Callers must iterate this slice (not a manually constructed list) so
    /// that a future phase insertion is automatically picked up everywhere.
    pub const ALL: &'static [Self] = &[
        Self::Input,
        Self::FixedSim,
        Self::Update,
        Self::LateUpdate,
        Self::StageRender,
        Self::EndFrame,
    ];

    /// A short, human-readable label for this phase (useful in diagnostics and
    /// profiler spans).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::FixedSim => "FixedSim",
            Self::Update => "Update",
            Self::LateUpdate => "LateUpdate",
            Self::StageRender => "StageRender",
            Self::EndFrame => "EndFrame",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_sorted_by_discriminant() {
        let discriminants: Vec<u8> = FramePhase::ALL.iter().map(|&p| p as u8).collect();
        let mut sorted = discriminants.clone();
        sorted.sort_unstable();
        assert_eq!(
            discriminants, sorted,
            "FramePhase::ALL must be in ascending discriminant order"
        );
    }

    #[test]
    fn ordering_matches_spec() {
        assert!(FramePhase::Input < FramePhase::FixedSim);
        assert!(FramePhase::FixedSim < FramePhase::Update);
        assert!(FramePhase::Update < FramePhase::LateUpdate);
        assert!(FramePhase::LateUpdate < FramePhase::StageRender);
        assert!(FramePhase::StageRender < FramePhase::EndFrame);
    }

    #[test]
    fn label_is_nonempty() {
        for &phase in FramePhase::ALL {
            assert!(!phase.label().is_empty());
        }
    }

    #[test]
    fn all_contains_six_phases() {
        assert_eq!(FramePhase::ALL.len(), 6);
    }
}
