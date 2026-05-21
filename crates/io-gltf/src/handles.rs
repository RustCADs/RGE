// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Content-hash handles used to refer to assets stored in a [`crate::Cache`].
//!
//! W17-local stubs — when W01 (`components-render`) and W01-extension
//! (`components-animation`) merge their canonical types, callers will swap the
//! `use rge_io_gltf::{MeshHandle, ...}` lines for those upstream paths. These
//! stubs are deliberately shape-identical: a transparent `[u8; 32]` newtype
//! over a blake3 hash, with `to_hex` / `from_hex` for log-friendly display.

use serde::{Deserialize, Serialize};

macro_rules! handle {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[repr(transparent)]
        pub struct $name(pub [u8; 32]);

        impl $name {
            /// Render the handle as 64-char lowercase hex.
            #[must_use]
            pub fn to_hex(self) -> String {
                let mut s = String::with_capacity(64);
                for b in self.0 {
                    use std::fmt::Write as _;
                    let _ = write!(s, "{:02x}", b);
                }
                s
            }

            /// Parse a 64-char lowercase hex string back to a handle.
            #[must_use]
            pub fn from_hex(s: &str) -> Option<Self> {
                if s.len() != 64 { return None; }
                let mut out = [0u8; 32];
                for (i, b) in out.iter_mut().enumerate() {
                    *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
                }
                Some(Self(out))
            }

            /// Build a handle from a slice via blake3 — same algorithm the
            /// `Cache` impls use, so call sites that pre-hash an asset get
            /// stable identity.
            #[must_use]
            pub fn from_bytes(bytes: &[u8]) -> Self {
                Self(*blake3::hash(bytes).as_bytes())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                // All-zero hash is reserved as "unbound" sentinel; production
                // callers should never use Default outside of test scaffolding.
                Self([0u8; 32])
            }
        }
    };
}

handle! {
    /// Stable handle to a mesh asset cached in a [`crate::Cache`].
    MeshHandle
}

handle! {
    /// Stable handle to a material asset cached in a [`crate::Cache`].
    MaterialHandle
}

handle! {
    /// Stable handle to an animation-clip asset cached in a [`crate::Cache`].
    AnimationHandle
}

handle! {
    /// Stable handle to a skeleton asset cached in a [`crate::Cache`].
    SkeletonHandle
}

handle! {
    /// Dispatch L — stable handle to an [`crate::ImageAsset`] cached in a
    /// [`crate::Cache`]. Same shape as the other handles (blake3 of the
    /// asset's canonical byte form). Used by [`crate::MaterialAsset::
    /// base_color_image_handle`] to point at the decoded image bytes
    /// belonging to the material's `base_color_texture` slot.
    ImageHandle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_hex_round_trip() {
        let h1 = MeshHandle::from_bytes(b"hello");
        let s = h1.to_hex();
        assert_eq!(s.len(), 64);
        let h2 = MeshHandle::from_hex(&s).expect("parse");
        assert_eq!(h1, h2);
    }

    #[test]
    fn handle_from_bytes_is_blake3() {
        let h1 = MaterialHandle::from_bytes(b"abc");
        let h2 = MaterialHandle::from_bytes(b"abc");
        let h3 = MaterialHandle::from_bytes(b"abd");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn from_hex_rejects_bad_input() {
        assert!(MeshHandle::from_hex("nope").is_none());
        assert!(MeshHandle::from_hex(&"g".repeat(64)).is_none());
    }

    #[test]
    fn default_is_all_zero() {
        assert_eq!(SkeletonHandle::default().0, [0u8; 32]);
    }
}
