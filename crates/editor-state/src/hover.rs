//! `editor_state::hover` — per-panel hover state.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15).

use std::collections::BTreeMap;

use rge_kernel_ecs::EntityId;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PanelId
// ---------------------------------------------------------------------------

/// Stable identifier for an editor panel. Uses a static string slug
/// (e.g., `"scene-tree"`, `"inspector"`, `"viewport"`). Future migration to a
/// numeric handle is a Phase 6 concern.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PanelId(pub String);

impl PanelId {
    /// Construct a [`PanelId`] from any string-like value.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Return the panel slug as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// EntityId serde helpers (same pattern as selection.rs)
//
// `rge-kernel-ecs` does not enable `ulid`'s optional `serde` feature.
// We serialise through `ulid::Ulid` (which picks up serde via `ulid/serde`
// enabled in this crate's Cargo.toml) and reconstruct via `EntityId::from_ulid`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct EntityIdSerde(ulid::Ulid);

impl From<EntityId> for EntityIdSerde {
    fn from(id: EntityId) -> Self {
        Self(id.ulid())
    }
}

impl From<EntityIdSerde> for EntityId {
    fn from(s: EntityIdSerde) -> Self {
        EntityId::from_ulid(s.0)
    }
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// Hover state across editor panels. Each panel stores at most one hovered
/// entity (or `None`). [`BTreeMap`] for deterministic iteration.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Hover {
    panels: BTreeMap<PanelId, EntityId>,
}

// Manual Serialize/Deserialize: round-trip via BTreeMap<PanelId, EntityIdSerde>.
impl Serialize for Hover {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let map: BTreeMap<&PanelId, EntityIdSerde> = self
            .panels
            .iter()
            .map(|(k, v)| (k, EntityIdSerde::from(*v)))
            .collect();
        map.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Hover {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let map = BTreeMap::<PanelId, EntityIdSerde>::deserialize(d)?;
        Ok(Self {
            panels: map
                .into_iter()
                .map(|(k, v)| (k, EntityId::from(v)))
                .collect(),
        })
    }
}

impl Hover {
    /// Construct an empty hover state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the hovered entity for `panel`. Replaces any previous hover.
    pub fn set(&mut self, panel: PanelId, entity: EntityId) {
        self.panels.insert(panel, entity);
    }

    /// Clear hover for `panel` (no-op when panel has no hover).
    pub fn clear(&mut self, panel: &PanelId) {
        self.panels.remove(panel);
    }

    /// Clear hover for all panels.
    pub fn clear_all(&mut self) {
        self.panels.clear();
    }

    /// Return the hovered entity for `panel`, or `None`.
    #[must_use]
    pub fn get(&self, panel: &PanelId) -> Option<EntityId> {
        self.panels.get(panel).copied()
    }

    /// Iterate `(panel, entity)` pairs in deterministic (panel-slug) order.
    pub fn iter(&self) -> impl Iterator<Item = (&PanelId, EntityId)> {
        self.panels.iter().map(|(k, v)| (k, *v))
    }

    /// Number of panels with an active hover.
    #[must_use]
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    /// True when no panel has an active hover.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(s: &str) -> PanelId {
        PanelId::new(s)
    }

    fn eid() -> EntityId {
        EntityId::new()
    }

    #[test]
    fn set_and_get_per_panel() {
        let mut h = Hover::new();
        let p = panel("viewport");
        let e = eid();
        h.set(p.clone(), e);
        assert_eq!(h.get(&p), Some(e));
    }

    #[test]
    fn get_absent_panel_is_none() {
        let h = Hover::new();
        assert_eq!(h.get(&panel("inspector")), None);
    }

    #[test]
    fn clear_removes_panel_hover() {
        let mut h = Hover::new();
        let p = panel("scene-tree");
        let e = eid();
        h.set(p.clone(), e);
        h.clear(&p);
        assert_eq!(h.get(&p), None);
    }

    #[test]
    fn clear_all_empties_everything() {
        let mut h = Hover::new();
        h.set(panel("a"), eid());
        h.set(panel("b"), eid());
        h.set(panel("c"), eid());
        assert_eq!(h.len(), 3);
        h.clear_all();
        assert!(h.is_empty());
    }

    #[test]
    fn iter_is_deterministic_by_panel_slug() {
        let mut h = Hover::new();
        // Insert out of lexicographic order.
        h.set(panel("z-panel"), eid());
        h.set(panel("a-panel"), eid());
        h.set(panel("m-panel"), eid());
        let keys: Vec<&str> = h.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(keys, ["a-panel", "m-panel", "z-panel"]);
    }

    #[test]
    fn set_replaces_previous_hover() {
        let mut h = Hover::new();
        let p = panel("viewport");
        let e1 = eid();
        let e2 = eid();
        h.set(p.clone(), e1);
        h.set(p.clone(), e2);
        assert_eq!(h.get(&p), Some(e2));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn panel_id_display() {
        let p = PanelId::new("inspector");
        assert_eq!(p.to_string(), "inspector");
    }

    #[test]
    fn panel_id_as_str() {
        let p = PanelId::new("scene-tree");
        assert_eq!(p.as_str(), "scene-tree");
    }

    #[test]
    fn default_is_empty() {
        let h = Hover::default();
        assert!(h.is_empty());
    }
}
