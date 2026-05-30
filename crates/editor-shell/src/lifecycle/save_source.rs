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

    /// A human-friendly display name for the window title / status bar: the
    /// `.rge-scene` file name for a [`SaveSource::Scene`], or the **project
    /// folder** name (the parent directory of the `.rge-project`) for a
    /// [`SaveSource::Project`] — so a project reads as e.g. `my-game` rather than
    /// the literal `.rge-project`. Falls back to the file name (`.rge-project`)
    /// when the path has no usable parent directory. `None` only when no UTF-8
    /// name can be derived (no file name / non-UTF-8), matching the title/status
    /// behaviour of dropping unnameable sources.
    ///
    /// Pure-path: no manifest read (editor-shell is loader-free). Showing the
    /// manifest `name` field instead is a later refinement.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        match self {
            SaveSource::Scene(p) => p.file_name(),
            SaveSource::Project(p) => p
                .parent()
                .and_then(Path::file_name)
                .or_else(|| p.file_name()),
        }
        .and_then(|name| name.to_str())
    }
}
