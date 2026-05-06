// adapted from rustforge::crates::persistence on 2026-05-05 — content-addressed cache for general assets

//! `rge-asset-store` — content-addressed local cache for general assets.
//!
//! Implements PLAN §1.2.4 (zero-copy asset views — this crate is the
//! resolve-the-bytes seam) and §1.6.3 (cooked binary — the cooker
//! stores its outputs here, keyed by their own content).
//!
//! # Module map
//!
//! - [`asset_id`] — the [`AssetId`] type, BLAKE3-keyed and string-form
//!   serializable. Canonical owner; W14 (`rge-data`) re-exports.
//! - [`layout`] — filesystem layout helpers (`<root>/<2-char>/<full>`).
//! - [`cache`] — the [`Cache`] trait other crates stub against, plus
//!   an in-memory test fake.
//! - [`local`] — [`LocalCache`], the production filesystem-backed
//!   implementation.
//! - [`dependency`] — [`DepGraph`] for tracking invalidation-cascade
//!   edges between assets.
//!
//! # Quick start
//!
//! ```ignore
//! use rge_asset_store::{Cache, LocalCache, layout};
//!
//! let mut cache = LocalCache::open(layout::default_cache_root()).unwrap();
//! let id = cache.put(b"my asset bytes".to_vec()).unwrap();
//! let bytes = cache.get(&id).unwrap().expect("present");
//! assert_eq!(bytes, b"my asset bytes");
//! ```

#![forbid(unsafe_code)]

pub mod asset_id;
pub mod cache;
pub mod dependency;
pub mod layout;
pub mod local;

pub use asset_id::{AssetId, AssetIdParseError};
pub use cache::{Bytes, Cache, InMemoryCache};
pub use dependency::{DepError, DepGraph};
pub use local::LocalCache;

// ---------------------------------------------------------------------------
// CacheError
// ---------------------------------------------------------------------------

/// Errors emitted by the cache trait and its filesystem-backed impl.
///
/// All variants carry owned strings so the type is `Clone + Eq` —
/// matches the workspace pattern (see `rustforge::persistence`'s
/// `AuditStoreError`) for errors that may ride RPC boundaries cleanly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CacheError {
    /// Filesystem I/O failure (read, write, sync, rename, mkdir).
    /// The wrapped string carries the original `std::io::Error`'s
    /// `Display`, plus the path that triggered it where helpful.
    #[error("asset_store: io error: {0}")]
    Io(String),
    /// An asset id reference (e.g. inside the `.index` recency file
    /// or a dependency-graph load) failed to parse.
    #[error("asset_store: bad asset id: {0}")]
    BadAssetId(String),
}

impl From<std::io::Error> for CacheError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<AssetIdParseError> for CacheError {
    fn from(e: AssetIdParseError) -> Self {
        Self::BadAssetId(e.to_string())
    }
}
