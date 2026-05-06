//! Font registry — owns a [`cosmic_text::FontSystem`] and tracks which
//! families have been loaded from local asset bytes.
//!
//! The registry is intentionally thin. It does not do shaping or measurement
//! itself; consumers go through [`crate::Measure`] for that.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cosmic_text::fontdb::{self, Source};
use cosmic_text::FontSystem;

/// A single loaded face — one weight/slant of a family.
#[derive(Clone, Debug)]
pub struct FontFace {
    /// Internal fontdb identifier for the loaded face.
    pub id: fontdb::ID,
    /// Family name as reported by the font's `name` table.
    pub family: String,
    /// Optional file path — `None` if loaded from in-memory bytes.
    pub path: Option<PathBuf>,
    /// CSS-style font weight (100..900) read from the face.
    pub weight: u16,
    /// True iff the face declares italic / oblique slant.
    pub italic: bool,
    /// True iff the face advertises monospaced metrics.
    pub monospaced: bool,
}

/// Aggregate description of one registered family. Multiple [`FontFace`]
/// entries (e.g. Regular / Bold / Italic) may belong to the same family.
#[derive(Clone, Debug)]
pub struct RegisteredFamily {
    /// Family name as reported by the loaded faces.
    pub name: String,
    /// All faces belonging to this family.
    pub faces: Vec<FontFace>,
}

/// Errors produced when loading or registering fonts.
#[derive(Debug, thiserror::Error)]
pub enum FontRegistryError {
    /// The asset directory did not exist or could not be read.
    #[error("font asset directory `{path}` could not be read: {source}")]
    AssetDir {
        /// Path that failed to be read.
        path: PathBuf,
        /// Underlying I/O error from `std::fs::read_dir`.
        #[source]
        source: std::io::Error,
    },
    /// A specific font file could not be opened.
    #[error("font file `{path}` could not be read: {source}")]
    FontFile {
        /// Path of the offending font file.
        path: PathBuf,
        /// Underlying I/O error from `std::fs::read`.
        #[source]
        source: std::io::Error,
    },
    /// The file's bytes did not parse as a font that fontdb could index.
    #[error("font file `{path}` produced no parseable faces")]
    NoFacesFound {
        /// Path of the unparseable file.
        path: PathBuf,
    },
}

/// Owns a [`FontSystem`] and tracks the families loaded into it.
///
/// Construction does *not* automatically load system fonts; call
/// [`FontRegistry::with_system_fonts`] for that.
#[derive(Debug)]
pub struct FontRegistry {
    font_system: FontSystem,
    loaded: Vec<FontFace>,
}

impl FontRegistry {
    /// Build an empty registry. Only the explicitly loaded faces will be
    /// available — system fonts are *not* enumerated.
    #[must_use]
    pub fn new_empty() -> Self {
        let locale = String::from("en-US");
        let db = fontdb::Database::new();
        let font_system = FontSystem::new_with_locale_and_db(locale, db);
        Self {
            font_system,
            loaded: Vec::new(),
        }
    }

    /// Build a registry seeded with the platform's installed fonts. Equivalent
    /// to [`FontSystem::new`] but without the implicit Fira / `DejaVu`
    /// defaults — those are usually not present on Windows.
    #[must_use]
    pub fn with_system_fonts() -> Self {
        let font_system = FontSystem::new();
        Self {
            font_system,
            loaded: Vec::new(),
        }
    }

    /// Borrow the underlying [`FontSystem`] mutably. Useful when handing the
    /// system to cosmic-text APIs that require `&mut FontSystem`.
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    /// Borrow the underlying [`FontSystem`] immutably.
    #[must_use]
    pub fn font_system(&self) -> &FontSystem {
        &self.font_system
    }

    /// All faces that have been loaded into this registry through one of the
    /// `load_*` methods. Faces sourced from the OS are *not* listed here even
    /// when [`Self::with_system_fonts`] was used.
    #[must_use]
    pub fn loaded_faces(&self) -> &[FontFace] {
        &self.loaded
    }

    /// Group [`Self::loaded_faces`] by family name.
    #[must_use]
    pub fn families(&self) -> Vec<RegisteredFamily> {
        let mut by_name: std::collections::BTreeMap<String, Vec<FontFace>> =
            std::collections::BTreeMap::new();
        for face in &self.loaded {
            by_name
                .entry(face.family.clone())
                .or_default()
                .push(face.clone());
        }
        by_name
            .into_iter()
            .map(|(name, faces)| RegisteredFamily { name, faces })
            .collect()
    }

