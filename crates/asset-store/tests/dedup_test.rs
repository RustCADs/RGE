//! Integration test: insert same bytes twice → same `AssetId`, single
//! storage entry on disk. This is the W16 exit-criteria invariant.

use std::fs;

use rge_asset_store::{layout, AssetId, Cache, LocalCache};
use tempfile::TempDir;

/// A storage entry under the cache root is a single file living at
/// `<root>/<2-char>/<full-hash>`. Two `put`s of identical bytes must
/// not produce two such files.
#[test]
fn put_twice_same_bytes_yields_one_file_on_disk() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let payload = b"the same bytes, twice";
    let id_a = cache.put(payload.to_vec()).expect("put a");
    let id_b = cache.put(payload.to_vec()).expect("put b");
    assert_eq!(
        id_a, id_b,
        "AssetId is content-addressed; same input → same id"
    );

    // The cache reports exactly one logical entry.
    assert_eq!(cache.len(), 1, "dedup must collapse to one entry");

    // And exactly one file lives in the shard dir for that id.
    let path = layout::path_for(dir.path(), &id_a);
    let shard = path.parent().expect("shard parent");
    let count = fs::read_dir(shard)
        .expect("read shard")
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_file())
        .count();
    assert_eq!(count, 1, "shard dir must contain exactly one file");
}

/// Round-trip: `get(put(bytes))` returns bytes byte-identical to the
/// input.
#[test]
fn get_returns_byte_identical_payload() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let payload: Vec<u8> = (0..=255u8).chain(0..=128u8).collect(); // 384 mixed bytes
    let id = cache.put(payload.clone()).expect("put");
    let got = cache.get(&id).expect("get").expect("present");
    assert_eq!(got, payload, "round-trip must be byte-identical");
}

/// `AssetId` is a pure function of input bytes: same bytes → same id,
/// even across two distinct cache instances at different roots.
#[test]
fn asset_id_is_deterministic_across_caches() {
    let dir1 = TempDir::new().expect("t1");
    let dir2 = TempDir::new().expect("t2");
    let mut c1 = LocalCache::open(dir1.path()).expect("c1");
    let mut c2 = LocalCache::open(dir2.path()).expect("c2");

    let payload = b"determinism witness";
    let a = c1.put(payload.to_vec()).expect("put1");
    let b = c2.put(payload.to_vec()).expect("put2");

    assert_eq!(a, b, "two independent caches must agree on the id");

    // And the from_bytes path agrees (no salt sneaks in).
    let standalone = AssetId::from_bytes(payload);
    assert_eq!(a, standalone);
}

/// Three distinct payloads → three distinct ids, three files on disk.
#[test]
fn distinct_payloads_get_distinct_ids() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let id1 = cache.put(b"alpha".to_vec()).unwrap();
    let id2 = cache.put(b"beta".to_vec()).unwrap();
    let id3 = cache.put(b"gamma".to_vec()).unwrap();

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    assert_eq!(cache.len(), 3);
}
