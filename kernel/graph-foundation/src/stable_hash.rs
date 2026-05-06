//! Generic interface for content-deriving stable 128-bit hashes.
//!
//! Implementors describe how to feed their structural fields into a BLAKE3
//! hasher. The trait is considered sealed at v1 — graph systems should call
//! [`stable_node_id`] / [`stable_edge_id`] free functions rather than
//! implementing this trait directly until the API stabilises.

use crate::id::{EdgeId, NodeId};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Generic interface for content-deriving stable 128-bit hashes.
///
/// Implement this trait by feeding all structural fields into the hasher in
/// deterministic (field-declaration) order. Skipping fields or using
/// nondeterministic iteration order will produce nondeterministic ids.
///
/// # Example
///
/// ```rust
/// use rge_kernel_graph_foundation::StableHash;
///
/// struct MyNode {
///     kind: u8,
///     name: String,
/// }
///
/// impl StableHash for MyNode {
///     fn hash_into(&self, hasher: &mut blake3::Hasher) {
///         hasher.update(&[self.kind]);
///         hasher.update(self.name.as_bytes());
///     }
/// }
/// ```
pub trait StableHash {
    /// Feed all structural fields into the hasher in deterministic order.
    fn hash_into(&self, hasher: &mut blake3::Hasher);
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Derive a [`NodeId`] from any value that implements [`StableHash`].
///
/// # Panics
///
/// Never panics in practice: BLAKE3 output is always 32 bytes and we
/// take a fixed 16-byte prefix.
#[must_use]
pub fn stable_node_id<T: StableHash>(value: &T) -> NodeId {
    let mut hasher = blake3::Hasher::new();
    value.hash_into(&mut hasher);
    let hash = hasher.finalize();
    let raw = u128::from_le_bytes(hash.as_bytes()[..16].try_into().expect("16 bytes"));
    NodeId::from_raw(raw)
}

/// Derive an [`EdgeId`] from any value that implements [`StableHash`].
///
/// # Panics
///
/// Never panics in practice: BLAKE3 output is always 32 bytes and we
/// take a fixed 16-byte prefix.
#[must_use]
pub fn stable_edge_id<T: StableHash>(value: &T) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    value.hash_into(&mut hasher);
    let hash = hasher.finalize();
    let raw = u128::from_le_bytes(hash.as_bytes()[..16].try_into().expect("16 bytes"));
    EdgeId::from_raw(raw)
}

// ---------------------------------------------------------------------------
// Blanket impls for primitive types (convenience)
// ---------------------------------------------------------------------------

impl StableHash for u8 {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&[*self]);
    }
}

impl StableHash for u32 {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&self.to_le_bytes());
    }
}

impl StableHash for u64 {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&self.to_le_bytes());
    }
}

impl StableHash for u128 {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&self.to_le_bytes());
    }
}

impl StableHash for str {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        // Hash the length prefix so "ab"+"c" != "a"+"bc".
        hasher.update(&(self.len() as u64).to_le_bytes());
        hasher.update(self.as_bytes());
    }
}

impl StableHash for String {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        self.as_str().hash_into(hasher);
    }
}

impl StableHash for [u8] {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&(self.len() as u64).to_le_bytes());
        hasher.update(self);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct Point {
        x: u32,
        y: u32,
    }

    impl StableHash for Point {
        fn hash_into(&self, hasher: &mut blake3::Hasher) {
            self.x.hash_into(hasher);
            self.y.hash_into(hasher);
        }
    }

    #[test]
    fn stable_node_id_deterministic() {
        let p = Point { x: 1, y: 2 };
        let id1 = stable_node_id(&p);
        let id2 = stable_node_id(&p);
        assert_eq!(id1, id2, "stable_node_id must be deterministic");
    }

    #[test]
    fn stable_node_id_different_values() {
        let p1 = Point { x: 1, y: 2 };
        let p2 = Point { x: 3, y: 4 };
        assert_ne!(stable_node_id(&p1), stable_node_id(&p2));
    }

    #[test]
    fn stable_edge_id_deterministic() {
        let p = Point { x: 10, y: 20 };
        let id1 = stable_edge_id(&p);
        let id2 = stable_edge_id(&p);
        assert_eq!(id1, id2, "stable_edge_id must be deterministic");
    }
}
