// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets
//
// Persistence's `Database::open(path)` creates parent dirs lazily and
// runs migrations on first open. `LocalCache::open(root)` does the
// equivalent: indexes the on-disk shard directories, loads any existing
// `<root>/.index` recency state, and returns a ready handle.
//
// The persistence crate uses SQLite for everything; here, the bulk
// payload lives as files (one per asset) and we keep a single small
// recency-index file alongside. See `layout.rs` for why.

//! Filesystem-backed [`Cache`] implementation.
//!
//! Layout under the cache root:
//! ```text
//! <root>/
//!   <2-char-prefix>/
//!     <full-hash>          ← asset payload, raw bytes
//!   .index                  ← LRU recency state (text format, see below)
//! ```
//!
//! The recency index is plain ASCII so corrupted state is salvageable
//! by hand: each line is `<tick> <hex-hash> <byte-len>`. On startup the
//! file is read once, missing entries are stat'd from disk and assigned
//! tick zero, and any indexed-but-missing entries are dropped. New
//! entries land in memory and are flushed on `flush_index` / `Drop`.
//!
//! # Recency model
//!
//! `tick` is a monotonic u64 counter on the `LocalCache` struct.
//! Bumped on every `get`/`put` hit. This makes eviction deterministic
//! across runs (no wall-clock — see PLAN §1.6.7 versioning hard rule
//! about avoiding wall-clock as a cross-build identity).
//!
//! # Thread safety
//!
//! Single-threaded. Wrap in `Arc<Mutex<...>>` for sharing — same
//! pattern as `persistence::Database`. The kernel's
//! `kernel/asset-streaming` crate (post-W16) wraps this for concurrent
//! callers.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::cache::Bytes;
use crate::{layout, AssetId, Cache, CacheError};

/// Filename of the recency index inside the cache root.
const INDEX_FILE: &str = ".index";

/// Filesystem-backed content-addressed cache.
#[derive(Debug)]
pub struct LocalCache {
    /// Root directory (e.g. `~/.cache/rge/assets`).
    root: PathBuf,
    /// Monotonic recency counter. Wrapped in `RefCell` so `get(&self)`
    /// can bump it without taking `&mut self` — the trait signature is
    /// `&self` to match the broader read-mostly contract.
    tick: RefCell<u64>,
    /// In-memory mirror of the recency index. `Some(entry)` means we
    /// believe the asset is on disk; absent means we don't.
    /// `RefCell` for the same reason as `tick`.
    entries: RefCell<BTreeMap<AssetId, IndexEntry>>,
}

/// One recency-index row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndexEntry {
    /// Last-touch tick. Higher = more recent.
    tick: u64,
    /// Cached file size (bytes). Avoids stat'ing on every total-size
    /// query.
    size: u64,
}

impl LocalCache {
    /// Open or create a cache at `root`. Loads `.index` if present,
    /// stat-reconciles against the on-disk shard dirs, and returns the
    /// handle. The shard dirs are *not* eagerly created — they're
    /// lazily made on first `put`.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] if `root` exists but is not a
    /// directory, or if the index file exists but cannot be read.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let root = root.into();
        if root.exists() && !root.is_dir() {
            return Err(CacheError::Io(format!(
                "cache root {} exists but is not a directory",
                root.display()
            )));
        }
        if !root.exists() {
            fs::create_dir_all(&root)
                .map_err(|e| CacheError::Io(format!("create_dir_all({}): {e}", root.display())))?;
        }
        let index_path = root.join(INDEX_FILE);
        let entries = if index_path.exists() {
            load_index(&index_path)?
        } else {
            BTreeMap::new()
        };
        // Reconcile: drop entries whose payload file is gone.
        let entries = reconcile(&root, entries);
        let max_tick = entries.values().map(|e| e.tick).max().unwrap_or(0);
        Ok(Self {
            root,
            tick: RefCell::new(max_tick),
            entries: RefCell::new(entries),
        })
    }

    /// Cache root directory. Useful for tests and diagnostics.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Persist the in-memory recency index to `<root>/.index`.
    /// Called automatically from `Drop`; callers may invoke explicitly
    /// before a long quiescent period.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] if the index file cannot be written.
    pub fn flush_index(&self) -> Result<(), CacheError> {
        let path = self.root.join(INDEX_FILE);
        let entries = self.entries.borrow();
        save_index(&path, &entries)
    }

    /// Internal: bump and return the recency counter.
    fn touch(&self) -> u64 {
        let mut t = self.tick.borrow_mut();
        *t = t.saturating_add(1);
        *t
    }
}

