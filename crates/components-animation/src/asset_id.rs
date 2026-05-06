//! Re-export of canonical [`AssetId`] from `rge-kernel-asset` (Phase 4.1).
//!
//! The W01 local stub (`AssetId(pub u64)`) documented its own replacement by
//! `rge-kernel-asset::AssetId` (W14). This module performs that replacement.
//!
//! # NULL sentinel (Option A)
//!
//! `rge-kernel-asset::AssetId` deliberately omits a `Default` / `NULL` value
//! because content-addressed IDs have no meaningful "zero" value. For
//! backward-compatibility with component structs that need a placeholder,
//! [`NULL_ASSET_ID`] is provided as an all-zeros 32-byte digest (all-zeros is
//! astronomically unlikely to collide with any real content).
//!
//! For new fields, prefer `Option<AssetId>` to express "may be unset"
//! naturally.

pub use rge_kernel_asset::AssetId;

/// Sentinel "no asset bound yet" value — all-zero 32-byte digest.
///
/// Equivalent to the former `AssetId::NULL = AssetId(0)` on the W01 stub.
/// `rge-kernel-asset::AssetId` omits a sentinel by design; this constant
/// bridges the gap for existing component fields.
pub const NULL_ASSET_ID: AssetId = AssetId::from_raw([0u8; 32]);
