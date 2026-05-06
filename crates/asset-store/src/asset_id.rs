//! Re-export of canonical [`AssetId`] from `rge-kernel-asset` (Phase 4.1).
//!
//! The canonical type lives in `kernel/asset`; this module re-exports it so
//! downstream code inside `rge-asset-store` (and any crate that depended on
//! the old `rge_asset_store::AssetId`) continues to resolve the same name.
//!
//! `AssetIdParseError` is also re-exported for callers that match on parse
//! failures.

pub use rge_kernel_asset::{AssetId, AssetIdParseError};
