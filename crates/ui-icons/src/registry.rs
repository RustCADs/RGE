//! [`IconRegistry`] — load icon sets, lookup by name, switch active set.
//!
//! Lifecycle, in expected order:
//!
//! 1. [`IconRegistry::new`] — create an empty registry.
//! 2. [`IconRegistry::register_set`] — add one or more icon sets,
//!    each loaded from a `.icons.ron` manifest. The first registered
//!    set becomes active by default.
//! 3. [`IconRegistry::set_active`] — optionally switch the active set
//!    (theme/skin swap).
//! 4. [`IconRegistry::lookup`] — given an icon name, return an
//!    [`IconHandle`] referencing the icon in the currently-active set
//!    (or [`None`] if the name doesn't exist).
//! 5. [`IconRegistry::svg_bytes`] — resolve a handle back to raw SVG
//!    bytes, lazily reading from disk and caching on first hit.
//! 6. [`IconRegistry::reload_set`] — invalidate cache and re-read the
//!    manifest from disk; backs the editor's hot-reload watcher.
//!
//! ## Threading
//!
//! `IconRegistry` is not internally synchronised. The editor uses one
//! per app and reads from the main UI thread; if a future panel needs
//! cross-thread access, wrap in `Arc<RwLock<_>>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::icon_handle::{IconHandle, IconName, IconSetId};
use crate::loader::{self, LoadedIconSet};

