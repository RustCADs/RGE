//! RON `.icons.ron` manifest loader.
//!
//! Each icon set ships with a single manifest file mapping `IconName`
//! strings to relative SVG file paths. The manifest format is a tagged
//! RON struct so future fields (license attribution, default size hint,
//! etc.) can be added without breaking the wire format.
//!
//! Example `lucide.icons.ron`:
//!
//! ```ron
//! IconSetManifest(
//!     id: "lucide",
//!     license: "MIT",
//!     attribution: "Lucide — https://lucide.dev",
//!     icons: {
//!         "folder-open": "lucide/folder-open.svg",
//!         "save":        "lucide/save.svg",
//!     },
//! )
//! ```
//!
//! Paths in the manifest are resolved relative to the manifest file's
//! own directory. The loader rejects manifests whose paths escape the
//! manifest directory (no `..` traversal).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::icon_handle::{IconName, IconSetId};

/// On-disk schema for `<set>.icons.ron`. The struct shape is the public
/// wire format — adding fields is non-breaking iff the new field is
/// `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconSetManifest {
    /// Identifier this set will be registered under. Must round-trip
    /// through [`IconSetId::new`].
    pub id: String,
    /// SPDX-style license token (e.g. `"MIT"`). Stored only for
    /// attribution surfaces; not enforced by the loader.
    #[serde(default)]
    pub license: String,
    /// Human-readable attribution line, surfaced in the editor's
    /// About dialog.
    #[serde(default)]
    pub attribution: String,
    /// Map from icon name to SVG file path, relative to the manifest's
    /// directory.
    pub icons: BTreeMap<String, String>,
}

/// Parsed-and-validated icon set, ready to be registered.
///
/// Differs from [`IconSetManifest`] in three ways:
/// 1. Identifiers are validated via the typed wrappers.
/// 2. SVG paths are resolved to absolute paths anchored at the manifest
///    directory, with `..` traversal rejected.
/// 3. SVG file existence is *not* checked here — that's the registry's
///    job at lookup time, so manifests can validate even when assets
///    are still being downloaded.
#[derive(Debug, Clone)]
pub struct LoadedIconSet {
    /// Validated set identifier.
    pub id: IconSetId,
    /// SPDX license token from the manifest.
    pub license: String,
    /// Human-readable attribution string.
    pub attribution: String,
    /// Map from validated [`IconName`] to absolute SVG path.
    pub entries: BTreeMap<IconName, PathBuf>,
}

