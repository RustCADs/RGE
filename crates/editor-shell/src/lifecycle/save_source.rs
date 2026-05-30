//! [`SaveSource`] — the on-disk document the editor saves to on `Ctrl+S`.
//!
//! The save-side source model for the editor: a `.rge-scene` or a
//! `.rge-project`, held as `Option<SaveSource>` on [`EditorShell`]. Replaces a
//! former separate scene-source-path field so a scene-source and a
//! project-source cannot both be set (the illegal "both tracked" state is
//! unrepresentable). `.glb` is intentionally NOT a variant: it is a
//! *reload/watch* source (`glb_source_path`), not a *save* source.
//!
//! [`EditorShell`]: crate::EditorShell

use std::path::{Path, PathBuf};

/// The on-disk document the editor saves to on `Ctrl+S` — a `.rge-scene` or a
/// `.rge-project`. Held as `Option<SaveSource>` on [`EditorShell`]; `None` for
/// a blank / demo / `.glb` context (where `Ctrl+S` is Save-As). Replaces a
/// former separate scene-source-path field so a scene-source and a
/// project-source cannot both be set (illegal state unrepresentable).
///
/// `.glb` is intentionally NOT a variant: it is a *reload/watch* source
/// (`glb_source_path`), not a *save* source.
///
/// [`EditorShell`]: crate::EditorShell
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveSource {
    /// An opened/Save-As `.rge-scene`; `Ctrl+S` silently overwrites it.
    Scene(PathBuf),
    /// An opened literal `.rge-project`; `Ctrl+S` writes the world back to its
    /// first scene + re-writes the manifest (via `save_project_world_to_path`).
    Project(PathBuf),
}

impl SaveSource {
    /// The on-disk path of the open document (the `.rge-scene` file or the
    /// literal `.rge-project`).
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            SaveSource::Scene(p) | SaveSource::Project(p) => p,
        }
    }

    /// `true` for a [`SaveSource::Project`] source.
    #[must_use]
    pub fn is_project(&self) -> bool {
        matches!(self, SaveSource::Project(_))
    }
}
