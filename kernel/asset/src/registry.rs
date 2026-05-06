//! In-memory asset registry with optional disk-backed dependency persistence.
//!
//! [`Registry`] stores type-erased asset payloads keyed by [`AssetId`] and
//! integrates with the [`DependencyGraph`] for invalidation propagation.
//!
//! # Disk persistence
//!
//! Only the **dependency graph** is serialised to disk (RON).  Asset payloads
//! are stored separately by callers (e.g. `crates/asset-store`).  This keeps
//! the registry thin and avoids a dependency on any particular asset format.
//!
//! # `unsafe_code = forbid`
//!
//! Type-erased storage uses `Box<dyn Any + Send + Sync>` with safe
//! `downcast_ref` / `downcast_mut`.  No unsafe code is needed.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use tracing::warn;

use crate::dependency_graph::DependencyGraph;
use crate::handle::{Handle, HandleStrong};
use crate::id::AssetId;

// ---------------------------------------------------------------------------
// RegistryEntry — one slot in the registry
// ---------------------------------------------------------------------------

/// One slot in the registry: type-erased payload + weak ref-count token.
struct RegistryEntry {
    /// Type-erased payload.  Downcast with `downcast_ref::<T>()`.
    payload: Box<dyn Any + Send + Sync>,
    /// Weak pointer to the strong-count token.  When `upgrade()` returns
    /// `None`, no live [`Handle`] exists for this asset.
    strong: Weak<HandleStrong>,
    /// Human-readable type name of the stored payload for error messages.
    type_name: &'static str,
}

// ---------------------------------------------------------------------------
// RegistryError
// ---------------------------------------------------------------------------

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// The requested asset does not exist in the registry.
    #[error("asset {0} not found")]
    NotFound(AssetId),

    /// The asset exists but was stored as a different type.
    #[error("type mismatch for asset {id}: stored as {stored}, requested as {requested}")]
    TypeMismatch {
        /// The asset whose type did not match.
        id: AssetId,
        /// The name of the type that is stored.
        stored: &'static str,
        /// The name of the type that was requested.
        requested: &'static str,
    },

    /// Disk serialisation or deserialisation failed.
    #[error("disk persistence error: {0}")]
    DiskError(String),
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// In-memory asset registry.
///
/// Stores typed payloads keyed by [`AssetId`] and tracks asset dependencies
/// for invalidation propagation via an integrated [`DependencyGraph`].
///
/// # Disk persistence
///
/// The dependency graph can be serialised to RON with [`serialize_deps`] and
/// restored with [`restore_deps`].  Asset payloads are NOT included — callers
/// store those separately (e.g. on-disk via `crates/asset-store`).
///
/// [`serialize_deps`]: Registry::serialize_deps
/// [`restore_deps`]: Registry::restore_deps
#[derive(Default)]
pub struct Registry {
    /// Type-erased payload storage: `AssetId` → `RegistryEntry`.
    payloads: HashMap<AssetId, RegistryEntry>,
    /// Dependency graph (serialisable).
    deps: DependencyGraph,
}

