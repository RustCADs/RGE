// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets
//
// Persistence's `open(path)` resolves a single SQLite file; the W16 cache
// instead resolves a *directory tree*, but the layering choice — a single
// well-known root with parent-dir auto-create on first write — is the same.

//! Filesystem layout for the on-disk cache.
//!
//! The convention is `<root>/<2-char-prefix>/<full-64-hex>` where
//! `<root>` is `~/.cache/rge/assets/` by default. The two-character shard
//! distributes a million-asset cache across 256 directories instead of
//! crushing one — many filesystems (NTFS, ext4) slow noticeably above
//! ~10k entries per directory.
//!
//! # Why filesystem rather than `SQLite` blobs
//!
//! Three reasons:
//! 1. The cache holds **bytes-already-on-disk** — there is nothing for
//!    `SQLite` to add (no joins, no transactions across multiple blobs).
//! 2. We want OS-level page cache to do its job. `SQLite` blob columns
//!    bypass that.
//! 3. The future `kernel/asset-streaming` (PLAN §1.2.4 zero-copy asset
//!    views) will mmap files directly — no copy through SQL.
//!
//! Persistence's audit ledger (the rustforge crate this is adapted from)
//! does use `SQLite`, because it has different requirements (queryable,
//! transactional, mutable index). The decision tree splits there.

use std::path::{Path, PathBuf};

use crate::AssetId;

/// Default cache root: `<user-cache-dir>/rge/assets/`.
///
/// On first call this resolves the platform's user-cache directory
/// (`~/.cache` on Linux, `%LOCALAPPDATA%` on Windows, `~/Library/Caches`
/// on macOS) without depending on the `dirs` crate (kept out of the dep
/// graph at the workspace level — see `Cargo.toml` workspace deps list).
///
/// Falls back to `./.rge-cache/` if every environment variable lookup
/// fails. The fallback is deterministic so tests on a stripped-down
/// container can still resolve a cache path.
#[must_use]
pub fn default_cache_root() -> PathBuf {
    if let Some(p) = platform_user_cache_dir() {
        p.join("rge").join("assets")
    } else {
        PathBuf::from(".rge-cache")
    }
}

/// Platform-specific user-cache directory. Hand-rolled so this crate
/// doesn't pull in `dirs`/`directories` (kept out of the workspace
/// `[workspace.dependencies]` list deliberately at v0.8).
fn platform_user_cache_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        // %LOCALAPPDATA% is the canonical caches root on Windows.
        // (`Local`/`Caches` would be a sub-namespace — keep simple.)
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.is_empty() {
                return Some(PathBuf::from(local));
            }
        }
        // Last-ditch: USERPROFILE\AppData\Local.
        if let Ok(profile) = std::env::var("USERPROFILE") {
            if !profile.is_empty() {
                return Some(PathBuf::from(profile).join("AppData").join("Local"));
            }
        }
        None
    } else if cfg!(target_os = "macos") {
        // ~/Library/Caches per Apple convention.
        std::env::var("HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|h| PathBuf::from(h).join("Library").join("Caches"))
    } else {
        // XDG: $XDG_CACHE_HOME, else $HOME/.cache.
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg));
            }
        }
        std::env::var("HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|h| PathBuf::from(h).join(".cache"))
    }
}

/// First two hex characters of an [`AssetId`] digest.
///
/// Used as the directory-shard prefix so a cache growing to ~1 M assets
/// distributes across 256 directories rather than crushing one. Matches
/// the convention that was previously exposed as `AssetId::two_char_prefix`
/// on the old local type; pulled into a free function here so the canonical
/// `rge_kernel_asset::AssetId` (which has no filesystem-layout knowledge)
/// stays clean.
#[must_use]
fn two_char_prefix(id: &AssetId) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let b = id.raw()[0];
    let mut out = String::with_capacity(2);
    out.push(HEX[(b >> 4) as usize] as char);
    out.push(HEX[(b & 0x0f) as usize] as char);
    out
}

/// Compute the on-disk path for an asset relative to a given cache root.
///
/// The path is `<root>/<two-char-prefix>/<full-64-hex>`. No I/O is
/// performed; this is a pure function on the `AssetId` hex form.
#[must_use]
pub fn path_for(root: &Path, id: &AssetId) -> PathBuf {
    root.join(two_char_prefix(id)).join(id.hex())
}

/// The shard subdirectory (relative path under root). Useful for callers
/// that want to enumerate / lock a single shard without walking the full
/// hex path.
#[must_use]
pub fn shard_dir_for(root: &Path, id: &AssetId) -> PathBuf {
    root.join(two_char_prefix(id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_for_uses_two_char_prefix_then_full_hex() {
        let root = PathBuf::from("/tmp/cache");
        let id = AssetId::from_bytes(b"layout-test");
        let p = path_for(&root, &id);

        // Component layout: <root> / <2-char prefix> / <full hex>.
        let comps: Vec<_> = p.components().map(|c| c.as_os_str().to_owned()).collect();
        // Last component is the full hex digest.
        assert_eq!(comps.last().unwrap().to_string_lossy(), id.hex());
        // Second-to-last is the prefix.
        let nth_back = comps.len() - 2;
        assert_eq!(comps[nth_back].to_string_lossy(), two_char_prefix(&id),);
    }

    #[test]
    fn path_for_is_deterministic() {
        // Same id + same root → same path, every time. The cache leans
        // on this for read-after-write consistency.
        let root = PathBuf::from("/var/cache/rge");
        let id = AssetId::from_bytes(b"determinism");
        let a = path_for(&root, &id);
        let b = path_for(&root, &id);
        assert_eq!(a, b);
    }

    #[test]
    fn shard_dir_strips_only_the_filename() {
        let root = PathBuf::from("/cache");
        let id = AssetId::from_bytes(b"shard-strip");
        let full = path_for(&root, &id);
        let shard = shard_dir_for(&root, &id);
        assert_eq!(full.parent().expect("parent"), shard.as_path());
    }

    #[test]
    fn default_cache_root_returns_some_path() {
        // On any reasonable host, *one* of LOCALAPPDATA / HOME /
        // XDG_CACHE_HOME is set — and if none of them are, the fallback
        // path `./.rge-cache` is non-empty. So this never returns ""
        // and never panics.
        let p = default_cache_root();
        assert!(!p.as_os_str().is_empty(), "got empty path");
    }

    #[test]
    fn two_assets_share_shard_iff_first_byte_matches() {
        // Two ids with the same first byte share the same shard
        // directory — exercise the property test that the shard is a
        // function only of the first byte.
        let id1 = AssetId::from_raw([0xab; 32]);
        let id2 = AssetId::from_raw({
            let mut b = [0u8; 32];
            b[0] = 0xab;
            b
        });
        let id3 = AssetId::from_raw([0xcd; 32]);
        let root = PathBuf::from("/r");
        assert_eq!(shard_dir_for(&root, &id1), shard_dir_for(&root, &id2));
        assert_ne!(shard_dir_for(&root, &id1), shard_dir_for(&root, &id3));
    }
}