impl Cache for LocalCache {
    fn get(&self, id: &AssetId) -> Result<Option<Bytes>, CacheError> {
        let path = layout::path_for(&self.root, id);
        if !path.exists() {
            // The asset isn't there — make sure our index agrees.
            // (Prior to reconciliation this could happen if a parallel
            // process / human deleted the file out from under us.)
            self.entries.borrow_mut().remove(id);
            return Ok(None);
        }
        let bytes = fs::read(&path)
            .map_err(|e| CacheError::Io(format!("read({}): {e}", path.display())))?;
        // Touch — bump recency.
        let now = self.touch();
        let mut map = self.entries.borrow_mut();
        let len = bytes.len() as u64;
        map.entry(*id)
            .and_modify(|e| {
                e.tick = now;
                e.size = len;
            })
            .or_insert(IndexEntry {
                tick: now,
                size: len,
            });
        Ok(Some(bytes))
    }

    fn put(&mut self, bytes: Bytes) -> Result<AssetId, CacheError> {
        let id = AssetId::from_bytes(&bytes);
        let path = layout::path_for(&self.root, &id);
        let now = self.touch();
        let len = bytes.len() as u64;

        // Already present? Just bump recency, skip the write.
        if path.exists() {
            self.entries
                .borrow_mut()
                .entry(id)
                .and_modify(|e| {
                    e.tick = now;
                    e.size = len;
                })
                .or_insert(IndexEntry {
                    tick: now,
                    size: len,
                });
            return Ok(id);
        }

        // Ensure the shard dir exists, then write atomically via
        // temp-file + rename (so a crash mid-write doesn't leave a
        // half-written file under the canonical path).
        let shard = layout::shard_dir_for(&self.root, &id);
        fs::create_dir_all(&shard)
            .map_err(|e| CacheError::Io(format!("create_dir_all({}): {e}", shard.display())))?;

        let staging = path.with_extension(format!("tmp-{now}"));
        {
            let mut f = fs::File::create(&staging)
                .map_err(|e| CacheError::Io(format!("create({}): {e}", staging.display())))?;
            f.write_all(&bytes)
                .map_err(|e| CacheError::Io(format!("write({}): {e}", staging.display())))?;
            f.sync_all()
                .map_err(|e| CacheError::Io(format!("sync_all({}): {e}", staging.display())))?;
        }
        fs::rename(&staging, &path).map_err(|e| {
            CacheError::Io(format!(
                "rename({} -> {}): {e}",
                staging.display(),
                path.display()
            ))
        })?;

        self.entries.borrow_mut().insert(
            id,
            IndexEntry {
                tick: now,
                size: len,
            },
        );
        Ok(id)
    }

    fn evict_lru(&mut self, max_bytes: u64) -> Result<(), CacheError> {
        let mut total: u64 = self.entries.borrow().values().map(|e| e.size).sum();
        if total <= max_bytes {
            return Ok(());
        }
        // Snapshot ids in ascending tick order — LRU first.
        let mut order: Vec<(AssetId, u64)> = self
            .entries
            .borrow()
            .iter()
            .map(|(id, e)| (*id, e.tick))
            .collect();
        order.sort_by_key(|(_, t)| *t);

        for (id, _) in order {
            if total <= max_bytes {
                break;
            }
            let path = layout::path_for(&self.root, &id);
            // Remove from disk first; if that fails, leave the index
            // entry alone so a retry can pick it up.
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(CacheError::Io(format!(
                        "remove_file({}) during eviction: {e}",
                        path.display()
                    )));
                }
            }
            if let Some(removed) = self.entries.borrow_mut().remove(&id) {
                total = total.saturating_sub(removed.size);
            }
        }
        Ok(())
    }

    fn total_size(&self) -> u64 {
        self.entries.borrow().values().map(|e| e.size).sum()
    }

    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