    /// Load a single font file by path. Returns the faces that the file
    /// contributed (a `.ttc` may contribute multiple).
    ///
    /// # Errors
    ///
    /// Returns [`FontRegistryError::FontFile`] if the file cannot be read and
    /// [`FontRegistryError::NoFacesFound`] if fontdb produced no faces.
    pub fn load_file(&mut self, path: &Path) -> Result<Vec<FontFace>, FontRegistryError> {
        let bytes = std::fs::read(path).map_err(|source| FontRegistryError::FontFile {
            path: path.to_path_buf(),
            source,
        })?;
        let arc: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(Source::Binary(arc));
        if ids.is_empty() {
            return Err(FontRegistryError::NoFacesFound {
                path: path.to_path_buf(),
            });
        }
        let mut new_faces = Vec::with_capacity(ids.len());
        for id in ids.iter().copied() {
            let Some(info) = self.font_system.db().face(id) else {
                continue;
            };
            // Family preference order: typographic family ("Inter") over
            // platform-style "Inter Bold" entries when both exist.
            let family_name = info
                .families
                .iter()
                .map(|(name, _lang)| name.clone())
                .next()
                .unwrap_or_default();
            let face = FontFace {
                id,
                family: family_name,
                path: Some(path.to_path_buf()),
                weight: info.weight.0,
                italic: !matches!(info.style, fontdb::Style::Normal),
                monospaced: info.monospaced,
            };
            new_faces.push(face.clone());
            self.loaded.push(face);
        }
        if new_faces.is_empty() {
            return Err(FontRegistryError::NoFacesFound {
                path: path.to_path_buf(),
            });
        }
        Ok(new_faces)
    }

    /// Load every `.ttf` / `.otf` in `dir` (non-recursively). Returns the
    /// total list of newly registered faces.
    ///
    /// # Errors
    ///
    /// Returns [`FontRegistryError::AssetDir`] if the directory itself cannot
    /// be enumerated. Per-file failures are propagated unchanged: the first
    /// bad font aborts the whole call so the caller can see the cause.
    pub fn load_dir(&mut self, dir: &Path) -> Result<Vec<FontFace>, FontRegistryError> {
        let entries = std::fs::read_dir(dir).map_err(|source| FontRegistryError::AssetDir {
            path: dir.to_path_buf(),
            source,
        })?;
        let mut all = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| FontRegistryError::AssetDir {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("ttf" | "otf")) {
                let mut faces = self.load_file(&path)?;
                all.append(&mut faces);
            }
        }
        Ok(all)
    }

    /// Recursively load every `.ttf` / `.otf` under `root`. Used to ingest
    /// the vendored `assets/fonts/` tree where each family lives in its own
    /// sub-directory.
    ///
    /// # Errors
    ///
    /// As [`Self::load_dir`].
    pub fn load_tree(&mut self, root: &Path) -> Result<Vec<FontFace>, FontRegistryError> {
        let mut all = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries =
                std::fs::read_dir(&dir).map_err(|source| FontRegistryError::AssetDir {
                    path: dir.clone(),
                    source,
                })?;
            for entry in entries {
                let entry = entry.map_err(|source| FontRegistryError::AssetDir {
                    path: dir.clone(),
                    source,
                })?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_ascii_lowercase);
                if matches!(ext.as_deref(), Some("ttf" | "otf")) {
                    let mut faces = self.load_file(&path)?;
                    all.append(&mut faces);
                }
            }
        }
        Ok(all)
    }

    /// Load font bytes that are already in memory. Used by tests and by
    /// callers that want to embed font assets via `include_bytes!`.
    ///
    /// # Errors
    ///
    /// Returns [`FontRegistryError::NoFacesFound`] if fontdb produced no
    /// faces from the supplied bytes.
    pub fn load_bytes(
        &mut self,
        bytes: Vec<u8>,
        hint_path: Option<PathBuf>,
    ) -> Result<Vec<FontFace>, FontRegistryError> {
        let arc: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(Source::Binary(arc));
        if ids.is_empty() {
            return Err(FontRegistryError::NoFacesFound {
                path: hint_path.unwrap_or_else(|| PathBuf::from("<bytes>")),
            });
        }
        let mut new_faces = Vec::with_capacity(ids.len());
        for id in ids.iter().copied() {
            let Some(info) = self.font_system.db().face(id) else {
                continue;
            };
            let family_name = info
                .families
                .iter()
                .map(|(name, _lang)| name.clone())
                .next()
                .unwrap_or_default();
            let face = FontFace {
                id,
                family: family_name,
                path: hint_path.clone(),
                weight: info.weight.0,
                italic: !matches!(info.style, fontdb::Style::Normal),
                monospaced: info.monospaced,
            };
            new_faces.push(face.clone());
            self.loaded.push(face);
        }
        if new_faces.is_empty() {
            return Err(FontRegistryError::NoFacesFound {
                path: hint_path.unwrap_or_else(|| PathBuf::from("<bytes>")),
            });
        }
        Ok(new_faces)
    }

    /// Path to the vendored `assets/fonts/` directory shipped with this crate
    /// at compile time. Resolves to `<crate-manifest>/assets/fonts/`.
    #[must_use]
    pub fn vendored_fonts_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("fonts")
    }

    /// Convenience: build a registry that includes the platform's system
    /// fonts *and* all vendored families shipped with this crate.
    ///
    /// # Errors
    ///
    /// Propagates [`FontRegistryError`] from [`Self::load_tree`].
    pub fn with_system_and_vendored() -> Result<Self, FontRegistryError> {
        let mut reg = Self::with_system_fonts();
        let dir = Self::vendored_fonts_dir();
        if dir.is_dir() {
            reg.load_tree(&dir)?;
        }
        Ok(reg)
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new_empty()
    }
}
