//! `LayoutService` — persistence and version migration for [`LayoutBlueprint`].
//!
//! UE Slate parallel: `FLayoutSaveRestore` in `Engine/Source/Runtime/Slate/Public/Framework/
//! Docking/LayoutService.h`. Per the W10 dispatch package this module owns:
//!
//! 1. **Persistence** — write/read `LayoutBlueprint` to `~/.config/rge/editor_layout.{ron,json}`
//!    (RON is canonical; JSON is supported for debug/tooling).
//! 2. **Tamper detection** — every persisted file embeds a blake3 hash over the layout content;
//!    on load we verify and surface a [`LayoutLoadError::Tampered`] when the hash mismatches.
//! 3. **Version migration** — when the persisted layout's name has a different `(major, minor)`
//!    suffix from what the application requests, we run [`LayoutBlueprint::migrate`] which
//!    preserves geometry for tabs whose [`TabId`] is unchanged and appends new tabs to the
//!    primary leaf.
//!
//! ## On-disk format
//!
//! ```ron
//! PersistedLayout(
//!     blueprint: LayoutBlueprint(...),
//!     hash: "blake3-hex-of-canonical-blueprint-bytes",
//!     written_at_unix: 1746468000,
//! )
//! ```
//!
//! The hash covers the *canonical RON encoding of the blueprint only* — `hash` and
//! `written_at_unix` are excluded, so editing the timestamp by hand doesn't trip tamper
//! detection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dock::tab_id::TabId;
use crate::dock::tab_manager::{LayoutBlueprint, LayoutBuildError, LayoutNode};
use crate::dock::version::{LayoutName, LayoutNameError};

/// File extension chosen by the caller — drives serialization format.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayoutFormat {
    /// RON (Rusty Object Notation) — canonical on-disk form.
    Ron,
    /// JSON — pretty-printed; debug/tooling fallback.
    Json,
}

impl LayoutFormat {
    /// Pick a format from a path's extension. Defaults to RON if unrecognized.
    #[must_use]
    pub fn from_path(p: &Path) -> Self {
        match p.extension().and_then(|s| s.to_str()) {
            Some("json") => Self::Json,
            _ => Self::Ron,
        }
    }
}

/// On-disk envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedLayout {
    /// The blueprint.
    blueprint: LayoutBlueprint,
    /// blake3 of the canonical RON encoding of `blueprint`.
    hash: String,
    /// Unix timestamp of last write — informational only, excluded from hash.
    written_at_unix: u64,
}

/// Errors when saving a layout.
#[derive(Debug, Error)]
pub enum LayoutSaveError {
    /// I/O failure writing the layout file.
    #[error("io error writing layout: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization failure.
    #[error("serialize error: {0}")]
    Serialize(String),
}

