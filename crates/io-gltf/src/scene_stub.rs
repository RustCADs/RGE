// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! W17-local stub for `rge-data::Scene` (W14).
//!
//! W17 needs a concrete `Scene` type to import *into* and export *from*. W14
//! is the canonical home for that type — when it lands, this file is deleted
//! and `crate::Scene` becomes a re-export. Until then we maintain a minimal
//! stand-in: an entity table with parent links and the per-entity component
//! payloads the importer / exporter actually round-trip.
//!
//! Not a general ECS: no archetype storage, no system scheduler. Just enough
//! shape to faithfully serialise a glTF scene tree and let tests inspect the
//! result (entity count, mesh / material / skeleton attachment).

use serde::{Deserialize, Serialize};

use crate::handles::{AnimationHandle, MaterialHandle, MeshHandle, SkeletonHandle};

/// W17-local entity handle (replaces `components-spatial::Entity` once
/// `rge-data::Scene` consumes it directly). `u32` index into [`Scene::entities`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Entity(pub u32);

impl Entity {
    /// Sentinel for "no entity" (root node parent slot).
    pub const ROOT: Entity = Entity(u32::MAX);
}

impl Default for Entity {
    /// Default = [`Entity::ROOT`] so [`EntityComponents::default`] produces a
    /// root entity (no parent). Real entity handles come from
    /// [`Scene::spawn`].
    fn default() -> Self {
        Self::ROOT
    }
}

/// W17-local TRS transform (matches `components-spatial::Transform` field
/// order so once W14 lands we can swap a memcpy / `From` impl).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Translation (x, y, z) in parent frame.
    pub translation: [f32; 3],
    /// Rotation quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Per-axis scale.
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Identity transform.
    pub const IDENTITY: Transform = Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    };
}

/// Per-entity component payload that round-trips through glTF.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityComponents {
    /// Optional human-readable name (the glTF node `name` field).
    pub name: String,
    /// Local-space TRS.
    pub transform: Transform,
    /// Parent entity in the scene tree, or [`Entity::ROOT`] for roots.
    pub parent: Entity,
    /// Optional mesh handle (cached) — present if the glTF node carries a
    /// mesh index.
    pub mesh: Option<MeshHandle>,
    /// Optional material handle. v0 attaches the **first** primitive's
    /// material to the entity (multi-primitive meshes get one entity per
    /// primitive in [`crate::scene_builder::build_scene`]).
    pub material: Option<MaterialHandle>,
    /// Optional skin / skeleton handle.
    pub skeleton: Option<SkeletonHandle>,
}

/// W17-local scene container — flat entity table + animation handle list.
///
/// Replace with `rge-data::Scene` when W14 merges; the public field shape
/// is kept identical so consumers don't need to change call sites.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// All entities in this scene, indexed by their [`Entity`] id.
    pub entities: Vec<EntityComponents>,
    /// Animation clips referenced by this scene. Same insertion rules as
    /// the asset cache: handles are content-hash-stable, list order is
    /// glTF document order.
    pub animations: Vec<AnimationHandle>,
}

impl Scene {
    /// Construct a fresh empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a new entity, returning its handle.
    pub fn spawn(&mut self, components: EntityComponents) -> Entity {
        let id = self.entities.len() as u32;
        self.entities.push(components);
        Entity(id)
    }

    /// Borrow components by entity handle.
    #[must_use]
    pub fn get(&self, e: Entity) -> Option<&EntityComponents> {
        self.entities.get(e.0 as usize)
    }

    /// Mutably borrow components by entity handle.
    pub fn get_mut(&mut self, e: Entity) -> Option<&mut EntityComponents> {
        self.entities.get_mut(e.0 as usize)
    }

    /// Number of entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// True iff there are no entities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Iterate over entities in id order.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &EntityComponents)> {
        self.entities
            .iter()
            .enumerate()
            .map(|(i, c)| (Entity(i as u32), c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_returns_sequential_ids() {
        let mut s = Scene::new();
        let a = s.spawn(EntityComponents::default());
        let b = s.spawn(EntityComponents::default());
        assert_eq!(a, Entity(0));
        assert_eq!(b, Entity(1));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn entity_components_default_is_root_unattached() {
        let c = EntityComponents::default();
        assert_eq!(c.parent, Entity::default());
        assert!(c.mesh.is_none());
        assert!(c.material.is_none());
        assert!(c.skeleton.is_none());
    }

    #[test]
    fn round_trip_ron() {
        let mut s = Scene::new();
        s.spawn(EntityComponents {
            name: "root".into(),
            transform: Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let txt = ron::to_string(&s).expect("serialize");
        let back: Scene = ron::from_str(&txt).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn root_sentinel_is_max() {
        assert_eq!(Entity::ROOT, Entity(u32::MAX));
    }
}
