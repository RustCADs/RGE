// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//                                                                  for the
//                                                                  `.rge-prefab`
//                                                                  schema.
//
//! [`Prefab`] — top-level schema for `.rge-prefab` files.
//!
//! Per `PLAN.md` §1.6.6 a prefab is a reusable, parameterizable bundle of
//! entities. It looks like a [`Scene`](crate::scene::Scene) but additionally
//! carries:
//!
//! - **`parameters`** — typed knobs the instantiating scene can override
//!   (e.g. `color`, `max_health`),
//! - **`exposed_overrides`** — the set of `(entity_id, component_field)`
//!   pairs that a parent scene is allowed to override per-instance.
//!
//! The renderer / runtime never instantiates a prefab directly; the editor
//! and the asset pipeline consume this struct, expand it into runtime
//! entities, and emit a "flat" scene under the hood.

use serde::{Deserialize, Serialize};

use crate::entity_ref::EntityId;
use crate::scene::Entity;
use crate::schema_version::SchemaVersion;

/// Top-level `.rge-prefab` schema. See module docs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Prefab {
    /// File-format schema version.
    pub version: SchemaVersion,
    /// Display name.
    pub name: String,
    /// Typed parameter declarations. Filling these in at instantiation time
    /// is the prefab's substitute for inheritance.
    pub parameters: Vec<ParamSpec>,
    /// Entities baked into the prefab. Their ids are scene-stable in the
    /// **prefab's** namespace; runtime instantiation re-maps them.
    pub entities: Vec<Entity>,
    /// `(entity_id, component_field)` paths the parent scene can override.
    /// A parent scene that overrides an unexposed field is rejected at load.
    pub exposed_overrides: Vec<ExposedOverride>,
}

impl Prefab {
    /// Empty prefab at the given schema version.
    #[must_use]
    pub fn empty(name: impl Into<String>, version: SchemaVersion) -> Self {
        Self {
            version,
            name: name.into(),
            parameters: Vec::new(),
            entities: Vec::new(),
            exposed_overrides: Vec::new(),
        }
    }
}

/// One typed knob a parent scene may set when instantiating the prefab.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Parameter name (`"color"`, `"max_health"`, …).
    pub name: String,
    /// Type name; matches `kernel/types::TypeId::path`. Stored as a string
    /// so this crate has no Reflect dependency.
    pub ty: String,
    /// Default RON literal, parsed by the reflection bridge at instantiation.
    pub default: String,
}

/// One `(entity, field)` path the parent scene may override.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ExposedOverride {
    /// Which entity inside the prefab is exposed.
    pub entity: EntityId,
    /// Component type id (`"rge::components::Transform"`, …).
    pub component_type: String,
    /// Field path within the component (`"translation"`, `"color.r"`).
    /// Dot-separated — exact mapping belongs to the reflection bridge.
    pub field_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ComponentValue;

    fn fixture_prefab() -> Prefab {
        let entity = EntityId::from_u128(0x0000_0000_0000_0042_0000_0000_0000_0000_u128);
        Prefab {
            version: SchemaVersion::V0_1_0,
            name: "EnemyArcher".into(),
            parameters: vec![ParamSpec {
                name: "max_health".into(),
                ty: "f32".into(),
                default: "100.0".into(),
            }],
            entities: vec![Entity {
                id: entity,
                name: "Body".into(),
                components: vec![ComponentValue {
                    type_id: "rge::components::Transform".into(),
                    data: "(translation:(0.0,0.0,0.0), rotation:(0.0,0.0,0.0,1.0), scale:(1.0,1.0,1.0))"
                        .into(),
                }],
                relations: vec![],
            }],
            exposed_overrides: vec![ExposedOverride {
                entity,
                component_type: "rge::components::Transform".into(),
                field_path: "translation".into(),
            }],
        }
    }

    #[test]
    fn round_trip_ron() {
        let p0 = fixture_prefab();
        let text = ron::ser::to_string_pretty(&p0, ron::ser::PrettyConfig::default()).expect("ser");
        let back: Prefab = ron::from_str(&text).expect("de");
        assert_eq!(p0, back);
    }

    #[test]
    fn empty_constructor() {
        let p = Prefab::empty("blank", SchemaVersion::V0_0_0);
        assert!(p.entities.is_empty());
        assert!(p.parameters.is_empty());
        assert!(p.exposed_overrides.is_empty());
    }
}
