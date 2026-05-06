//! Specialised relation-storage backends.
//!
//! Three storage strategies are provided, each optimised for a different
//! relation topology:
//!
//! | Type | Relation | Topology |
//! |---|---|---|
//! | [`TreeRelationStorage`] | `parent_of` | Sparse tree; parent has few children |
//! | [`DenseLinearRelationStorage`] | `bone_of` | Dense ordered list; insertion order matters |
//! | [`SparseRelationStorage`] | `lod_of`, `template_of` | Sparse map; arbitrary density |

use std::collections::HashMap;

use crate::entity::EntityId;

// ---------------------------------------------------------------------------
// TreeRelationStorage
// ---------------------------------------------------------------------------

/// Relation storage for tree-shaped hierarchies (e.g., `parent_of`).
///
/// Each entity can have at most one parent; each entity can have multiple
/// children.  Iteration over children is O(children) not O(all entities).
///
/// Internally keeps two maps:
/// - `children: HashMap<EntityId, Vec<EntityId>>` — parent → ordered child list.
/// - `parent: HashMap<EntityId, EntityId>` — child → parent.
#[derive(Debug, Default)]
pub struct TreeRelationStorage {
    children: HashMap<EntityId, Vec<EntityId>>,
    parent: HashMap<EntityId, EntityId>,
}

impl TreeRelationStorage {
    /// Create an empty storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Link `parent → child`.
    ///
    /// If `child` already has a different parent, the old link is first removed.
    /// Appends `child` to the end of `parent`'s child list.
    pub fn link(&mut self, parent: EntityId, child: EntityId) {
        // Remove old parent link if any.
        if let Some(old_parent) = self.parent.remove(&child) {
            if let Some(siblings) = self.children.get_mut(&old_parent) {
                siblings.retain(|&s| s != child);
            }
        }
        self.parent.insert(child, parent);
        self.children.entry(parent).or_default().push(child);
    }

    /// Unlink `child` from its current parent.
    ///
    /// No-op if `child` has no parent.
    pub fn unlink(&mut self, child: EntityId) {
        if let Some(old_parent) = self.parent.remove(&child) {
            if let Some(siblings) = self.children.get_mut(&old_parent) {
                siblings.retain(|&s| s != child);
            }
        }
    }

    /// Return the parent of `child`, if any.
    #[must_use]
    pub fn parent(&self, child: EntityId) -> Option<EntityId> {
        self.parent.get(&child).copied()
    }

