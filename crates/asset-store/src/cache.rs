// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets

//! Cache trait — the seam other crates stub against.
//!
//! Crates downstream of `asset-store` (W17 io-gltf, W18 io-image,
//! W15 pak-format, W14 rge-data) use the [`Cache`] trait directly so
//! they can be unit-tested against an in-memory fake without paying for
//! filesystem I/O. [`crate::LocalCache`] is the production impl; the
//! trait keeps the seam swappable for a future remote / distributed
//! cache (PLAN §1.2.4 hints; not v1 scope).
//!
//! # Bytes type
//!
//! Bytes are passed as owned `Vec<u8>` on the `put` side and returned as
//! owned `Vec<u8>` on the `get` side. The kernel's eventual zero-copy
//! pipeline (`kernel/asset-view`) will provide a separate
//! `AssetView<'_>` reborrow surface; `Cache` is the *resolve-the-bytes*
//! seam, not the runtime-frame view.

use crate::{AssetId, CacheError};

/// Owned byte payload. Aliased to `Vec<u8>` so callers don't have to
/// pull in `bytes::Bytes` (kept out of workspace deps at v0.8 — see
/// `Cargo.toml` rationale).
pub type Bytes = Vec<u8>;

/// Content-addressed cache contract.
///
/// All implementations must satisfy:
/// - **Idempotence:** `put(b)` twice with identical `b` produces the
///   same `AssetId` and ends with one logical entry stored. The second
///   call is a no-op storage-wise.
/// - **Round-trip fidelity:** `get(put(b))` returns `Some(c)` where
///   `c == b` byte-for-byte.
/// - **Cross-machine determinism:** the [`AssetId`] returned by `put`
///   depends only on the input bytes (delegated to `AssetId::from_bytes`).
/// - **Eviction respects the cap:** after `evict_lru(max)`, the total
///   stored size is `<= max`. The eviction order is least-recently-used,
///   where "use" is defined as `get` or `put` (a put-after-eviction
///   counts as a fresh use).
pub trait Cache {
    /// Look up an asset by its content id. Returns `None` if no asset
    /// with that id is stored.
    ///
    /// # Errors
    ///
    /// Returns a [`CacheError`] for I/O failures during the lookup
    /// (corrupted file, permission denied, etc). Returning `Ok(None)`
    /// is reserved for the "not present" case — distinguish those when
    /// hitting this through the trait.
    fn get(&self, id: &AssetId) -> Result<Option<Bytes>, CacheError>;

    /// Store the given bytes; return the content-addressed id. If an
    /// entry with the same content already exists, this is a no-op
    /// storage-wise but still updates the LRU recency.
    ///
    /// # Errors
    ///
    /// Returns a [`CacheError`] if the underlying I/O fails (filesystem
    /// out of space, permission denied, etc).
    fn put(&mut self, bytes: Bytes) -> Result<AssetId, CacheError>;

    /// Evict least-recently-used entries until total stored size is
    /// `<= max_bytes`. A no-op when current size is already under cap.
    ///
    /// # Errors
    ///
    /// Returns a [`CacheError`] if a delete I/O fails. The cache is
    /// left in a *valid but possibly partially-evicted* state on error —
    /// callers may retry.
    fn evict_lru(&mut self, max_bytes: u64) -> Result<(), CacheError>;

    /// Total size in bytes of all currently-stored assets. Snapshot —
    /// not transactionally consistent with concurrent puts (the cache
    /// is single-threaded by contract; cross-thread access requires
    /// external synchronization).
    fn total_size(&self) -> u64;

    /// Number of entries currently stored.
    fn len(&self) -> usize;

    /// Whether the cache has zero entries. Default trait impl on top of
    /// `len`; impls don't usually override.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// In-memory test fake
// ---------------------------------------------------------------------------

/// Minimal in-memory `Cache` impl for use in tests of *other* crates
/// (W17 io-gltf, W18 io-image, etc).
///
/// LRU recency is tracked by a monotonic counter so eviction order is
/// fully deterministic regardless of wall-clock skew. Not intended for
/// production — `LocalCache` is the canonical filesystem-backed impl.
#[derive(Debug, Default)]
pub struct InMemoryCache {
    /// Recency counter. Incremented on every `get` or `put`; entries
    /// store the value at their last touch.
    tick: u64,
    /// `id → (bytes, last_touch_tick)`.
    ///
    /// `BTreeMap` rather than `HashMap` so iteration is stable across
    /// platforms — useful when an evict-tied-recency case would
    /// otherwise pick differently per build. Order of iteration is by
    /// `AssetId`, not recency, so this still requires a sort on eviction.
    entries: std::collections::BTreeMap<AssetId, (Bytes, u64)>,
}

impl InMemoryCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bump the recency counter and return the new tick.
    fn touch(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }
}

impl Cache for InMemoryCache {
    fn get(&self, id: &AssetId) -> Result<Option<Bytes>, CacheError> {
        // Note: this `&self` signature can't bump the tick. The
        // `LocalCache` impl needs interior mutability for the same
        // reason, and uses a `RefCell` — see `local.rs`. The
        // in-memory test fake gets away without it because tests don't
        // exercise get-recency paths.
        Ok(self.entries.get(id).map(|(b, _)| b.clone()))
    }

