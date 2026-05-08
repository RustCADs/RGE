//! View descriptor types — the read-only-slice vocabulary substrate.
//!
//! v0 ships descriptors only. The eventual implementation (per PLAN.md
//! §1.6.5 / §10.1) maps these descriptors to zero-copy WASM linear-memory
//! slices, but v0 deliberately does NOT expose any actual buffer bytes —
//! see the crate-level NON-GOALS section for the full exclusion list.

use serde::{Deserialize, Serialize};

use crate::id::AssetViewId;

/// Discriminant for the kind of asset view.
///
/// v0 stub: a single placeholder variant. Real view kinds (`MeshVertices` /
/// `MeshIndices` / `Texture2D` / `TessellationOutput` / etc.) land in
/// dedicated future dispatches when concrete consumers and zero-copy
/// machinery exist. Marking `#[non_exhaustive]` preserves the freedom to
/// add variants without breaking downstream consumers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ViewKind {
    /// v0 placeholder; real domain-specific variants land in future dispatches.
    Placeholder,
}

/// Descriptor for a single read-only asset view.
///
/// v0 stub: minimal payload — `id` + `kind` + `byte_len`. Future
/// dispatches may extend with payload bytes / slice offsets / format
/// metadata / lifetime hints, all behind dedicated ADRs.
///
/// `byte_len` is informational in v0 — it captures the eventual slice
/// size so callers can size their reads, but v0 does not enforce buffer
/// presence or validate the length against any backing allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDescriptor {
    /// Unique identifier for this view.
    pub id: AssetViewId,
    /// Discriminant for the kind of view.
    pub kind: ViewKind,
    /// Eventual slice size in bytes. Informational only in v0; future
    /// dispatches may enforce against backing allocations.
    pub byte_len: u64,
}

impl ViewDescriptor {
    /// Construct a [`ViewDescriptor`] from owned components.
    #[must_use]
    pub fn new(id: AssetViewId, kind: ViewKind, byte_len: u64) -> Self {
        Self { id, kind, byte_len }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_constructs_with_explicit_fields() {
        let id = AssetViewId::from_bytes([1u8; 16]);
        let d = ViewDescriptor::new(id, ViewKind::Placeholder, 1024);
        assert_eq!(d.id.as_bytes(), &[1u8; 16]);
        assert_eq!(d.kind, ViewKind::Placeholder);
        assert_eq!(d.byte_len, 1024);
    }

    #[test]
    fn descriptor_serde_round_trip_preserves_all_fields() {
        let d = ViewDescriptor::new(
            AssetViewId::from_bytes([7u8; 16]),
            ViewKind::Placeholder,
            42,
        );
        let json = serde_json::to_string(&d).expect("serialize");
        let decoded: ViewDescriptor = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d, decoded);
    }

    #[test]
    fn descriptor_zero_byte_len_is_legal() {
        // v0 does NOT validate byte_len; zero-length descriptors are
        // permitted (the eventual allocator may reject them, but the
        // substrate is unopinionated).
        let d = ViewDescriptor::new(AssetViewId::from_bytes([0u8; 16]), ViewKind::Placeholder, 0);
        assert_eq!(d.byte_len, 0);
    }

    #[test]
    fn descriptor_max_byte_len_is_legal() {
        let d = ViewDescriptor::new(
            AssetViewId::from_bytes([0u8; 16]),
            ViewKind::Placeholder,
            u64::MAX,
        );
        assert_eq!(d.byte_len, u64::MAX);
    }

    #[test]
    fn view_kind_non_exhaustive_pattern_compiles_via_default_arm() {
        #[allow(
            unreachable_patterns,
            reason = "cross-crate consumer pattern — wildcard required"
        )]
        fn label(k: &ViewKind) -> &'static str {
            match k {
                ViewKind::Placeholder => "placeholder",
                _ => "unknown",
            }
        }
        assert_eq!(label(&ViewKind::Placeholder), "placeholder");
    }
}
