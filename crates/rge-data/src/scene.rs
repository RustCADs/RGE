// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for the
//                                                                  `.rge-scene`
//                                                                  schema.
//
//! [`Scene`] — top-level schema for `.rge-scene` files.
//!
//! Per `PLAN.md` §1.6.6 a scene is the unit of authoring inside a project.
//! It carries:
//!
//! - a list of [`Entity`] records — every concrete game object,
//! - a list of root-entity [`EntityId`]s — entries with no parent,
//! - components and relations on each entity, stored in a reflection-
//!   neutral [`ComponentValue`] / [`Relation`] envelope so this crate has
//!   **no** dependency on `kernel/types::Reflect` (deferred to W02).
//!
//! When the W02 reflection wave merges, callers of this crate will:
//!
//! 1. parse a `.rge-scene` into the structs here,
//! 2. walk each [`ComponentValue::data`] string through
//!    `kernel/types::serde_bridge::from_ron` to obtain a strongly typed
//!    `dyn Reflect` instance keyed by [`ComponentValue::type_id`].
//!
//! That two-phase split is exactly what `kernel/types` already exposes;
//! see `crates/rge-data/src/lib.rs` for the local `Reflect` stub.

use serde::{Deserialize, Serialize};

use crate::entity_ref::EntityId;
use crate::schema_version::SchemaVersion;

/// Top-level `.rge-scene` schema. See module docs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// File-format schema version. Drives migrations on load.
    pub version: SchemaVersion,
    /// Display name (used by editor tab labels and diagnostics).
    pub name: String,
    /// Every entity in the scene. Order is significant only for cook
    /// determinism; runtime systems don't index into this list.
    pub entities: Vec<Entity>,
    /// IDs of entities with no [`Relation::ChildOf`] parent. Roots are
    /// the entry points the renderer / scripting iteration walks.
    pub root_entities: Vec<EntityId>,
}

impl Scene {
    /// Empty scene at the given schema version.
    #[must_use]
    pub fn empty(name: impl Into<String>, version: SchemaVersion) -> Self {
        Self {
            version,
            name: name.into(),
            entities: Vec::new(),
            root_entities: Vec::new(),
        }
    }

    /// Find the [`Entity`] with the given id, or `None`. Linear scan — fine
    /// for editor-time lookups; runtime uses `kernel/ecs` storage.
    #[must_use]
    pub fn find_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }
}

/// One concrete entity. State-only — all behaviour belongs to systems.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Scene-stable identity (ULID).
    pub id: EntityId,
    /// Display name (inspector label, diagnostic spans). May be empty.
    pub name: String,
    /// Component values attached to this entity. Each is a reflection-
    /// neutral envelope; see [`ComponentValue`].
    pub components: Vec<ComponentValue>,
    /// ECS relations with other entities (`ChildOf`, `LinkedTo`, …). The
    /// runtime resolves these into `kernel/ecs::TreeRelationStorage` and
    /// friends; this struct is just the source-of-truth wire shape.
    pub relations: Vec<Relation>,
}

/// Reflection-neutral component envelope.
///
/// `type_id` names the component type (`"rge::components::Transform"`,
/// `"rge::components::MeshRenderer"`, …); `data` is the RON literal that
/// would deserialize into that type. Two-phase deserialize: parse the
/// scene first, then walk `data` through reflection-aware bridging.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentValue {
    /// Canonical type path (matches `kernel/types::TypeId::path`).
    pub type_id: String,
    /// RON literal that serializes the component's payload.
    pub data: String,
}

/// ECS relation between entities.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Relation {
    /// Hierarchical parenting — `self` is a child of `parent`. Per
    /// `PLAN.md` §1.5.1 every renderable entity participates in the
    /// scene tree via this relation.
    ChildOf {
        /// Parent entity id.
        parent: EntityId,
    },
    /// Generic typed link — used by gameplay-domain plugins. The
    /// `kind` string is interned at runtime; this crate only stores it.
    LinkedTo {
        /// Marker for the link semantics (`"focuses"`, `"locks_to"`, …).
        kind: String,
        /// Target entity id.
        target: EntityId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_scene() -> Scene {
        let parent = EntityId::from_u128(0x0000_0000_0000_0001_0000_0000_0000_0000_u128);
        let child = EntityId::from_u128(0x0000_0000_0000_0002_0000_0000_0000_0000_u128);

        Scene {
            version: SchemaVersion::V0_1_0,
            name: "main-menu".into(),
            entities: vec![
                Entity {
                    id: parent,
                    name: "Camera".into(),
                    components: vec![ComponentValue {
                        type_id: "rge::components::Transform".into(),
                        data: "(translation:(0.0,0.0,5.0), rotation:(0.0,0.0,0.0,1.0), scale:(1.0,1.0,1.0))"
                            .into(),
                    }],
                    relations: vec![],
                },
                Entity {
                    id: child,
                    name: "ChildLight".into(),
                    components: vec![],
                    relations: vec![Relation::ChildOf { parent }],
                },
            ],
            root_entities: vec![parent],
        }
    }

    #[test]
    fn round_trip_ron() {
        let s0 = fixture_scene();
        let text = ron::ser::to_string_pretty(&s0, ron::ser::PrettyConfig::default()).expect("ser");
        let back: Scene = ron::from_str(&text).expect("de");
        assert_eq!(s0, back);
    }

    #[test]
    fn find_entity_returns_match() {
        let s0 = fixture_scene();
        let id = s0.entities[0].id;
        assert!(s0.find_entity(id).is_some());
        assert!(s0.find_entity(EntityId::from_u128(999)).is_none());
    }

    #[test]
    fn empty_constructor() {
        let s = Scene::empty("blank", SchemaVersion::V0_0_0);
        assert!(s.entities.is_empty());
        assert!(s.root_entities.is_empty());
    }
}
