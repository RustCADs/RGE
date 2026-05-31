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
    ///
    /// `path` is the literal `.rge-project`; `name` is the manifest's declared
    /// `name`, captured at open / launch time by the loader-aware caller for
    /// display (`None` when unavailable — editor-shell is loader-free and never
    /// reads the manifest here).
    Project {
        /// The on-disk `.rge-project` path.
        path: PathBuf,
        /// The manifest `name` for display, or `None` to fall back to the
        /// project folder name. See [`SaveSource::display_name`].
        name: Option<String>,
    },
}

impl SaveSource {
    /// The on-disk path of the open document (the `.rge-scene` file or the
    /// literal `.rge-project`).
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            SaveSource::Scene(p) => p,
            SaveSource::Project { path, .. } => path,
        }
    }

    /// `true` for a [`SaveSource::Project`] source.
    #[must_use]
    pub fn is_project(&self) -> bool {
        matches!(self, SaveSource::Project { .. })
    }

    /// A human-friendly display name for the window title / status bar.
    ///
    /// - [`SaveSource::Scene`] → the `.rge-scene` file name.
    /// - [`SaveSource::Project`] → the manifest's declared `name` when present
    ///   (and non-empty), so a project reads as e.g. `My Cool Game`; otherwise
    ///   the **project folder** name (the parent directory of the
    ///   `.rge-project`, e.g. `my-game`), falling back to the file name
    ///   (`.rge-project`) when the path has no usable parent directory.
    ///
    /// The manifest `name` is supplied by the loader-aware caller at open /
    /// launch time (editor-shell stays loader-free — `display_name` performs no
    /// disk read). `None` only when no UTF-8 name can be derived (no file name /
    /// non-UTF-8), matching the title/status behaviour of dropping unnameable
    /// sources.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        match self {
            SaveSource::Scene(p) => p.file_name().and_then(|name| name.to_str()),
            SaveSource::Project { path, name } => {
                // Prefer a present, non-empty manifest name; an empty name must
                // not blank the title, so fall back to the project folder name.
                match name.as_deref().filter(|n| !n.is_empty()) {
                    Some(name) => Some(name),
                    None => path
                        .parent()
                        .and_then(Path::file_name)
                        .or_else(|| path.file_name())
                        .and_then(|name| name.to_str()),
                }
            }
        }
    }
}
