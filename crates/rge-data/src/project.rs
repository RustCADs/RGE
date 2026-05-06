// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for the
//                                                                  `.rge-project`
//                                                                  schema.
//
//! [`Project`] — top-level schema for `.rge-project` files.
//!
//! Per `PLAN.md` §1.6.6 (project layout) every game project on disk has one
//! `.rge-project` at the root, alongside its `assets/`, `scenes/`,
//! `prefabs/`, `materials/`, `scripts/`, `plugins/`, and `target/cook/`
//! subtrees. The project file carries the metadata an editor / tooling
//! layer needs to load the rest:
//!
//! - which scenes ship with the project,
//! - which plugins are required,
//! - which target tiers the project supports (per `PLAN.md` §0.3.1
//!   execution domains and §1.6.4 cooked binary).
//!
//! All component-specific reflection lives in `kernel/types`; this crate
//! holds only the **container shape**.

use serde::{Deserialize, Serialize};

use crate::schema_version::SchemaVersion;

/// Top-level `.rge-project` schema. See module docs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// File-format schema version; loader uses this to drive migrations.
    pub version: SchemaVersion,
    /// Display name (UI title bar, marketplace listing).
    pub name: String,
    /// Free-form description (marketplace, README seed). May be empty.
    pub description: String,
    /// Tier-0/1/2/3 platform targets this project ships to.
    /// See `PLAN.md` §0.4 (floor/reach product) and §1.6.4 (cooked binary).
    pub target_tiers: Vec<TargetTier>,
    /// Plugin manifest references the project depends on.
    pub plugins: Vec<PluginRef>,
    /// Relative paths (project-root-relative) to every `.rge-scene` shipped
    /// with the project. The editor loads the first entry as the start
    /// scene unless overridden by user prefs.
    pub scenes: Vec<ScenePath>,
}

impl Project {
    /// Construct an empty project at the supplied schema version.
    #[must_use]
    pub fn empty(name: impl Into<String>, version: SchemaVersion) -> Self {
        Self {
            version,
            name: name.into(),
            description: String::new(),
            target_tiers: Vec::new(),
            plugins: Vec::new(),
            scenes: Vec::new(),
        }
    }
}

/// Platform-tier targeting flag. Mirrors `PLAN.md` §0.1 (four pillars,
/// tier-0/1/2 platform list).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetTier {
    /// Native desktop (Windows / macOS / Linux). Tier-0.
    Desktop,
    /// iOS / Android. Tier-1.
    Mobile,
    /// Browser / WASM. Tier-1.
    Web,
    /// Headless (CI, dedicated server). Tier-2.
    Headless,
}

/// Reference to a plugin the project requires. Resolution / signing /
/// capability gating happens in `crates/marketplace`; this struct is the
/// on-disk pointer only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginRef {
    /// Canonical plugin id (`<author>/<plugin>` in the marketplace).
    pub id: String,
    /// Required version range, SemVer-shaped. v0 stores the literal string
    /// from the manifest; full range parsing belongs to the marketplace
    /// crate.
    pub version_req: String,
}

/// Relative path to a `.rge-scene` file under the project root.
///
/// Stored as a single transparent string so the on-disk RON looks like
/// `"scenes/main-menu.rge-scene"` rather than `(path: "...")`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScenePath(pub String);

impl ScenePath {
    /// Borrow the path string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_project() -> Project {
        Project {
            version: SchemaVersion::V0_1_0,
            name: "demo".into(),
            description: "demo project".into(),
            target_tiers: vec![TargetTier::Desktop, TargetTier::Headless],
            plugins: vec![PluginRef {
                id: "rge/standard-light".into(),
                version_req: "0.1".into(),
            }],
            scenes: vec![
                ScenePath("scenes/main-menu.rge-scene".into()),
                ScenePath("scenes/level-1.rge-scene".into()),
            ],
        }
    }

    #[test]
    fn round_trip_ron() {
        let p = fixture_project();
        let s = ron::ser::to_string_pretty(&p, ron::ser::PrettyConfig::default()).expect("ser");
        let back: Project = ron::from_str(&s).expect("de");
        assert_eq!(p, back);
    }

    #[test]
    fn empty_constructor() {
        let p = Project::empty("demo", SchemaVersion::V0_0_0);
        assert_eq!(p.name, "demo");
        assert_eq!(p.version, SchemaVersion::V0_0_0);
        assert!(p.scenes.is_empty());
    }

    #[test]
    fn target_tier_serializes_lowercase() {
        let s = ron::to_string(&TargetTier::Desktop).expect("ser");
        assert_eq!(s, "desktop");
    }
}
