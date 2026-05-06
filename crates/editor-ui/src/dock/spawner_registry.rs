//! `SpawnerRegistry` — `TabId` → factory closure map.
//!
//! UE Slate parallel: `FGlobalTabmanager::RegisterNomadTabSpawner` and the broader
//! `FTabManager::RegisterTabSpawner` family in `Engine/Source/Runtime/Slate/Public/Framework/
//! Docking/TabManager.h`. Each registered spawner is a closure of type
//! `Fn(&TabId) -> TabBody` that produces the *content* of a tab when the layout-service materializes
//! a [`LayoutBlueprint`].
//!
//! Per the W10 dispatch package, `register_default_spawners(&mut SpawnerRegistry)` populates the
//! registry with the built-in tab IDs:
//!
//! - `scene_panel`
//! - `hierarchy`
//! - `viewport`
//! - `property_panel`
//! - `asset_browser`
//! - `console`
//! - `log`
//!
//! Bodies returned by the default spawners at v0.0.1 are *placeholder* [`PlaceholderTabBody`]
//! values — the actual viewport/scene/etc. widgets are owned by other crates and other waves.
//! The registry is generic over the body type, so when those waves land they can plug their
//! real types in by re-registering with their own factory closures.

use std::collections::HashMap;

use crate::dock::tab_id::TabId;

/// Factory closure: produces a `TabBody` instance given a [`TabId`].
///
/// Closures must be `Send + Sync + 'static` so the registry can be shared across the editor's
/// scheduler threads (per PLAN.md §8.1 schedule-stage isolation).
pub type Spawner<TabBody> = Box<dyn Fn(&TabId) -> TabBody + Send + Sync + 'static>;

/// Map from [`TabId`] → factory closure.
///
/// Generic over `TabBody` so the same registry shape works for both v0.0.1 placeholder bodies
/// and the real per-tab widgets that will land in later waves.
pub struct SpawnerRegistry<TabBody> {
    spawners: HashMap<TabId, Spawner<TabBody>>,
}

impl<TabBody> Default for SpawnerRegistry<TabBody> {
    fn default() -> Self {
        Self {
            spawners: HashMap::new(),
        }
    }
}

impl<TabBody> SpawnerRegistry<TabBody> {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the spawner for `id`.
    ///
    /// Returns `true` if a previous spawner under this id was replaced — useful for plugins that
    /// override built-in tabs.
    pub fn register<F>(&mut self, id: impl Into<TabId>, factory: F) -> bool
    where
        F: Fn(&TabId) -> TabBody + Send + Sync + 'static,
    {
        self.spawners.insert(id.into(), Box::new(factory)).is_some()
    }

    /// Remove a spawner. Returns `true` if there was one to remove.
    pub fn unregister(&mut self, id: &TabId) -> bool {
        self.spawners.remove(id).is_some()
    }

    /// Look up the factory for `id` and invoke it. Returns `None` if no spawner is registered.
    #[must_use]
    pub fn spawn(&self, id: &TabId) -> Option<TabBody> {
        self.spawners.get(id).map(|f| f(id))
    }

    /// True if a spawner is registered for `id`.
    #[must_use]
    pub fn contains(&self, id: &TabId) -> bool {
        self.spawners.contains_key(id)
    }

    /// Number of registered spawners.
    #[must_use]
    pub fn len(&self) -> usize {
        self.spawners.len()
    }

    /// True if no spawners are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spawners.is_empty()
    }

    /// All registered tab ids, unordered.
    pub fn ids(&self) -> impl Iterator<Item = &TabId> {
        self.spawners.keys()
    }
}

// =============================================================================
// Default spawner pack
// =============================================================================

/// The canonical set of built-in tab IDs documented in W10. Exposed as a constant so version-
/// migration code can compute "is this tab one of ours?" without re-string-typing the names.
pub const DEFAULT_TAB_IDS: &[&str] = &[
    "scene_panel",
    "hierarchy",
    "viewport",
    "property_panel",
    "asset_browser",
    "console",
    "log",
];

/// Placeholder tab-body for v0.0.1.
///
/// Other waves will replace this with their real widget types via [`SpawnerRegistry::register`].
/// We keep the shape intentionally minimal: just enough to verify in tests that the spawner
/// produced *something* and that the something is correctly tagged with its source [`TabId`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceholderTabBody {
    /// The id this body was spawned for.
    pub source: TabId,
    /// Display title (defaults to the id, but plugins/tests can override).
    pub title: String,
}

impl PlaceholderTabBody {
    fn for_id(id: &TabId) -> Self {
        Self {
            source: id.clone(),
            title: prettify_id(id.as_str()),
        }
    }
}

/// Convert `snake_case_ish_name` to `Snake Case Ish Name`.
fn prettify_id(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            out.push(' ');
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Populate `registry` with the built-in tab spawners enumerated in [`DEFAULT_TAB_IDS`].
///
/// All default spawners produce a [`PlaceholderTabBody`] tagged with the requesting id; the
/// registry is generic, so callers using a different body type need to register their own
/// factories — this convenience function only applies when `TabBody = PlaceholderTabBody`.
pub fn register_default_spawners(registry: &mut SpawnerRegistry<PlaceholderTabBody>) {
    for id_str in DEFAULT_TAB_IDS {
        let _ = registry.register(*id_str, PlaceholderTabBody::for_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_and_spawns_default_tabs() {
        let mut reg: SpawnerRegistry<PlaceholderTabBody> = SpawnerRegistry::new();
        register_default_spawners(&mut reg);
        assert_eq!(reg.len(), DEFAULT_TAB_IDS.len());
        for id_str in DEFAULT_TAB_IDS {
            let id = TabId::new(*id_str);
            assert!(reg.contains(&id));
            let body = reg.spawn(&id).expect("spawn");
            assert_eq!(body.source, id);
        }
    }

    #[test]
    fn returns_none_for_unknown_id() {
        let reg: SpawnerRegistry<PlaceholderTabBody> = SpawnerRegistry::new();
        assert!(reg.spawn(&TabId::new("nonexistent")).is_none());
    }

    #[test]
    fn replace_returns_true() {
        let mut reg: SpawnerRegistry<u32> = SpawnerRegistry::new();
        assert!(!reg.register("foo", |_| 1));
        assert!(reg.register("foo", |_| 2));
        assert_eq!(reg.spawn(&TabId::new("foo")), Some(2));
    }

    #[test]
    fn prettify_basic_cases() {
        assert_eq!(prettify_id("scene_panel"), "Scene Panel");
        assert_eq!(prettify_id("log"), "Log");
        assert_eq!(prettify_id(""), "");
    }

    #[test]
    fn unregister_removes_entry() {
        let mut reg: SpawnerRegistry<PlaceholderTabBody> = SpawnerRegistry::new();
        register_default_spawners(&mut reg);
        let id = TabId::new("console");
        assert!(reg.unregister(&id));
        assert!(!reg.contains(&id));
        assert!(!reg.unregister(&id));
    }
}