    /// Iterate the children of `parent` in insertion order.
    pub fn iter_children(&self, parent: EntityId) -> impl Iterator<Item = EntityId> + '_ {
        self.children
            .get(&parent)
            .into_iter()
            .flat_map(|v| v.iter().copied())
    }

    /// Returns `true` when `entity` has any children.
    #[must_use]
    pub fn has_children(&self, entity: EntityId) -> bool {
        self.children.get(&entity).is_some_and(|v| !v.is_empty())
    }

    /// Remove all links involving `entity` (both as parent and child).
    ///
    /// Children of `entity` become roots (their parent is cleared).
    pub fn remove_entity(&mut self, entity: EntityId) {
        // Remove entity as a child from its parent.
        self.unlink(entity);
        // Remove entity as a parent; clear children's parent entries.
        if let Some(kids) = self.children.remove(&entity) {
            for kid in kids {
                self.parent.remove(&kid);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DenseLinearRelationStorage
// ---------------------------------------------------------------------------

/// Relation storage for dense ordered lists (e.g., `bone_of` skeleton).
///
/// Optimised for the common case where a "source" entity has an ordered list
/// of "target" entities (e.g., a skeleton root and its bones).  Insertion
/// order is preserved and is deterministic.
///
/// Internally: `HashMap<EntityId, Vec<EntityId>>` from source → ordered targets.
#[derive(Debug, Default)]
pub struct DenseLinearRelationStorage {
    targets: HashMap<EntityId, Vec<EntityId>>,
    /// Reverse map: target → source (for unlink efficiency).
    source_of: HashMap<EntityId, EntityId>,
}

impl DenseLinearRelationStorage {
    /// Create an empty storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `target` to `source`'s ordered list.
    ///
    /// If `target` already belongs to a source, the old link is removed first.
    pub fn link(&mut self, source: EntityId, target: EntityId) {
        if let Some(old_source) = self.source_of.remove(&target) {
            if let Some(list) = self.targets.get_mut(&old_source) {
                list.retain(|&t| t != target);
            }
        }
        self.source_of.insert(target, source);
        self.targets.entry(source).or_default().push(target);
    }

    /// Remove `target` from its current source list.
    pub fn unlink(&mut self, target: EntityId) {
        if let Some(old_source) = self.source_of.remove(&target) {
            if let Some(list) = self.targets.get_mut(&old_source) {
                list.retain(|&t| t != target);
            }
        }
    }

    /// The source of `target`, if any.
    #[must_use]
    pub fn source_of(&self, target: EntityId) -> Option<EntityId> {
        self.source_of.get(&target).copied()
    }

    /// Iterate targets of `source` in insertion order.
    pub fn iter_targets(&self, source: EntityId) -> impl Iterator<Item = EntityId> + '_ {
        self.targets
            .get(&source)
            .into_iter()
            .flat_map(|v| v.iter().copied())
    }

    /// Number of targets linked to `source`.
    #[must_use]
    pub fn target_count(&self, source: EntityId) -> usize {
        self.targets.get(&source).map_or(0, Vec::len)
    }

    /// Remove all links involving `entity`.
    pub fn remove_entity(&mut self, entity: EntityId) {
        self.unlink(entity);
        if let Some(list) = self.targets.remove(&entity) {
            for t in list {
                self.source_of.remove(&t);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SparseRelationStorage
// ---------------------------------------------------------------------------

/// Relation storage for sparse many-to-many relations (e.g., `lod_of`, `template_of`).
///
/// Unlike [`TreeRelationStorage`] (1:many) and [`DenseLinearRelationStorage`]
/// (1:ordered-many), this type supports arbitrary-density relations where an
/// entity may point to multiple targets and multiple sources may point to the
/// same target.
///
/// Internally: `HashMap<EntityId, Vec<EntityId>>` from source → list of targets.
#[derive(Debug, Default)]
pub struct SparseRelationStorage {
    relations: HashMap<EntityId, Vec<EntityId>>,
}

impl SparseRelationStorage {
    /// Create an empty storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a `source → target` entry.  Duplicates are silently ignored.
    pub fn link(&mut self, source: EntityId, target: EntityId) {
        let list = self.relations.entry(source).or_default();
        if !list.contains(&target) {
            list.push(target);
        }
    }

    /// Remove a specific `source → target` entry.
    pub fn unlink(&mut self, source: EntityId, target: EntityId) {
        if let Some(list) = self.relations.get_mut(&source) {
            list.retain(|&t| t != target);
        }
    }

    /// Remove all entries originating from `source`.
    pub fn remove_source(&mut self, source: EntityId) {
        self.relations.remove(&source);
    }

    /// Iterate all targets of `source`.
    pub fn iter_targets(&self, source: EntityId) -> impl Iterator<Item = EntityId> + '_ {
        self.relations
            .get(&source)
            .into_iter()
            .flat_map(|v| v.iter().copied())
    }

    /// Returns `true` when `source → target` is present.
    #[must_use]
    pub fn contains(&self, source: EntityId, target: EntityId) -> bool {
        self.relations
            .get(&source)
            .is_some_and(|v| v.contains(&target))
    }

    /// Number of sources with at least one target.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.relations.len()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<EntityId> {
        (0..n).map(|_| EntityId::new()).collect()
    }

    #[test]
    fn tree_link_unlink() {
        let mut s = TreeRelationStorage::new();
        let [root, a, b] = ids(3)[..] else {
            unreachable!()
        };
        s.link(root, a);
        s.link(root, b);
        let children: Vec<_> = s.iter_children(root).collect();
        assert_eq!(children, vec![a, b]);
        assert_eq!(s.parent(a), Some(root));
        s.unlink(a);
        assert_eq!(s.iter_children(root).collect::<Vec<_>>(), vec![b]);
        assert_eq!(s.parent(a), None);
    }

    #[test]
    fn tree_reparent() {
        let mut s = TreeRelationStorage::new();
        let [root1, root2, child] = ids(3)[..] else {
            unreachable!()
        };
        s.link(root1, child);
        s.link(root2, child); // reparent
        assert_eq!(s.parent(child), Some(root2));
        assert!(!s.has_children(root1));
        assert!(s.has_children(root2));
    }

    #[test]
    fn dense_linear_order_preserved() {
        let mut s = DenseLinearRelationStorage::new();
        let [src, b0, b1, b2] = ids(4)[..] else {
            unreachable!()
        };
        s.link(src, b0);
        s.link(src, b1);
        s.link(src, b2);
        let targets: Vec<_> = s.iter_targets(src).collect();
        assert_eq!(targets, vec![b0, b1, b2]);
        assert_eq!(s.target_count(src), 3);
    }

    #[test]
    fn dense_linear_unlink() {
        let mut s = DenseLinearRelationStorage::new();
        let [src, t] = ids(2)[..] else { unreachable!() };
        s.link(src, t);
        s.unlink(t);
        assert_eq!(s.target_count(src), 0);
        assert!(s.source_of(t).is_none());
    }

    #[test]
    fn sparse_link_unlink_contains() {
        let mut s = SparseRelationStorage::new();
        let [a, b, c] = ids(3)[..] else {
            unreachable!()
        };
        s.link(a, b);
        s.link(a, c);
        assert!(s.contains(a, b));
        assert!(s.contains(a, c));
        s.unlink(a, b);
        assert!(!s.contains(a, b));
        assert!(s.contains(a, c));
    }

    #[test]
    fn sparse_duplicate_ignored() {
        let mut s = SparseRelationStorage::new();
        let [a, b] = ids(2)[..] else { unreachable!() };
        s.link(a, b);
        s.link(a, b);
        assert_eq!(s.iter_targets(a).count(), 1);
    }
}
