//! View registry substrate.
//!
//! # NON-GOALS (mirror of crate-level doc, restated for in-module ownership clarity)
//!
//! - No WASM linear-memory mapping. v0 ships descriptors only; the eventual
//!   zero-copy slice exposure (PLAN §1.6.5) lands in dedicated future
//!   dispatches with the relevant unsafe boundary.
//! - No `unsafe` zero-copy. v0 has no buffer ownership; descriptors point
//!   at imaginary backing storage.
//! - No buffer / allocation ownership. The registry tracks descriptors;
//!   the buffers themselves live elsewhere (eventual `kernel/asset` /
//!   `kernel/gfx` / `kernel/asset-streaming` consumers).
//! - No residency / streaming policy — that's `kernel/asset-streaming`.
//! - No GPU upload semantics — that's `kernel/gfx` / future `kernel/gpu-resources`.
//! - No I/O scheduling priority — that's `kernel/io-scheduler`.
//! - No work scheduling — that's `kernel/job-system`.
//! - No closures / callbacks / observers. The registry is passive lookup;
//!   consumers poll via [`AssetViewRegistry::lookup`] / [`AssetViewRegistry::iter`].
//! - No `kernel/asset` integration. Callers route ID generation; v0 does NOT
//!   tie [`crate::AssetViewId`] to `kernel/asset::AssetId`.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.

use std::collections::BTreeMap;

use crate::descriptor::ViewDescriptor;
use crate::id::AssetViewId;

/// Registry of asset-view descriptors.
///
/// v0 stub: `BTreeMap`-backed for deterministic iteration order. Lookup is
/// O(log n); iteration yields descriptors in `AssetViewId` byte-order.
///
/// The registry owns no buffers, no GPU resources, no IO state. It is
/// purely a substrate for tracking which views are currently registered;
/// consumers (eventual zero-copy WASM bridge, asset-streaming residency
/// manager, future render-side view binders) look up descriptors and
/// dispatch buffer access via mechanisms that land in dedicated future
/// dispatches.
#[derive(Default, Debug)]
pub struct AssetViewRegistry {
    /// Registered views keyed by [`AssetViewId`] for deterministic
    /// iteration. `BTreeMap` rather than `HashMap` so `iter()` is stable
    /// and reproducible across runs.
    views: BTreeMap<AssetViewId, ViewDescriptor>,
}

impl AssetViewRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `descriptor`. If a descriptor with the same [`AssetViewId`]
    /// was already registered, returns the previous descriptor (replaces
    /// the entry in-place; matches `BTreeMap::insert` semantics).
    pub fn register(&mut self, descriptor: ViewDescriptor) -> Option<ViewDescriptor> {
        self.views.insert(descriptor.id, descriptor)
    }

    /// Unregister the descriptor for `id`.
    ///
    /// Returns the removed descriptor, or `None` if no descriptor was
    /// registered under `id`.
    pub fn unregister(&mut self, id: AssetViewId) -> Option<ViewDescriptor> {
        self.views.remove(&id)
    }

    /// Look up the descriptor for `id`, or `None` if no descriptor is
    /// registered.
    #[must_use]
    pub fn lookup(&self, id: AssetViewId) -> Option<&ViewDescriptor> {
        self.views.get(&id)
    }

    /// True iff a descriptor for `id` is currently registered.
    #[must_use]
    pub fn contains(&self, id: AssetViewId) -> bool {
        self.views.contains_key(&id)
    }

    /// Number of registered descriptors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// `true` when no descriptors are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Iterate registered descriptors in `AssetViewId` byte-order.
    ///
    /// Iteration is deterministic and does not consume the registry.
    pub fn iter(&self) -> impl Iterator<Item = &ViewDescriptor> {
        self.views.values()
    }

    /// Remove every registered descriptor.
    pub fn clear(&mut self) {
        self.views.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::ViewKind;

    fn d(id_byte: u8, byte_len: u64) -> ViewDescriptor {
        ViewDescriptor::new(
            AssetViewId::from_bytes([id_byte; 16]),
            ViewKind::Placeholder,
            byte_len,
        )
    }

    #[test]
    fn empty_new_is_empty() {
        let r = AssetViewRegistry::new();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn register_returns_none_for_new_id() {
        let mut r = AssetViewRegistry::new();
        let prev = r.register(d(1, 100));
        assert!(prev.is_none());
        assert_eq!(r.len(), 1);
        assert!(!r.is_empty());
    }

    #[test]
    fn register_returns_previous_for_duplicate_id() {
        let mut r = AssetViewRegistry::new();
        r.register(d(1, 100));
        let prev = r.register(d(1, 200));
        assert!(prev.is_some());
        assert_eq!(prev.unwrap().byte_len, 100);
        // Length unchanged (replacement, not addition).
        assert_eq!(r.len(), 1);
        // Looking up now yields the new descriptor.
        let now = r.lookup(AssetViewId::from_bytes([1u8; 16])).unwrap();
        assert_eq!(now.byte_len, 200);
    }

    #[test]
    fn lookup_finds_registered_descriptor() {
        let mut r = AssetViewRegistry::new();
        r.register(d(7, 42));
        let found = r.lookup(AssetViewId::from_bytes([7u8; 16]));
        assert!(found.is_some());
        assert_eq!(found.unwrap().byte_len, 42);
    }

    #[test]
    fn lookup_returns_none_for_missing_id() {
        let r = AssetViewRegistry::new();
        assert!(r.lookup(AssetViewId::from_bytes([0u8; 16])).is_none());
    }

    #[test]
    fn contains_reflects_registration() {
        let mut r = AssetViewRegistry::new();
        let id = AssetViewId::from_bytes([3u8; 16]);
        assert!(!r.contains(id));
        r.register(d(3, 10));
        assert!(r.contains(id));
        r.unregister(id);
        assert!(!r.contains(id));
    }

    #[test]
    fn unregister_returns_descriptor_when_present() {
        let mut r = AssetViewRegistry::new();
        r.register(d(5, 64));
        let removed = r.unregister(AssetViewId::from_bytes([5u8; 16]));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().byte_len, 64);
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn unregister_returns_none_when_missing() {
        let mut r = AssetViewRegistry::new();
        let removed = r.unregister(AssetViewId::from_bytes([99u8; 16]));
        assert!(removed.is_none());
    }

    #[test]
    fn iter_yields_descriptors_in_id_byte_order() {
        let mut r = AssetViewRegistry::new();
        // Register out of byte order; iteration must yield by-id-order.
        r.register(d(3, 30));
        r.register(d(1, 10));
        r.register(d(2, 20));

        let byte_lens: Vec<u64> = r.iter().map(|d| d.byte_len).collect();
        assert_eq!(byte_lens, vec![10, 20, 30]);
        // Iteration does not consume.
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn clear_empties_registry() {
        let mut r = AssetViewRegistry::new();
        r.register(d(1, 10));
        r.register(d(2, 20));
        r.clear();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn default_impl_matches_new() {
        let a = AssetViewRegistry::new();
        let b = AssetViewRegistry::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}
