//! Integration test: LRU eviction respects `max_bytes` cap.

use rge_asset_store::{Cache, LocalCache};
use tempfile::TempDir;

/// Insert N entries totalling more than the cap, evict, observe the
/// total stored size is `<= cap`. Spec exit criterion.
#[test]
fn evict_lru_caps_total_size() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    for i in 0u8..10 {
        cache.put(vec![i; 100]).expect("put");
    }
    assert_eq!(cache.total_size(), 1000);

    cache.evict_lru(350).expect("evict");
    assert!(
        cache.total_size() <= 350,
        "after evict_lru(350) total = {}",
        cache.total_size()
    );
}

/// Most-recently-inserted assets survive eviction; oldest ones are
/// dropped first.
#[test]
fn evict_lru_drops_oldest_first() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let ids: Vec<_> = (0u8..5)
        .map(|i| cache.put(vec![i; 200]).expect("put"))
        .collect();
    assert_eq!(cache.total_size(), 1000);

    // Cap to 400 — only the two newest survive.
    cache.evict_lru(400).expect("evict");
    assert!(cache.total_size() <= 400);
    assert!(
        cache.get(&ids[3]).unwrap().is_some(),
        "newest-1 must survive"
    );
    assert!(cache.get(&ids[4]).unwrap().is_some(), "newest must survive");
    assert!(
        cache.get(&ids[0]).unwrap().is_none(),
        "oldest must be evicted"
    );
}

/// `get` updates recency, so a recently-read entry survives an
/// eviction that would otherwise drop it.
#[test]
fn get_promotes_recency() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let id_old = cache.put(vec![0; 100]).unwrap();
    let id_new = cache.put(vec![1; 100]).unwrap();
    // Read the older one — bumps its recency above the newer one.
    drop(cache.get(&id_old).unwrap());
    // Cap to 100 — exactly one survives. Must be id_old (just read).
    cache.evict_lru(100).unwrap();
    assert!(
        cache.get(&id_old).unwrap().is_some(),
        "promoted entry must survive"
    );
    assert!(
        cache.get(&id_new).unwrap().is_none(),
        "non-promoted entry must evict"
    );
}

/// Eviction below cap is a no-op: nothing changes.
#[test]
fn evict_under_cap_is_noop() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    cache.put(vec![0; 50]).unwrap();
    cache.put(vec![1; 50]).unwrap();
    let before_total = cache.total_size();
    let before_len = cache.len();

    cache.evict_lru(10_000).expect("evict");

    assert_eq!(cache.total_size(), before_total);
    assert_eq!(cache.len(), before_len);
}

/// Eviction to zero clears every entry and removes every file.
#[test]
fn evict_to_zero_clears_completely() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    for i in 0u8..5 {
        cache.put(vec![i; 100]).unwrap();
    }
    cache.evict_lru(0).expect("evict to zero");
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.total_size(), 0);
}
