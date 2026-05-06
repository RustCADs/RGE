//! `rge-components-editor` — editor-only ECS markers.
//!
//! [`EditorOnlyRoot`] is the second of the two scene roots (PLAN.md §1.5.1
//! lists `SceneRoot` and `EditorOnlyRoot`); the cooked-build pipeline strips
//! everything under it. Gizmo markers live here too — the editor PIE smoke
//! test (W03) needs them and they aren't reusable outside the editor.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod editor_only_root;
mod gizmo;
mod selection;

pub use editor_only_root::EditorOnlyRoot;
pub use gizmo::{Gizmo, GizmoKind};
pub use selection::Selected;
