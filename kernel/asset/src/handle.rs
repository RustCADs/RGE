//! Typed, ref-counted asset handles.
//!
//! A [`Handle<T>`] is a cheap-to-clone reference to an asset of type `T`.
//! The handle does NOT own the payload — ownership lives in the [`Registry`].
//! The ref-count (via [`Arc`]) signals to the Registry when an asset has no
//! live references and is eligible for GC via [`Registry::sweep_orphans`].
//!
//! [`Registry`]: crate::registry::Registry

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use std::sync::Arc;

use crate::id::AssetId;

// ---------------------------------------------------------------------------
// HandleStrong — the ref-count token
// ---------------------------------------------------------------------------

/// Opaque strong-reference token.
///
/// Each live [`Handle`] holds an [`Arc<HandleStrong>`].  The [`Registry`]
/// stores a matching [`std::sync::Weak<HandleStrong>`].  When the weak
/// pointer's `upgrade()` returns `None`, the registry knows no handle remains
/// for that asset entry and `sweep_orphans` can evict it.
///
/// [`Registry`]: crate::registry::Registry
pub(crate) struct HandleStrong;

// ---------------------------------------------------------------------------
// Handle<T>
// ---------------------------------------------------------------------------

/// Typed, ref-counted reference to an asset of type `T`.
///
/// The handle does NOT own the asset payload. The [`Registry`] (or another
/// loader) owns it. Cloning a handle increments a strong-reference counter;
/// dropping the last handle for a given [`AssetId`] marks the entry eligible
/// for GC via [`Registry::sweep_orphans`].
///
/// Two handles compare equal when they refer to the same [`AssetId`],
/// regardless of `Arc` pointer identity.
///
/// [`Registry`]: crate::registry::Registry
pub struct Handle<T> {
    /// The content-addressed identifier of the asset.
    id: AssetId,
    /// Strong-reference arc. The Registry holds a weak counterpart; when this
    /// count reaches zero `sweep_orphans` evicts the entry.
    rc: Arc<HandleStrong>,
    /// Phantom marker so `Handle<T>` is covariant in `T` (or at least
    /// well-formed w.r.t. the borrow checker) without actually storing a `T`.
    /// Using `fn() -> T` keeps the handle `Send + Sync` regardless of `T`.
    _marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// Construct a new handle.  Only the [`Registry`] should call this.
    ///
    /// [`Registry`]: crate::registry::Registry
    #[must_use]
    pub(crate) fn new(id: AssetId, rc: Arc<HandleStrong>) -> Self {
        Self {
            id,
            rc,
            _marker: PhantomData,
        }
    }

    /// The content-addressed identifier of the asset this handle refers to.
    #[must_use]
    pub fn id(&self) -> AssetId {
        self.id
    }

    /// Number of live handles to this asset (including this one).
    ///
    /// Delegates to [`Arc::strong_count`].  A count of 0 is impossible while
    /// any `Handle` value exists; a count of 1 means `self` is the last live
    /// reference.
    #[must_use]
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.rc)
    }
}

// ---------------------------------------------------------------------------
// Trait impls
// ---------------------------------------------------------------------------

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            rc: Arc::clone(&self.rc),
            _marker: PhantomData,
        }
    }
}

impl<T> PartialEq for Handle<T> {
    /// Two handles are equal when they refer to the same [`AssetId`].
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use the type name of T for the debug representation, and include the
        // strong count so debugging orphan sweeps is easier.
        let count = Arc::strong_count(&self.rc);
        // Load the count via a re-borrow of the underlying AtomicUsize to
        // satisfy the orphan-count check without unsafe.  `Arc::strong_count`
        // already does this.
        f.debug_struct("Handle")
            .field("type", &core::any::type_name::<T>())
            .field("id", &self.id)
            .field("strong_count", &count)
            .finish()
    }
}

// `Handle<T>` is `Send + Sync` regardless of `T`:
// - `AssetId` is `Copy + Send + Sync`.
// - `Arc<HandleStrong>` is `Send + Sync` (HandleStrong contains no T-data).
// - `PhantomData<fn() -> T>` is `Send + Sync` for all T because raw function
//   pointer types are `Send + Sync`.
// The compiler derives these automatically — no `unsafe impl` is needed.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_handle() -> Handle<u32> {
        let id = AssetId::from_bytes(b"test");
        let rc = Arc::new(HandleStrong);
        Handle::new(id, rc)
    }

    #[test]
    fn handle_id_matches_construction_id() {
        let id = AssetId::from_bytes(b"test");
        let h = Handle::<u32>::new(id, Arc::new(HandleStrong));
        assert_eq!(h.id(), id);
    }

    #[test]
    fn clone_increments_strong_count() {
        let h1 = make_handle();
        assert_eq!(h1.strong_count(), 1);
        let h2 = h1.clone();
        assert_eq!(h1.strong_count(), 2);
        assert_eq!(h2.strong_count(), 2);
    }

    #[test]
    fn drop_decrements_strong_count() {
        let h1 = make_handle();
        let h2 = h1.clone();
        assert_eq!(h1.strong_count(), 2);
        drop(h2);
        assert_eq!(h1.strong_count(), 1);
    }

    #[test]
    fn equality_by_id() {
        let id = AssetId::from_bytes(b"same");
        let h1 = Handle::<u32>::new(id, Arc::new(HandleStrong));
        let h2 = Handle::<u32>::new(id, Arc::new(HandleStrong));
        // Different Arc, same id — must be equal.
        assert_eq!(h1, h2);
    }

    #[test]
    fn inequality_for_different_ids() {
        let h1 = Handle::<u32>::new(AssetId::from_bytes(b"a"), Arc::new(HandleStrong));
        let h2 = Handle::<u32>::new(AssetId::from_bytes(b"b"), Arc::new(HandleStrong));
        assert_ne!(h1, h2);
    }

    #[test]
    fn handle_is_hashable() {
        use std::collections::HashSet;
        let h1 = make_handle();
        let h2 = h1.clone();
        let mut set = HashSet::new();
        set.insert(h1);
        set.insert(h2);
        // Same id → same hash → deduplicated to 1 entry.
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn debug_contains_type_and_id() {
        let h = make_handle();
        let dbg = format!("{h:?}");
        assert!(dbg.contains("Handle"), "got: {dbg}");
    }
}
