//! Local stub for `asset-store::Cache` until W16 ships.
//!
//! W16 will provide a real content-addressed cache. Until then this module
//! exposes a minimal trait identical in shape to what the real crate is
//! expected to offer; downstream callers can target this trait now and we'll
//! re-export from `rge-asset-store` when W16 lands.
//!
//! # Migration
//!
//! When `rge-asset-store::Cache` exists with the same surface, replace
//! `use crate::asset_store_stub::Cache;` → `use rge_asset_store::Cache;`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Identifier for a cached asset blob (content-addressed key).
///
/// In the real crate this will be `blake3:<hex>`; the stub keeps it as an
/// opaque `Vec<u8>` so consumers don't have to lock down the format yet.
pub type AssetId = Vec<u8>;

/// Minimal cache surface — get, put, has.
pub trait Cache: Send + Sync {
    /// Insert an asset blob; the cache picks/derives a key.
    fn put(&self, blob: Vec<u8>) -> AssetId;
    /// Fetch an asset blob by id.
    fn get(&self, id: &AssetId) -> Option<Vec<u8>>;
    /// Check whether a blob exists in the cache.
    fn has(&self, id: &AssetId) -> bool;
}

/// In-memory implementation of [`Cache`] for tests/local use. Keys are
/// blake3-style synthetic ids derived from the byte length + a counter.
#[derive(Default)]
pub struct MemoryCache {
    entries: Arc<Mutex<HashMap<AssetId, Vec<u8>>>>,
    counter: Arc<Mutex<u64>>,
}

impl MemoryCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Cache for MemoryCache {
    fn put(&self, blob: Vec<u8>) -> AssetId {
        let mut counter = self.counter.lock().expect("MemoryCache counter poisoned");
        *counter += 1;
        let mut id = Vec::with_capacity(16);
        id.extend_from_slice(&counter.to_le_bytes());
        id.extend_from_slice(&(blob.len() as u64).to_le_bytes());
        let mut entries = self.entries.lock().expect("MemoryCache entries poisoned");
        entries.insert(id.clone(), blob);
        id
    }

    fn get(&self, id: &AssetId) -> Option<Vec<u8>> {
        self.entries
            .lock()
            .expect("MemoryCache entries poisoned")
            .get(id)
            .cloned()
    }

    fn has(&self, id: &AssetId) -> bool {
        self.entries
            .lock()
            .expect("MemoryCache entries poisoned")
            .contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let cache = MemoryCache::new();
        let id = cache.put(vec![1, 2, 3, 4]);
        assert!(cache.has(&id));
        assert_eq!(cache.get(&id), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn miss_returns_none() {
        let cache = MemoryCache::new();
        assert_eq!(cache.get(&vec![0, 0, 0, 0]), None);
    }
}
