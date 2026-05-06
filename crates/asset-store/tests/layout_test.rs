//! Integration test: filesystem layout follows the
//! `<root>/<2-char-prefix>/<full-hash>` convention.

use std::fs;

use rge_asset_store::{layout, AssetId, Cache, LocalCache};
use tempfile::TempDir;

/// After a put, the on-disk file lives exactly at
/// `<root>/<2-char>/<64-hex>`.
#[test]
fn put_writes_to_canonical_path() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    let id = cache.put(b"layout payload".to_vec()).expect("put");
    let canonical = layout::path_for(dir.path(), &id);
    assert!(canonical.is_file(), "{} must exist", canonical.display());

    // The file's parent (the shard dir) must be exactly two hex chars.
    let shard = canonical.parent().unwrap();
    let shard_name = shard.file_name().unwrap().to_str().unwrap();
    assert_eq!(shard_name.len(), 2);
    assert!(shard_name.chars().all(|c| c.is_ascii_hexdigit()));

    // The file's basename must be the full 64-hex digest.
    let fname = canonical.file_name().unwrap().to_str().unwrap();
    assert_eq!(fname.len(), 64);
    assert!(fname.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(fname, id.hex());
    // The shard prefix is the first two characters of the digest.
    assert_eq!(shard_name, &id.hex()[..2]);
}

/// Layout is pure-function: same id, same root, same path —
/// independent of any cache state.
#[test]
fn layout_path_for_is_pure() {
    let id = AssetId::from_bytes(b"pure-fn");
    let r = std::path::Path::new("/some/root");
    let p1 = layout::path_for(r, &id);
    let p2 = layout::path_for(r, &id);
    assert_eq!(p1, p2);
}

/// 256 distinct first-byte values produce 256 distinct shard dirs in
/// the worst case — verify a sampling.
#[test]
fn first_byte_determines_shard() {
    let dir = TempDir::new().expect("tempdir");
    let mut cache = LocalCache::open(dir.path()).expect("open");

    // Insert 8 different payloads that almost certainly produce
    // distinct first bytes in their digests.
    let mut shards = std::collections::BTreeSet::new();
    for i in 0u32..8 {
        let payload = format!("payload-{i}");
        let id = cache.put(payload.into_bytes()).expect("put");
        shards.insert(id.hex()[..2].to_string());
    }
    // Not all 8 must be unique, but at least 4 different shards is a
    // sanity check that the prefix reflects payload variety.
    assert!(
        shards.len() >= 4,
        "expected variety of shard prefixes, got {shards:?}"
    );
}

/// Removing a file from disk between cache reopens results in the
/// reconciler dropping the entry, so subsequent `get` returns None.
#[test]
fn reopen_after_external_delete_self_heals() {
    let dir = TempDir::new().expect("tempdir");

    let id;
    {
        let mut cache = LocalCache::open(dir.path()).expect("open");
        id = cache.put(b"will be deleted".to_vec()).unwrap();
        cache.flush_index().unwrap();
    }
    // Delete the file out from under the cache.
    let path = layout::path_for(dir.path(), &id);
    fs::remove_file(path).unwrap();

    let cache = LocalCache::open(dir.path()).expect("reopen");
    assert!(
        cache.get(&id).unwrap().is_none(),
        "self-heal: missing payload → None"
    );
    assert_eq!(cache.len(), 0);
}