/// Errors produced by [`load_manifest`] / [`load_manifest_str`].
#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    /// I/O error reading the manifest file.
    #[error("I/O error reading {path:?}: {source}")]
    Io {
        /// Path the loader was attempting to read.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// RON parse error.
    #[error("RON parse error in {path:?}: {source}")]
    RonParse {
        /// Path being parsed when the failure occurred.
        path: PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::error::SpannedError,
    },
    /// RON parse error for in-memory string load.
    #[error("RON parse error: {0}")]
    RonStr(#[from] ron::error::SpannedError),
    /// Manifest validation failure (bad id, traversal attempt, etc.).
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

/// Read a `.icons.ron` manifest from disk and validate it.
///
/// The manifest's icon paths are resolved relative to the directory
/// containing the manifest file, then canonicalised by simple
/// component-stripping (no symlink resolution — that would require I/O
/// on every path and isn't needed here).
///
/// # Errors
/// - [`LoaderError::Io`] if the manifest file can't be read.
/// - [`LoaderError::RonParse`] if RON syntax is malformed.
/// - [`LoaderError::Invalid`] if a name/id fails validation, or if any
///   icon path attempts `..` traversal outside the manifest directory.
pub fn load_manifest(path: &Path) -> Result<LoadedIconSet, LoaderError> {
    let bytes = std::fs::read_to_string(path).map_err(|source| LoaderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    // Tolerate UTF-8 BOM that some Windows text editors prepend.
    let bytes = bytes.strip_prefix('\u{FEFF}').unwrap_or(&bytes);
    let manifest: IconSetManifest =
        ron::from_str(bytes).map_err(|source| LoaderError::RonParse {
            path: path.to_path_buf(),
            source,
        })?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    validate(manifest, base_dir)
}

/// Parse a manifest from an in-memory string with a caller-supplied
/// base directory. Useful for tests and for embedded asset bundles.
///
/// # Errors
/// Same as [`load_manifest`] minus the I/O variant.
pub fn load_manifest_str(ron_text: &str, base_dir: &Path) -> Result<LoadedIconSet, LoaderError> {
    let trimmed = ron_text.strip_prefix('\u{FEFF}').unwrap_or(ron_text);
    let manifest: IconSetManifest = ron::from_str(trimmed)?;
    validate(manifest, base_dir)
}

fn validate(manifest: IconSetManifest, base_dir: &Path) -> Result<LoadedIconSet, LoaderError> {
    let id = IconSetId::new(manifest.id.clone())
        .map_err(|e| LoaderError::Invalid(format!("set id {:?}: {e}", manifest.id)))?;

    let mut entries = BTreeMap::new();
    for (name_raw, rel_path) in manifest.icons {
        let name = IconName::new(name_raw.clone())
            .map_err(|e| LoaderError::Invalid(format!("icon name {name_raw:?}: {e}")))?;

        // Reject obvious traversal. We forbid any `..` component in the
        // declared relative path; we also forbid absolute paths in
        // manifests so sets are relocatable. Path::is_absolute is
        // platform-sensitive, so we additionally reject paths that
        // start with `/` or `\` to catch Linux-style absolute paths
        // even when running on Windows.
        let rel = Path::new(&rel_path);
        let starts_root =
            rel_path.starts_with('/') || rel_path.starts_with('\\') || rel.is_absolute();
        if starts_root {
            return Err(LoaderError::Invalid(format!(
                "icon {name_raw:?}: absolute path {rel_path:?} not allowed"
            )));
        }
        if rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(LoaderError::Invalid(format!(
                "icon {name_raw:?}: traversal {rel_path:?} not allowed"
            )));
        }

        entries.insert(name, base_dir.join(rel));
    }

    Ok(LoadedIconSet {
        id,
        license: manifest.license,
        attribution: manifest.attribution,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_manifest() {
        let ron = r#"
            IconSetManifest(
                id: "lucide",
                license: "MIT",
                attribution: "Lucide",
                icons: {
                    "folder-open": "lucide/folder-open.svg",
                    "save": "lucide/save.svg",
                },
            )
        "#;
        let base = Path::new("/tmp/x");
        let set = load_manifest_str(ron, base).expect("parse");
        assert_eq!(set.id.as_str(), "lucide");
        assert_eq!(set.entries.len(), 2);
        let save_path = set.entries.get(&IconName::new("save").unwrap()).unwrap();
        assert!(save_path.ends_with("lucide/save.svg") || save_path.ends_with("lucide\\save.svg"));
    }

    #[test]
    fn rejects_traversal() {
        let ron = r#"
            IconSetManifest(
                id: "evil",
                icons: {
                    "x": "../../../../etc/passwd",
                },
            )
        "#;
        let err = load_manifest_str(ron, Path::new("/tmp")).expect_err("must reject");
        assert!(matches!(err, LoaderError::Invalid(_)));
    }

    #[test]
    fn rejects_absolute_path() {
        let ron = r#"
            IconSetManifest(
                id: "evil",
                icons: {
                    "x": "/etc/passwd",
                },
            )
        "#;
        let err = load_manifest_str(ron, Path::new("/tmp")).expect_err("must reject");
        assert!(matches!(err, LoaderError::Invalid(_)));
    }

    #[test]
    fn rejects_bad_set_id() {
        let ron = r#"
            IconSetManifest(
                id: "bad id",
                icons: { "ok": "ok.svg" },
            )
        "#;
        let err = load_manifest_str(ron, Path::new("/tmp")).expect_err("must reject");
        assert!(matches!(err, LoaderError::Invalid(_)));
    }
}