impl Drop for LocalCache {
    fn drop(&mut self) {
        // Best-effort flush. We can't propagate an error from `Drop`,
        // but a missing index after a crash is recoverable on next
        // open via `reconcile` — every payload is content-addressed
        // and stats can be re-acquired.
        drop(self.flush_index());
    }
}

// ---------------------------------------------------------------------------
// Index file I/O
// ---------------------------------------------------------------------------

/// Format: `<tick> <64-hex> <size>\n`. Plain-ASCII so that a corrupted
/// file is debuggable / hand-fixable.
fn load_index(path: &Path) -> Result<BTreeMap<AssetId, IndexEntry>, CacheError> {
    let mut s = String::new();
    fs::File::open(path)
        .map_err(|e| CacheError::Io(format!("open({}): {e}", path.display())))?
        .read_to_string(&mut s)
        .map_err(|e| CacheError::Io(format!("read({}): {e}", path.display())))?;

    let mut out = BTreeMap::new();
    for (lineno, line) in s.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let tick_s = parts.next().ok_or_else(|| {
            CacheError::Io(format!("index line {} missing tick: `{line}`", lineno + 1))
        })?;
        let hash_s = parts.next().ok_or_else(|| {
            CacheError::Io(format!("index line {} missing hash: `{line}`", lineno + 1))
        })?;
        let size_s = parts.next().ok_or_else(|| {
            CacheError::Io(format!("index line {} missing size: `{line}`", lineno + 1))
        })?;
        let tick: u64 = tick_s.parse().map_err(|e| {
            CacheError::Io(format!(
                "index line {} bad tick `{tick_s}`: {e}",
                lineno + 1
            ))
        })?;
        // Hash form in the index is bare hex — prepend `blake3:` so
        // the AssetId parser does its work uniformly.
        let id_form = format!("blake3:{hash_s}");
        let id: AssetId = id_form.parse().map_err(|e| {
            CacheError::Io(format!(
                "index line {} bad hash `{hash_s}`: {e}",
                lineno + 1
            ))
        })?;
        let size: u64 = size_s.parse().map_err(|e| {
            CacheError::Io(format!(
                "index line {} bad size `{size_s}`: {e}",
                lineno + 1
            ))
        })?;
        out.insert(id, IndexEntry { tick, size });
    }
    Ok(out)
}

fn save_index(path: &Path, entries: &BTreeMap<AssetId, IndexEntry>) -> Result<(), CacheError> {
    let mut s = String::new();
    // BTreeMap iterates in id order — stable across runs, which is
    // what we want for diff-friendly cache state.
    for (id, e) in entries {
        // `writeln!` to a `String` cannot fail — `fmt::Write::write_fmt`
        // returns `Err` only if a formatter callback errors, which the
        // built-in `{}` formatter for `u64` / `&str` never does.
        let _ = writeln!(&mut s, "{} {} {}", e.tick, id.hex(), e.size);
    }
    let staging = path.with_extension("staging");
    {
        let mut f = fs::File::create(&staging)
            .map_err(|err| CacheError::Io(format!("create({}): {err}", staging.display())))?;
        f.write_all(s.as_bytes())
            .map_err(|err| CacheError::Io(format!("write({}): {err}", staging.display())))?;
        f.sync_all()
            .map_err(|err| CacheError::Io(format!("sync_all({}): {err}", staging.display())))?;
    }
    fs::rename(&staging, path)
        .map_err(|err| CacheError::Io(format!("rename({}): {err}", path.display())))?;
    Ok(())
}

