//! `rge-kernel-asset-view` — read-only asset view descriptor substrate.
//!
//! Failure class: recoverable
//!
//! Implements the asset-view substrate listed in PLAN.md §1.6.5 / §10.1
//! alongside `kernel/io-scheduler`, `kernel/job-system`, and
//! `kernel/asset-streaming`. PLAN frames the eventual implementation as
//! "zero-copy WASM linear-memory mapping" exposing read-only slices of
//! GPU-ready buffers (mesh vertices, texel data, cad-core tessellation
//! output) directly to WASM linear memory; v0 ships only the descriptor +
//! ID + registry vocabulary substrate without any actual buffer mapping.
//!
//! # NON-GOALS
//!
//! v0 establishes vocabulary and ownership boundaries; it deliberately does
//! NOT establish behaviour richness. The strongest part of this crate's v0
//! is the list of what it intentionally is **not**:
//!
//! - No WASM linear-memory mapping. The PLAN §1.6.5 listed feature is NOT
//!   implemented here; v0 ships descriptors only.
//! - No `unsafe` zero-copy slice exposure. The eventual implementation will
//!   carry its own safety boundary in a dedicated dispatch with the relevant
//!   `unsafe` blocks audited and ADR'd.
//! - No buffer / allocation ownership. Descriptors point at imaginary
//!   backing storage; the real buffers live elsewhere (eventual
//!   `kernel/asset` / `kernel/gfx` / `kernel/asset-streaming` consumers).
//! - No residency / streaming policy — that's `kernel/asset-streaming`.
//! - No GPU upload semantics — that's `kernel/gfx` / future
//!   `kernel/gpu-resources`.
//! - No I/O scheduling priority — that's `kernel/io-scheduler`.
//! - No work scheduling — that's `kernel/job-system`.
//! - No closures / callbacks / observers. The registry is passive lookup;
//!   consumers poll via [`AssetViewRegistry::lookup`] /
//!   [`AssetViewRegistry::iter`].
//! - No `kernel/asset` integration. Callers route ID generation; v0 does
//!   NOT tie [`AssetViewId`] to `kernel/asset::AssetId`.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.
//!
//! # What this crate is
//!
//! Vocabulary, ownership boundaries, and future-safe seams. Future
//! dispatches extend this substrate incrementally without undoing the
//! foundational choices made here: the view kind enum is `#[non_exhaustive]`
//! so new variants may be added (`MeshVertices` / `MeshIndices` / `Texture2D`
//! / `TessellationOutput` / etc.); the registry is `BTreeMap`-backed so
//! iteration is deterministic and reproducible; `AssetViewId` is opaque so
//! the eventual derivation strategy (BLAKE3 of `(asset_id, view_kind,
//! byte_offset, byte_len)` or similar) does not require a public API change.

pub mod descriptor;
pub mod id;
pub mod registry;

pub use descriptor::{ViewDescriptor, ViewKind};
pub use id::AssetViewId;
pub use registry::AssetViewRegistry;

#[cfg(test)]
mod smoke {
    use super::*;

    /// End-to-end: construct registry, register two descriptors, look up
    /// each by id, unregister one, assert the other survives.
    #[test]
    fn registry_round_trip_register_lookup_unregister() {
        let mut r = AssetViewRegistry::new();

        let id_a = AssetViewId::from_bytes([0xaa; 16]);
        let id_b = AssetViewId::from_bytes([0xbb; 16]);

        r.register(ViewDescriptor::new(id_a, ViewKind::Placeholder, 128));
        r.register(ViewDescriptor::new(id_b, ViewKind::Placeholder, 256));
        assert_eq!(r.len(), 2);

        let found_a = r.lookup(id_a).expect("a registered");
        assert_eq!(found_a.byte_len, 128);

        let removed_b = r.unregister(id_b).expect("b removed");
        assert_eq!(removed_b.byte_len, 256);

        // a still present, b gone.
        assert!(r.contains(id_a));
        assert!(!r.contains(id_b));
        assert_eq!(r.len(), 1);
    }
}