impl Registry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // Insertion / retrieval
    // -----------------------------------------------------------------------

    /// Insert a typed asset and return a [`Handle`].
    ///
    /// If an asset already exists at `id`, the old payload is replaced and a
    /// `tracing::warn` is emitted (replacing is intentional API but unusual
    /// enough to warrant a log).
    pub fn insert<T: Send + Sync + 'static>(&mut self, id: AssetId, payload: T) -> Handle<T> {
        if self.payloads.contains_key(&id) {
            warn!(
                asset_id = %id,
                "Registry::insert: replacing existing payload at id (hot-reload or duplicate insert)"
            );
        }
        let rc = Arc::new(HandleStrong);
        let strong: Weak<HandleStrong> = Arc::downgrade(&rc);
        let entry = RegistryEntry {
            payload: Box::new(payload),
            strong,
            type_name: core::any::type_name::<T>(),
        };
        self.payloads.insert(id, entry);
        Handle::new(id, rc)
    }

    /// Get a typed [`Handle`] for an existing asset.
    ///
    /// Returns:
    /// - `Ok(Some(handle))` if the asset exists and has type `T`.
    /// - `Ok(None)` if the asset is not in the registry.
    /// - `Err(RegistryError::TypeMismatch)` if the asset exists but as a
    ///   different type.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::TypeMismatch`] if the asset exists but was
    /// stored as a type other than `T`.
    ///
    /// # Note
    ///
    /// The returned handle re-uses the existing strong-count arc if one is
    /// alive; otherwise a new arc is minted and the entry's weak pointer is
    /// updated.  This means calling `handle` on an entry whose original handle
    /// was dropped and swept will resurrect it (`strong_count` = 1).
    pub fn handle<T: Send + Sync + 'static>(
        &self,
        id: AssetId,
    ) -> Result<Option<Handle<T>>, RegistryError> {
        let Some(entry) = self.payloads.get(&id) else {
            return Ok(None);
        };
        if entry.payload.downcast_ref::<T>().is_none() {
            return Err(RegistryError::TypeMismatch {
                id,
                stored: entry.type_name,
                requested: core::any::type_name::<T>(),
            });
        }
        // Reuse the existing arc if alive; otherwise mint a new one.
        let rc = entry
            .strong
            .upgrade()
            .unwrap_or_else(|| Arc::new(HandleStrong));
        Ok(Some(Handle::new(id, rc)))
    }

    /// Borrow the typed payload of an asset.
    ///
    /// Returns:
    /// - `Ok(Some(&T))` if the asset exists and has type `T`.
    /// - `Ok(None)` if the asset is not in the registry.
    /// - `Err(RegistryError::TypeMismatch)` if the types do not match.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::TypeMismatch`] if the asset exists but was
    /// stored as a type other than `T`.
    pub fn get<T: Send + Sync + 'static>(&self, id: AssetId) -> Result<Option<&T>, RegistryError> {
        let Some(entry) = self.payloads.get(&id) else {
            return Ok(None);
        };
        entry
            .payload
            .downcast_ref::<T>()
            .map(Some)
            .ok_or_else(|| RegistryError::TypeMismatch {
                id,
                stored: entry.type_name,
                requested: core::any::type_name::<T>(),
            })
    }

    /// Mutably borrow the typed payload of an asset.
    ///
    /// Returns:
    /// - `Ok(Some(&mut T))` if the asset exists and has type `T`.
    /// - `Ok(None)` if the asset is not in the registry.
    /// - `Err(RegistryError::TypeMismatch)` if the types do not match.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::TypeMismatch`] if the asset exists but was
    /// stored as a type other than `T`.
    pub fn get_mut<T: Send + Sync + 'static>(
        &mut self,
        id: AssetId,
    ) -> Result<Option<&mut T>, RegistryError> {
        let Some(entry) = self.payloads.get_mut(&id) else {
            return Ok(None);
        };
        let type_name = entry.type_name;
        entry
            .payload
            .downcast_mut::<T>()
            .map(Some)
            .ok_or(RegistryError::TypeMismatch {
                id,
                stored: type_name,
                requested: core::any::type_name::<T>(),
            })
    }

    // -----------------------------------------------------------------------
    // Removal / eviction
    // -----------------------------------------------------------------------

    /// Drop an asset entry regardless of outstanding handles.
    ///
    /// Returns `true` if the entry existed and was removed.
    pub fn remove(&mut self, id: AssetId) -> bool {
        self.payloads.remove(&id).is_some()
    }

    /// Sweep entries whose strong-count has reached zero (no live handles).
    ///
    /// Returns the number of entries evicted.
    pub fn sweep_orphans(&mut self) -> usize {
        let before = self.payloads.len();
        self.payloads
            .retain(|_id, entry| entry.strong.upgrade().is_some());
        before - self.payloads.len()
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    /// Number of assets currently in the registry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.payloads.len()
    }

    /// `true` when the registry contains no assets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.payloads.is_empty()
    }

    /// Iterator over all [`AssetId`]s currently in the registry.
    pub fn ids(&self) -> impl Iterator<Item = AssetId> + '_ {
        self.payloads.keys().copied()
    }

    // -----------------------------------------------------------------------
    // Dependency graph access
    // -----------------------------------------------------------------------

    /// Shared reference to the dependency graph.
    #[must_use]
    pub fn deps(&self) -> &DependencyGraph {
        &self.deps
    }

    /// Mutable reference to the dependency graph.
    pub fn deps_mut(&mut self) -> &mut DependencyGraph {
        &mut self.deps
    }

    // -----------------------------------------------------------------------
    // Disk persistence
    // -----------------------------------------------------------------------

    /// Serialise the dependency graph to a RON string.
    ///
    /// Asset payloads are NOT included — callers store those separately (e.g.
    /// on the filesystem via `crates/asset-store`).
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::DiskError`] if RON serialisation fails.
    pub fn serialize_deps(&self) -> Result<String, RegistryError> {
        ron::to_string(&self.deps).map_err(|e| RegistryError::DiskError(e.to_string()))
    }

    /// Restore the dependency graph from a RON string produced by
    /// [`serialize_deps`].
    ///
    /// Does NOT load asset payloads.  The existing dependency graph is
    /// **replaced** entirely by the deserialised graph.
    ///
    /// [`serialize_deps`]: Registry::serialize_deps
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::DiskError`] if RON deserialisation fails.
    pub fn restore_deps(&mut self, ron_text: &str) -> Result<(), RegistryError> {
        self.deps = ron::from_str(ron_text).map_err(|e| RegistryError::DiskError(e.to_string()))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &[u8]) -> AssetId {
        AssetId::from_bytes(s)
    }

    #[test]
    fn insert_returns_handle_with_matching_id() {
        let mut reg = Registry::new();
        let aid = id(b"my-asset");
        let h = reg.insert(aid, 42u32);
        assert_eq!(h.id(), aid);
        assert_eq!(h.strong_count(), 1);
    }

    #[test]
    fn second_insert_at_same_id_replaces() {
        let mut reg = Registry::new();
        let aid = id(b"asset");
        reg.insert(aid, 1u32);
        reg.insert(aid, 2u32);
        let val = reg.get::<u32>(aid).expect("ok").expect("some");
        assert_eq!(*val, 2);
    }

    #[test]
    fn get_returns_none_for_missing_id() {
        let reg = Registry::new();
        let result = reg.get::<u32>(id(b"missing")).expect("ok");
        assert!(result.is_none());
    }

    #[test]
    fn get_errors_on_type_mismatch() {
        let mut reg = Registry::new();
        let aid = id(b"typed");
        reg.insert(aid, 42u32);
        let err = reg.get::<u64>(aid).unwrap_err();
        assert!(matches!(err, RegistryError::TypeMismatch { .. }));
    }

    #[test]
    fn handle_returns_none_for_missing_id() {
        let reg = Registry::new();
        let result = reg.handle::<u32>(id(b"missing")).expect("ok");
        assert!(result.is_none());
    }

    #[test]
    fn handle_errors_on_type_mismatch() {
        let mut reg = Registry::new();
        let aid = id(b"typed");
        reg.insert(aid, 42u32);
        let err = reg.handle::<String>(aid).unwrap_err();
        assert!(matches!(err, RegistryError::TypeMismatch { .. }));
    }

    #[test]
    fn get_mut_can_modify_payload() {
        let mut reg = Registry::new();
        let aid = id(b"mut");
        reg.insert(aid, 0u32);
        *reg.get_mut::<u32>(aid).expect("ok").expect("some") = 99;
        assert_eq!(*reg.get::<u32>(aid).expect("ok").expect("some"), 99);
    }

    #[test]
    fn remove_returns_true_when_existed() {
        let mut reg = Registry::new();
        let aid = id(b"r");
        reg.insert(aid, 1u32);
        assert!(reg.remove(aid));
        assert!(reg.is_empty());
    }

    #[test]
    fn remove_returns_false_when_not_present() {
        let mut reg = Registry::new();
        assert!(!reg.remove(id(b"missing")));
    }

    #[test]
    fn len_and_is_empty() {
        let mut reg = Registry::new();
        assert!(reg.is_empty());
        reg.insert(id(b"a"), 1u32);
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn sweep_orphans_removes_entries_with_no_live_handles() {
        let mut reg = Registry::new();
        let aid = id(b"sweep");
        let h = reg.insert(aid, 42u32);
        let h2 = h.clone();
        // Both handles alive — sweep must not evict.
        assert_eq!(reg.sweep_orphans(), 0);
        assert_eq!(reg.len(), 1);
        // Drop both handles.
        drop(h);
        drop(h2);
        // Now sweep should evict.
        assert_eq!(reg.sweep_orphans(), 1);
        assert!(reg.is_empty());
    }

    #[test]
    fn sweep_orphans_keeps_entries_with_live_handles() {
        let mut reg = Registry::new();
        let aid = id(b"alive");
        let _h = reg.insert(aid, 1u32);
        assert_eq!(reg.sweep_orphans(), 0);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn dep_graph_accessible_via_registry() {
        let mut reg = Registry::new();
        let a = id(b"a");
        let b = id(b"b");
        reg.deps_mut().add_edge(a, b);
        assert!(reg.deps().dependencies(a).any(|d| d == b));
    }

    #[test]
    fn serialize_and_restore_deps_round_trips() {
        let mut reg = Registry::new();
        let a = id(b"a");
        let b = id(b"b");
        reg.deps_mut().add_edge(a, b);
        let ron = reg.serialize_deps().expect("serialize");

        let mut reg2 = Registry::new();
        reg2.restore_deps(&ron).expect("restore");
        let ron2 = reg2.serialize_deps().expect("re-serialize");
        assert_eq!(ron, ron2);
    }
}
