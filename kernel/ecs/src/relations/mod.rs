//! Relation types and helper functions.
//!
//! Relations are typed edges between entities.  Each relation type has a
//! corresponding storage backend chosen for its expected topology:
//!
//! | Relation | Storage | Topology |
//! |---|---|---|
//! | [`ParentOf`] (`parent_of`) | [`TreeRelationStorage`] | Sparse tree |
//! | [`BoneOf`] (`bone_of`) | [`DenseLinearRelationStorage`] | Dense ordered list |
//!
//! # Usage
//!
//! ```rust
//! # use rge_kernel_ecs::{World, parent_of};
//! let mut world = World::new();
//! let parent = world.spawn();
//! let child  = world.spawn();
//! parent_of(&mut world, parent, child);
//! ```

use crate::entity::EntityId;
use crate::storage::{DenseLinearRelationStorage, SparseRelationStorage, TreeRelationStorage};
use crate::world::World;

// ---------------------------------------------------------------------------
// RelationTag — sealed marker trait
// ---------------------------------------------------------------------------

/// Marker trait that associates a relation type with its storage backend.
///
/// Implemented by [`ParentOf`] and [`BoneOf`].
pub trait RelationTag: 'static {
    /// The storage type used for this relation.
    type Storage: Default + Send + Sync + 'static;
}

/// Marker for the `parent_of` relation (tree hierarchy).
///
/// Storage: [`TreeRelationStorage`].
#[derive(Debug, Clone, Copy)]
pub struct ParentOf;

impl RelationTag for ParentOf {
    type Storage = TreeRelationStorage;
}

/// Marker for the `bone_of` relation (dense linear skeleton hierarchy).
///
/// Storage: [`DenseLinearRelationStorage`].
#[derive(Debug, Clone, Copy)]
pub struct BoneOf;

impl RelationTag for BoneOf {
    type Storage = DenseLinearRelationStorage;
}

/// Marker for the `lod_of` relation (sparse LOD group membership).
///
/// Storage: [`SparseRelationStorage`].
#[derive(Debug, Clone, Copy)]
pub struct LodOf;

impl RelationTag for LodOf {
    type Storage = SparseRelationStorage;
}

/// Marker for the `template_of` relation (sparse template instantiation).
///
/// Storage: [`SparseRelationStorage`].
#[derive(Debug, Clone, Copy)]
pub struct TemplateOf;

impl RelationTag for TemplateOf {
    type Storage = SparseRelationStorage;
}

// ---------------------------------------------------------------------------
// Free-function helpers
// ---------------------------------------------------------------------------

/// Link `parent → child` in the world's [`ParentOf`] (tree) relation storage.
///
/// If `child` already has a parent, the old link is replaced.
pub fn parent_of(world: &mut World, parent: EntityId, child: EntityId) {
    world.relations_mut::<ParentOf>().link(parent, child);
}

/// Link `source → target` in the world's [`BoneOf`] (dense-linear) relation storage.
///
/// Appends `target` to `source`'s ordered bone list.
pub fn bone_of(world: &mut World, source: EntityId, target: EntityId) {
    world.relations_mut::<BoneOf>().link(source, target);
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn parent_of_link_and_iter() {
        let mut world = World::new();
        let root = world.spawn();
        let a = world.spawn();
        let b = world.spawn();
        parent_of(&mut world, root, a);
        parent_of(&mut world, root, b);
        let children: Vec<_> = world
            .relations::<ParentOf>()
            .unwrap()
            .iter_children(root)
            .collect();
        assert_eq!(children.len(), 2);
        assert!(children.contains(&a));
        assert!(children.contains(&b));
    }

    #[test]
    fn bone_of_order_preserved() {
        let mut world = World::new();
        let skeleton = world.spawn();
        let bones: Vec<EntityId> = (0..4).map(|_| world.spawn()).collect();
        for &bone in &bones {
            bone_of(&mut world, skeleton, bone);
        }
        let linked: Vec<_> = world
            .relations::<BoneOf>()
            .unwrap()
            .iter_targets(skeleton)
            .collect();
        assert_eq!(linked, bones);
    }
}