/// Drop entries whose backing file is no longer present, and stat any
/// existing payloads we don't have an index entry for. Tick stays
/// where the loaded index left it for entries the index already knew
/// about; freshly-discovered entries get tick 0 (oldest).
fn reconcile(
    root: &Path,
    mut entries: BTreeMap<AssetId, IndexEntry>,
) -> BTreeMap<AssetId, IndexEntry> {
    // Step 1: drop indexed-but-missing.
    entries.retain(|id, _| layout::path_for(root, id).is_file());

    // Step 2: discover unindexed-but-present.
    if let Ok(rd) = fs::read_dir(root) {
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            // shard dirs are exactly two hex chars; ignore anything else.
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.len() != 2 || !name.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            if let Ok(rd2) = fs::read_dir(&p) {
                for sub in rd2.flatten() {
                    let sub_path = sub.path();
                    if !sub_path.is_file() {
                        continue;
                    }
                    let Some(fname) = sub_path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    // 64 lowercase hex chars expected.
                    if fname.len() != 64 || !fname.chars().all(|c| c.is_ascii_hexdigit()) {
                        continue;
                    }
                    let id_str = format!("blake3:{fname}");
                    let id: AssetId = match id_str.parse() {
                        Ok(i) => i,
                        Err(_) => continue,
                    };
                    if entries.contains_key(&id) {
                        continue;
                    }
                    let size = match sub.metadata() {
                        Ok(m) => m.len(),
                        Err(_) => continue,
                    };
                    entries.insert(id, IndexEntry { tick: 0, size });
                }
            }
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn fresh_cache() -> (TempDir, LocalCache) {
        let dir = TempDir::new().expect("tempdir");
        let cache = LocalCache::open(dir.path()).expect("open");
        (dir, cache)
    }

    #[test]
    fn open_creates_root_if_missing() {
        let dir = TempDir::new().expect("tempdir");
        let nested = dir.path().join("does").join("not").join("exist");
        assert!(!nested.exists());
        let _c = LocalCache::open(&nested).expect("open");
        assert!(nested.is_dir());
    }

    #[test]
    fn open_rejects_existing_non_directory() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("a-file");
        fs::write(&file, b"i am a file").unwrap();
        let err = LocalCache::open(&file).expect_err("must fail on a non-dir");
        assert!(matches!(err, CacheError::Io(_)));
    }

    #[test]
    fn put_and_get_round_trip() {
        let (_dir, mut cache) = fresh_cache();
        let id = cache.put(b"hello world".to_vec()).expect("put");
        let got = cache.get(&id).expect("get").expect("present");
        assert_eq!(got, b"hello world");
    }

    #[test]
    fn put_writes_to_two_char_prefix_layout() {
        let (_dir, mut cache) = fresh_cache();
        let id = cache.put(b"on disk".to_vec()).expect("put");
        let expected = layout::path_for(cache.root(), &id);
        assert!(
            expected.is_file(),
            "expected {} to exist",
            expected.display()
        );
        // Parent must be the two-char shard.
        let parent_name = expected
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(parent_name.len(), 2);
        assert!(parent_name.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn put_dedups_repeated_input() {
        let (_dir, mut cache) = fresh_cache();
        let a = cache.put(b"same".to_vec()).expect("a");
        let b = cache.put(b"same".to_vec()).expect("b");
        assert_eq!(a, b);
        assert_eq!(cache.len(), 1, "dedup should keep one entry");
    }

    #[test]
    fn dedup_no_double_storage_one_file_on_disk() {
        let (_dir, mut cache) = fresh_cache();
        let _ = cache.put(b"x".to_vec()).unwrap();
        let _id1 = cache.put(b"x".to_vec()).unwrap();
        let _id2 = cache.put(b"x".to_vec()).unwrap();
        let id = AssetId::from_bytes(b"x");
        let path = layout::path_for(cache.root(), &id);
        assert!(path.is_file());
        // Walk the shard dir — must contain exactly one file.
        let shard = path.parent().unwrap();
        let n = fs::read_dir(shard)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_file())
            .count();
        assert_eq!(n, 1, "shard dir should contain exactly one file");
    }

    #[test]
    fn missing_id_returns_none() {
        let (_dir, cache) = fresh_cache();
        let id = AssetId::from_bytes(b"never inserted");
        assert!(cache.get(&id).expect("not error").is_none());
    }

    #[test]
    fn evict_lru_drops_least_recent_until_under_cap() {
        let (_dir, mut cache) = fresh_cache();
        let id1 = cache.put(vec![0u8; 100]).unwrap();
        let id2 = cache.put(vec![1u8; 100]).unwrap();
        let id3 = cache.put(vec![2u8; 100]).unwrap();
        assert_eq!(cache.total_size(), 300);

        cache.evict_lru(150).unwrap();
        assert!(cache.total_size() <= 150);
        // id3 (most recently inserted) must still be present.
        assert!(cache.get(&id3).unwrap().is_some());
        // id1 (least recent) must be gone.
        assert!(cache.get(&id1).unwrap().is_none());
        // id2's fate depends on whether the cap was met by dropping
        // just id1; with cap 150 and 100-byte entries, dropping id1
        // brings total to 200, still over cap, so id2 must also go.
        assert!(cache.get(&id2).unwrap().is_none());
    }

    #[test]
    fn evict_lru_zero_clears_cache_completely() {
        let (_dir, mut cache) = fresh_cache();
        cache.put(b"a".to_vec()).unwrap();
        cache.put(b"b".to_vec()).unwrap();
        cache.evict_lru(0).unwrap();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.total_size(), 0);
    }

    #[test]
    fn evict_lru_under_cap_is_noop() {
        let (_dir, mut cache) = fresh_cache();
        cache.put(b"x".to_vec()).unwrap();
        let before = cache.total_size();
        cache.evict_lru(1_000_000).unwrap();
        assert_eq!(cache.total_size(), before);
    }

    #[test]
    fn evict_lru_removes_files_from_disk_not_just_index() {
        let (_dir, mut cache) = fresh_cache();
        let id = cache.put(b"goodbye".to_vec()).unwrap();
        let path = layout::path_for(cache.root(), &id);
        assert!(path.is_file());
        cache.evict_lru(0).unwrap();
        assert!(!path.exists(), "evicted file must be removed from disk");
    }

    #[test]
    fn get_after_evict_returns_none() {
        let (_dir, mut cache) = fresh_cache();
        let id = cache.put(b"poof".to_vec()).unwrap();
        cache.evict_lru(0).unwrap();
        assert!(cache.get(&id).unwrap().is_none());
    }

    #[test]
    fn get_bumps_recency_so_evict_keeps_recently_read() {
        let (_dir, mut cache) = fresh_cache();
        let id1 = cache.put(vec![0u8; 100]).unwrap();
        let id2 = cache.put(vec![1u8; 100]).unwrap();
        // Touch id1 — id2 is now older from a recency standpoint.
        drop(cache.get(&id1).unwrap());
        // Cap to 100: must evict id2 (least recent), keep id1.
        cache.evict_lru(100).unwrap();
        assert!(cache.get(&id1).unwrap().is_some(), "id1 was just read");
        assert!(cache.get(&id2).unwrap().is_none(), "id2 must be evicted");
    }

    #[test]
    fn index_persists_across_reopens() {
        let dir = TempDir::new().expect("tempdir");
        let id;
        {
            let mut cache = LocalCache::open(dir.path()).expect("open #1");
            id = cache.put(b"durable".to_vec()).unwrap();
            cache.flush_index().unwrap();
        }
        let c2 = LocalCache::open(dir.path()).expect("open #2");
        assert_eq!(c2.len(), 1);
        let got = c2.get(&id).unwrap().unwrap();
        assert_eq!(got, b"durable");
    }

    #[test]
    fn reopen_without_flush_recovers_via_reconcile() {
        // Simulate "the process was killed before flush_index ran" —
        // delete the index file before reopening. The reconciler must
        // rediscover the on-disk asset.
        let dir = TempDir::new().expect("tempdir");
        let id;
        {
            let mut cache = LocalCache::open(dir.path()).expect("open");
            id = cache.put(b"recoverable".to_vec()).unwrap();
            cache.flush_index().unwrap();
        }
        // Wipe the index file but leave the payload.
        let idx = dir.path().join(INDEX_FILE);
        fs::remove_file(&idx).unwrap();
        assert!(!idx.exists());
        let c2 = LocalCache::open(dir.path()).expect("open #2");
        assert_eq!(c2.len(), 1, "reconciler must rediscover on-disk asset");
        let got = c2.get(&id).unwrap().unwrap();
        assert_eq!(got, b"recoverable");
    }

    #[test]
    fn reconcile_drops_indexed_but_missing_payload() {
        let dir = TempDir::new().expect("tempdir");
        let id;
        {
            let mut cache = LocalCache::open(dir.path()).expect("open");
            id = cache.put(b"will be deleted".to_vec()).unwrap();
            cache.flush_index().unwrap();
        }
        // Wipe payload but keep index.
        let path = layout::path_for(dir.path(), &id);
        fs::remove_file(path).unwrap();
        let c2 = LocalCache::open(dir.path()).expect("reopen");
        assert_eq!(c2.len(), 0, "reconciler must drop entries with no payload");
        assert!(c2.get(&id).unwrap().is_none());
    }

    #[test]
    fn cross_machine_determinism_id_only_depends_on_input() {
        // Same input bytes → same on-disk file path, regardless of
        // which LocalCache instance we use. This is the cross-machine
        // determinism guarantee in concrete form.
        let dir1 = TempDir::new().expect("d1");
        let dir2 = TempDir::new().expect("d2");
        let mut cache1 = LocalCache::open(dir1.path()).expect("cache1");
        let mut cache2 = LocalCache::open(dir2.path()).expect("cache2");
        let id1 = cache1.put(b"determined".to_vec()).unwrap();
        let id2 = cache2.put(b"determined".to_vec()).unwrap();
        assert_eq!(id1, id2, "same bytes must yield same id on two caches");
        // And the path-component within each root is the same.
        let p1 = layout::path_for(cache1.root(), &id1);
        let p2 = layout::path_for(cache2.root(), &id2);
        assert_eq!(
            p1.strip_prefix(cache1.root()).unwrap(),
            p2.strip_prefix(cache2.root()).unwrap()
        );
    }

    #[test]
    fn put_zero_byte_asset_round_trips() {
        let (_dir, mut cache) = fresh_cache();
        let id = cache.put(Vec::new()).unwrap();
        let got = cache.get(&id).unwrap().unwrap();
        assert!(got.is_empty());
        assert_eq!(cache.total_size(), 0);
    }

    #[test]
    fn total_size_matches_sum_of_entry_sizes() {
        let (_dir, mut cache) = fresh_cache();
        cache.put(vec![0u8; 17]).unwrap();
        cache.put(vec![0u8; 23]).unwrap();
        cache.put(vec![0u8; 29]).unwrap();
        assert_eq!(cache.total_size(), 17 + 23 + 29);
    }

    #[test]
    fn put_handles_concurrent_external_delete_gracefully() {
        // Someone (another tool, the user) deletes the payload between
        // open() and get() — get returns None and the index self-heals.
        let (dir, mut cache) = fresh_cache();
        let id = cache.put(b"vulnerable".to_vec()).unwrap();
        let path = layout::path_for(dir.path(), &id);
        fs::remove_file(path).unwrap();
        assert!(
            cache.get(&id).unwrap().is_none(),
            "missing payload must surface as None"
        );
        assert_eq!(cache.len(), 0, "index must self-heal on observed-missing");
    }
}