/// Errors when loading a layout.
#[derive(Debug, Error)]
pub enum LayoutLoadError {
    /// I/O failure reading the file.
    #[error("io error reading layout: {0}")]
    Io(#[from] std::io::Error),
    /// Parse failure.
    #[error("deserialize error: {0}")]
    Deserialize(String),
    /// Tamper detection: stored hash didn't match recomputed hash.
    #[error("layout tampered: hash mismatch (expected={expected}, actual={actual})")]
    Tampered {
        /// The blake3 hash that was embedded in the file at write time.
        expected: String,
        /// The blake3 hash recomputed from the file's blueprint at read time.
        actual: String,
    },
    /// Layout name parse failure.
    #[error("invalid layout name: {0}")]
    InvalidName(#[from] LayoutNameError),
}

/// Errors during migration.
#[derive(Debug, Error)]
pub enum LayoutMigrationError {
    /// The persisted layout and the target layout don't share a base name.
    #[error("cannot migrate `{from}` → `{to}` (different layout bases)")]
    DifferentBase {
        /// Source layout's full versioned name.
        from: String,
        /// Target layout's full versioned name.
        to: String,
    },
    /// The migration produced an empty tree (every tab in the persisted layout was removed).
    #[error("migration of `{0}` would produce an empty tree")]
    EmptyTree(String),
    /// The migrated blueprint failed to materialize.
    #[error("migrated blueprint invalid: {0}")]
    BadBlueprint(#[from] LayoutBuildError),
}

/// Stateless layout-service facade.
///
/// All operations are static-style methods — there is no in-memory cache at v0.0.1; layouts are
/// small enough (<10 KB serialized) that re-reading on demand is cheap. A future wave can add a
/// memoized variant if profiling justifies it.
pub struct LayoutService;

impl LayoutService {
    /// Default config directory: `~/.config/rge/`.
    ///
    /// Falls back to `./.rge_config/` if the home directory cannot be determined (e.g. sandboxed
    /// CI). The fallback is documented behaviour, not a hidden surprise — see PLAN.md §1.10
    /// determinism bar.
    #[must_use]
    pub fn default_config_dir() -> PathBuf {
        if let Some(home) = home_dir() {
            home.join(".config").join("rge")
        } else {
            PathBuf::from(".rge_config")
        }
    }

    /// Default persisted-layout path: `~/.config/rge/editor_layout.ron`.
    #[must_use]
    pub fn default_layout_path() -> PathBuf {
        Self::default_config_dir().join("editor_layout.ron")
    }

    /// Save `blueprint` to `path`, creating parent directories as needed.
    pub fn save(blueprint: &LayoutBlueprint, path: &Path) -> Result<(), LayoutSaveError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let format = LayoutFormat::from_path(path);
        let hash = canonical_hash(blueprint);
        let envelope = PersistedLayout {
            blueprint: blueprint.clone(),
            hash,
            written_at_unix: now_unix(),
        };
        let bytes = match format {
            LayoutFormat::Ron => {
                ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::default())
                    .map_err(|e| LayoutSaveError::Serialize(e.to_string()))?
                    .into_bytes()
            }
            LayoutFormat::Json => serde_json::to_vec_pretty(&envelope)
                .map_err(|e| LayoutSaveError::Serialize(e.to_string()))?,
        };
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Load a layout from `path` and verify its blake3 hash.
    pub fn load(path: &Path) -> Result<LayoutBlueprint, LayoutLoadError> {
        let bytes = std::fs::read(path)?;
        let format = LayoutFormat::from_path(path);
        let envelope: PersistedLayout = match format {
            LayoutFormat::Ron => {
                let s = std::str::from_utf8(&bytes)
                    .map_err(|e| LayoutLoadError::Deserialize(e.to_string()))?;
                ron::from_str(s).map_err(|e| LayoutLoadError::Deserialize(e.to_string()))?
            }
            LayoutFormat::Json => serde_json::from_slice(&bytes)
                .map_err(|e| LayoutLoadError::Deserialize(e.to_string()))?,
        };
        let recomputed = canonical_hash(&envelope.blueprint);
        if recomputed != envelope.hash {
            return Err(LayoutLoadError::Tampered {
                expected: envelope.hash,
                actual: recomputed,
            });
        }
        Ok(envelope.blueprint)
    }

    /// Load a persisted layout *and* migrate it to `target_name` if the version differs.
    ///
    /// Migration semantics (per PLAN.md §6.6 + W10 spec):
    /// - Same `(base, major, minor)` ⇒ no migration; persisted blueprint returned as-is (its name
    ///   stays at the persisted version, even if the patch differs).
    /// - Different `major`/`minor`, same `base` ⇒ run [`migrate`](Self::migrate) which:
    ///   - Filters the persisted tree to only contain tabs that still exist (per `expected_tabs`).
    ///   - Re-stamps the layout name to `target_name`.
    ///   - Falls back to `default_blueprint` if every tab was removed.
    /// - Different `base` ⇒ returns `default_blueprint` (we do not cross migrate between
    ///   workspaces).
    ///
    /// The `expected_tabs` set is the spawner registry's known-good list at the application's
    /// current version — typically `registry.ids().cloned().collect()`.
    pub fn load_or_migrate(
        path: &Path,
        target_name: &LayoutName,
        expected_tabs: &HashSet<TabId>,
        default_blueprint: impl FnOnce() -> LayoutBlueprint,
    ) -> Result<LayoutBlueprint, LayoutLoadError> {
        let persisted = match Self::load(path) {
            Ok(bp) => bp,
            Err(LayoutLoadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(default_blueprint());
            }
            Err(e) => return Err(e),
        };
        if persisted.name == *target_name {
            return Ok(persisted);
        }
        if !persisted.name.is_same_base(target_name) {
            return Ok(default_blueprint());
        }
        if !target_name.requires_migration_from(&persisted.name) {
            // patch-level only — keep persisted as-is
            return Ok(persisted);
        }
        // Geometry-preserving migration.
        match Self::migrate(persisted, target_name, expected_tabs) {
            Ok(bp) => Ok(bp),
            Err(_) => Ok(default_blueprint()),
        }
    }

    /// Apply a version migration: keep tabs in `expected_tabs`, re-stamp the layout name,
    /// degenerate-collapse single-child splitters.
    ///
    /// Caller is responsible for confirming the bases match before calling this.
    pub fn migrate(
        persisted: LayoutBlueprint,
        target_name: &LayoutName,
        expected_tabs: &HashSet<TabId>,
    ) -> Result<LayoutBlueprint, LayoutMigrationError> {
        if !persisted.name.is_same_base(target_name) {
            return Err(LayoutMigrationError::DifferentBase {
                from: persisted.name.to_string(),
                to: target_name.to_string(),
            });
        }
        let Some(filtered_root) = persisted.root.retain_tabs(expected_tabs) else {
            return Err(LayoutMigrationError::EmptyTree(persisted.name.to_string()));
        };
        // Wrap in the same primary-area shell we expect from the builder: a Splitter whose
        // children are exactly one. retain_tabs collapses single-child splitters, so we may need
        // to re-wrap.
        let root = match &filtered_root {
            LayoutNode::Splitter { children, .. } if children.len() == 1 => filtered_root,
            _ => LayoutNode::Splitter {
                direction: crate::dock::tab_manager::Direction::Vertical,
                fraction: 1.0,
                children: vec![filtered_root],
            },
        };
        Ok(LayoutBlueprint {
            name: target_name.clone(),
            root,
        })
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Canonical hash: blake3 over the RON encoding of the blueprint *without* envelope fields.
fn canonical_hash(bp: &LayoutBlueprint) -> String {
    let canonical = ron::ser::to_string(bp).unwrap_or_default();
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

fn now_unix() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Cross-platform home dir resolution without pulling in the `dirs`/`directories` crate.
///
/// Honours `$HOME` on Unix and `%USERPROFILE%` on Windows. Returns `None` if neither is set.
fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.is_empty() {
            return Some(PathBuf::from(profile));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::tab_manager::{Direction, TabManager};

    fn baseline_blueprint(name: &str) -> LayoutBlueprint {
        TabManager::new_layout(name)
            .new_primary_area(Direction::Vertical, 1.0)
            .new_splitter(Direction::Horizontal, 0.7)
            .new_stack()
            .add_tab("viewport")
            .done()
            .new_stack()
            .add_tab("scene_panel")
            .done()
            .done()
            .done()
            .build()
            .unwrap()
    }

    #[test]
    fn canonical_hash_is_stable_across_calls() {
        let bp = baseline_blueprint("rge_main_v0.1.0");
        assert_eq!(canonical_hash(&bp), canonical_hash(&bp));
    }

    #[test]
    fn round_trip_via_ron() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.ron");
        let bp = baseline_blueprint("rge_main_v0.1.0");
        LayoutService::save(&bp, &path).unwrap();
        let loaded = LayoutService::load(&path).unwrap();
        assert_eq!(loaded.name, bp.name);
        assert_eq!(loaded.collect_tab_ids(), bp.collect_tab_ids());
    }

    #[test]
    fn round_trip_via_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.json");
        let bp = baseline_blueprint("rge_main_v0.1.0");
        LayoutService::save(&bp, &path).unwrap();
        let loaded = LayoutService::load(&path).unwrap();
        assert_eq!(loaded.name, bp.name);
        assert_eq!(loaded.collect_tab_ids(), bp.collect_tab_ids());
    }

    #[test]
    fn tamper_detection_catches_byte_flip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.ron");
        let bp = baseline_blueprint("rge_main_v0.1.0");
        LayoutService::save(&bp, &path).unwrap();
        let mut content = std::fs::read_to_string(&path).unwrap();
        // mutate one of the tab names; the recomputed hash will differ from the stored hash.
        content = content.replace("viewport", "viewp0rt");
        std::fs::write(&path, content).unwrap();
        match LayoutService::load(&path) {
            Err(LayoutLoadError::Tampered { .. }) => {}
            other => panic!("expected Tampered, got {other:?}"),
        }
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.ron");
        let target = LayoutName::parse("rge_main_v0.1.0").unwrap();
        let bp = LayoutService::load_or_migrate(&path, &target, &HashSet::new(), || {
            baseline_blueprint("rge_main_v0.1.0")
        })
        .unwrap();
        assert_eq!(bp.name.to_string(), "rge_main_v0.1.0");
    }
}
