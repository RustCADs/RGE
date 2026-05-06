//! [`Gizmo`] — editor manipulator entity component.
//!
//! Gizmos (translate / rotate / scale handles, custom widget gizmos for
//! splines / lights / colliders) are spawned as ECS entities under
//! [`crate::EditorOnlyRoot`]. The component just identifies the entity's
//! manipulator role; the actual hit-testing + drag logic lives in
//! `crates/editor-actions` (W03).

use serde::{Deserialize, Serialize};

/// Manipulator kind.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GizmoKind {
    /// Three-axis translate handle.
    #[default]
    Translate,
    /// Three-axis rotate handle.
    Rotate,
    /// Three-axis scale handle.
    Scale,
    /// Spline / curve point handle.
    SplinePoint,
    /// Custom gizmo provided by an editor plugin (the plugin owns
    /// interpretation; the component just keeps the entity in the gizmo
    /// system's pick set).
    Custom,
}

/// Gizmo component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Gizmo {
    /// Manipulator kind.
    pub kind: GizmoKind,
    /// Whether the gizmo is currently being dragged. Editor systems flip
    /// this; gameplay code should never read it.
    pub is_active: bool,
}

impl Default for Gizmo {
    fn default() -> Self {
        Self {
            kind: GizmoKind::Translate,
            is_active: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default() {
        let g = Gizmo::default();
        let s = ron::to_string(&g).expect("serialize");
        let back: Gizmo = ron::from_str(&s).expect("deserialize");
        assert_eq!(g, back);
    }

    #[test]
    fn round_trip_ron_rotate() {
        let g = Gizmo {
            kind: GizmoKind::Rotate,
            is_active: true,
        };
        let s = ron::to_string(&g).expect("serialize");
        let back: Gizmo = ron::from_str(&s).expect("deserialize");
        assert_eq!(g, back);
    }

    #[test]
    fn round_trip_ron_custom() {
        let g = Gizmo {
            kind: GizmoKind::Custom,
            is_active: false,
        };
        let s = ron::to_string(&g).expect("serialize");
        let back: Gizmo = ron::from_str(&s).expect("deserialize");
        assert_eq!(g, back);
    }
}