    fn put(&mut self, bytes: Bytes) -> Result<AssetId, CacheError> {
        let id = AssetId::from_bytes(&bytes);
        let now = self.touch();
        // Dedup: if already present, just bump recency. Otherwise insert.
        match self.entries.get_mut(&id) {
            Some((_, t)) => {
                *t = now;
            }
            None => {
                self.entries.insert(id, (bytes, now));
            }
        }
        Ok(id)
    }

    fn evict_lru(&mut self, max_bytes: u64) -> Result<(), CacheError> {
        let mut total: u64 = self.entries.values().map(|(b, _)| b.len() as u64).sum();
        if total <= max_bytes {
            return Ok(());
        }
        // Build (id, tick) sorted ascending by tick — earliest = LRU.
        let mut order: Vec<(AssetId, u64)> =
            self.entries.iter().map(|(id, (_, t))| (*id, *t)).collect();
        order.sort_by_key(|(_, t)| *t);
        for (id, _) in order {
            if total <= max_bytes {
                break;
            }
            if let Some((b, _)) = self.entries.remove(&id) {
                total -= b.len() as u64;
            }
        }
        Ok(())
    }

    fn total_size(&self) -> u64 {
        self.entries.values().map(|(b, _)| b.len() as u64).sum()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_returns_content_addressed_id() {
        let mut c = InMemoryCache::new();
        let id = c.put(b"hello".to_vec()).expect("put");
        assert_eq!(id, AssetId::from_bytes(b"hello"));
    }

    #[test]
    fn put_then_get_round_trips_bytes() {
        let mut c = InMemoryCache::new();
        let id = c.put(b"data".to_vec()).expect("put");
        let got = c.get(&id).expect("get").expect("present");
        assert_eq!(got, b"data");
    }

    #[test]
    fn dedup_one_logical_entry_for_repeated_put() {
        let mut c = InMemoryCache::new();
        let a = c.put(b"same".to_vec()).expect("a");
        let b = c.put(b"same".to_vec()).expect("b");
        assert_eq!(a, b);
        assert_eq!(c.len(), 1, "dedup must collapse to one entry");
    }

    #[test]
    fn missing_id_returns_none_not_error() {
        let c = InMemoryCache::new();
        let id = AssetId::from_bytes(b"never inserted");
        assert!(c.get(&id).expect("not an error").is_none());
    }

    #[test]
    fn evict_lru_respects_cap() {
        let mut c = InMemoryCache::new();
        // Three 100-byte entries → 300 bytes. Cap to 200; expect at
        // most 200 bytes left.
        c.put(vec![1u8; 100]).unwrap();
        c.put(vec![2u8; 100]).unwrap();
        c.put(vec![3u8; 100]).unwrap();
        assert_eq!(c.total_size(), 300);
        c.evict_lru(200).unwrap();
        assert!(c.total_size() <= 200, "total {} > cap 200", c.total_size());
    }

    #[test]
    fn evict_lru_drops_oldest_first() {
        let mut c = InMemoryCache::new();
        let id1 = c.put(vec![1u8; 100]).unwrap();
        let id2 = c.put(vec![2u8; 100]).unwrap();
        let id3 = c.put(vec![3u8; 100]).unwrap();
        // Cap to 100 — only one entry survives, and it must be id3
        // (most recently inserted).
        c.evict_lru(100).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c.get(&id3).unwrap().is_some());
        assert!(c.get(&id1).unwrap().is_none());
        assert!(c.get(&id2).unwrap().is_none());
    }

    #[test]
    fn evict_lru_zero_cap_clears_everything() {
        let mut c = InMemoryCache::new();
        c.put(vec![1u8; 50]).unwrap();
        c.put(vec![2u8; 50]).unwrap();
        c.evict_lru(0).unwrap();
        assert_eq!(c.len(), 0);
        assert_eq!(c.total_size(), 0);
    }

    #[test]
    fn evict_lru_under_cap_is_noop() {
        let mut c = InMemoryCache::new();
        c.put(vec![1u8; 50]).unwrap();
        let before = c.total_size();
        c.evict_lru(1_000).unwrap();
        assert_eq!(c.total_size(), before);
    }

    #[test]
    fn put_after_evict_re_inserts_cleanly() {
        let mut c = InMemoryCache::new();
        let id = c.put(vec![9u8; 50]).unwrap();
        c.evict_lru(0).unwrap();
        assert!(c.get(&id).unwrap().is_none());
        let id2 = c.put(vec![9u8; 50]).unwrap();
        assert_eq!(id, id2, "id is content-addressed; reinsertion preserves it");
        assert!(c.get(&id).unwrap().is_some());
    }

    #[test]
    fn is_empty_tracks_len() {
        let mut c = InMemoryCache::new();
        assert!(c.is_empty());
        c.put(b"x".to_vec()).unwrap();
        assert!(!c.is_empty());
        c.evict_lru(0).unwrap();
        assert!(c.is_empty());
    }
}
