//! [`EditorOnlyRoot`] — zero-sized scene-root marker stripped on cook.
//!
//! Per PLAN.md §1.5.1 the roots `SceneRoot` and `EditorOnlyRoot` partition
//! the world: the cook pipeline deletes the entire `EditorOnlyRoot` subtree
//! before writing the `.rge-pak` file. Editor cameras, gizmos, in-progress
//! authoring scaffolding all live under this root.

use serde::{Deserialize, Serialize};

/// Zero-sized "this is the editor-only scene root" marker.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EditorOnlyRoot;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let r = EditorOnlyRoot;
        let s = ron::to_string(&r).expect("serialize");
        let back: EditorOnlyRoot = ron::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<EditorOnlyRoot>(), 0);
    }
}