/// Errors produced by the registry.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Wrapper around a [`loader::LoaderError`] from manifest parsing.
    #[error(transparent)]
    Loader(#[from] loader::LoaderError),
    /// Set with the given id was not registered.
    #[error("icon set {0:?} is not registered")]
    UnknownSet(IconSetId),
    /// Icon with the given name was not found in its set.
    #[error("icon {name:?} not found in set {set:?}")]
    UnknownIcon {
        /// Set the lookup targeted.
        set: IconSetId,
        /// Name that could not be resolved.
        name: IconName,
    },
    /// Failed to read SVG file from disk.
    #[error("I/O error reading {path:?}: {source}")]
    Io {
        /// Path being read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug)]
struct SetState {
    loaded: LoadedIconSet,
    /// Cache of name → SVG bytes; populated lazily on first lookup.
    cache: BTreeMap<IconName, String>,
    /// Manifest path used for [`IconRegistry::reload_set`].
    manifest_path: Option<PathBuf>,
}

/// Central registry of icon sets and the active selection.
#[derive(Debug, Default)]
pub struct IconRegistry {
    sets: BTreeMap<IconSetId, SetState>,
    active: Option<IconSetId>,
}

impl IconRegistry {
    /// Create an empty registry with no sets and no active set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an already-loaded icon set.
    ///
    /// If this is the first set registered, it becomes the active set.
    /// Re-registering the same id replaces the previous entry but
    /// preserves the active selection.
    pub fn register_loaded(&mut self, set: LoadedIconSet) {
        let id = set.id.clone();
        let state = SetState {
            loaded: set,
            cache: BTreeMap::new(),
            manifest_path: None,
        };
        self.sets.insert(id.clone(), state);
        if self.active.is_none() {
            self.active = Some(id);
        }
    }

    /// Load a manifest from disk and register the resulting set.
    ///
    /// # Errors
    /// - [`RegistryError::Loader`] if the manifest is malformed.
    pub fn register_set(&mut self, manifest_path: &Path) -> Result<IconSetId, RegistryError> {
        let loaded = loader::load_manifest(manifest_path)?;
        let id = loaded.id.clone();
        let state = SetState {
            loaded,
            cache: BTreeMap::new(),
            manifest_path: Some(manifest_path.to_path_buf()),
        };
        self.sets.insert(id.clone(), state);
        if self.active.is_none() {
            self.active = Some(id.clone());
        }
        Ok(id)
    }

    /// Switch the active icon set.
    ///
    /// # Errors
    /// - [`RegistryError::UnknownSet`] if the id was never registered.
    pub fn set_active(&mut self, id: IconSetId) -> Result<(), RegistryError> {
        if !self.sets.contains_key(&id) {
            return Err(RegistryError::UnknownSet(id));
        }
        self.active = Some(id);
        Ok(())
    }

    /// Currently-active set id, if any.
    pub fn active(&self) -> Option<&IconSetId> {
        self.active.as_ref()
    }

    /// Iterate over registered set ids.
    pub fn sets(&self) -> impl Iterator<Item = &IconSetId> {
        self.sets.keys()
    }

    /// Look up an icon name in the active set, returning a handle if
    /// the icon exists. Names are typed-validated by [`IconName::new`]
    /// before lookup.
    ///
    /// Returns [`None`] if no active set is configured, the name is
    /// invalid, or the icon does not exist.
    pub fn lookup(&self, name: &str) -> Option<IconHandle> {
        let active = self.active.as_ref()?;
        let icon_name = IconName::new(name).ok()?;
        let state = self.sets.get(active)?;
        if state.loaded.entries.contains_key(&icon_name) {
            Some(IconHandle::new(active.clone(), icon_name))
        } else {
            None
        }
    }

    /// Look up a name in a *specific* set (does not consult `active`).
    pub fn lookup_in(&self, set: &IconSetId, name: &str) -> Option<IconHandle> {
        let icon_name = IconName::new(name).ok()?;
        let state = self.sets.get(set)?;
        if state.loaded.entries.contains_key(&icon_name) {
            Some(IconHandle::new(set.clone(), icon_name))
        } else {
            None
        }
    }

    /// Resolve a handle to raw SVG source.
    ///
    /// Reads from disk on first hit; subsequent calls return a cached
    /// reference. The cache is invalidated by [`Self::reload_set`].
    ///
    /// # Errors
    /// - [`RegistryError::UnknownSet`] / [`RegistryError::UnknownIcon`]
    ///   if the handle does not match the registry.
    /// - [`RegistryError::Io`] if the SVG file can't be read.
    pub fn svg_bytes(&mut self, handle: &IconHandle) -> Result<&str, RegistryError> {
        let state = self
            .sets
            .get_mut(&handle.set)
            .ok_or_else(|| RegistryError::UnknownSet(handle.set.clone()))?;
        if !state.cache.contains_key(&handle.name) {
            let path = state
                .loaded
                .entries
                .get(&handle.name)
                .ok_or_else(|| RegistryError::UnknownIcon {
                    set: handle.set.clone(),
                    name: handle.name.clone(),
                })?
                .clone();
            let bytes = std::fs::read_to_string(&path).map_err(|source| RegistryError::Io {
                path: path.clone(),
                source,
            })?;
            state.cache.insert(handle.name.clone(), bytes);
        }
        Ok(state.cache.get(&handle.name).unwrap().as_str())
    }

    /// Reload a set from its manifest, invalidating the SVG cache.
    /// Returns the wall-clock duration of the reload, useful for the
    /// `<50ms` SLO test.
    ///
    /// # Errors
    /// - [`RegistryError::UnknownSet`] if the id was never registered
    ///   from a file (manually-registered sets don't have a manifest
    ///   path and cannot be reloaded).
    /// - [`RegistryError::Loader`] if re-parsing fails.
    pub fn reload_set(&mut self, id: &IconSetId) -> Result<std::time::Duration, RegistryError> {
        let state = self
            .sets
            .get(id)
            .ok_or_else(|| RegistryError::UnknownSet(id.clone()))?;
        let manifest_path = state
            .manifest_path
            .clone()
            .ok_or_else(|| RegistryError::UnknownSet(id.clone()))?;
        let start = Instant::now();
        let loaded = loader::load_manifest(&manifest_path)?;
        let new_state = SetState {
            loaded,
            cache: BTreeMap::new(),
            manifest_path: Some(manifest_path),
        };
        self.sets.insert(id.clone(), new_state);
        Ok(start.elapsed())
    }

    /// Borrow the loaded set metadata (license / attribution / etc.).
    pub fn set_info(&self, id: &IconSetId) -> Option<&LoadedIconSet> {
        self.sets.get(id).map(|s| &s.loaded)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::loader::LoadedIconSet;

    fn make_set(id: &str, names: &[&str]) -> LoadedIconSet {
        let mut entries = BTreeMap::new();
        for n in names {
            entries.insert(
                IconName::new(*n).unwrap(),
                PathBuf::from(format!("{n}.svg")),
            );
        }
        LoadedIconSet {
            id: IconSetId::new(id).unwrap(),
            license: "MIT".into(),
            attribution: "stub".into(),
            entries,
        }
    }

    #[test]
    fn first_registered_becomes_active() {
        let mut r = IconRegistry::new();
        r.register_loaded(make_set("a", &["x"]));
        assert_eq!(r.active().map(IconSetId::as_str), Some("a"));
    }

    #[test]
    fn lookup_returns_handle_when_present() {
        let mut r = IconRegistry::new();
        r.register_loaded(make_set("lucide", &["folder-open", "save"]));
        let h = r.lookup("folder-open").expect("present");
        assert_eq!(h.set.as_str(), "lucide");
        assert_eq!(h.name.as_str(), "folder-open");
    }

    #[test]
    fn lookup_misses_return_none() {
        let mut r = IconRegistry::new();
        r.register_loaded(make_set("lucide", &["save"]));
        assert!(r.lookup("nonexistent").is_none());
        assert!(r.lookup("bad name").is_none()); // invalid id
    }

    #[test]
    fn set_active_swaps() {
        let mut r = IconRegistry::new();
        r.register_loaded(make_set("a", &["x"]));
        r.register_loaded(make_set("b", &["y"]));
        assert_eq!(r.active().map(IconSetId::as_str), Some("a"));
        let b = IconSetId::new("b").unwrap();
        r.set_active(b).unwrap();
        assert_eq!(r.active().map(IconSetId::as_str), Some("b"));
        assert!(r.lookup("y").is_some());
        assert!(r.lookup("x").is_none());
    }

    #[test]
    fn set_active_unknown_errors() {
        let mut r = IconRegistry::new();
        let nope = IconSetId::new("nope").unwrap();
        let err = r.set_active(nope).unwrap_err();
        assert!(matches!(err, RegistryError::UnknownSet(_)));
    }
}
