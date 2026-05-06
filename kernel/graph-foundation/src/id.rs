//! Stable, content-derived 128-bit identifiers for graph nodes and edges.
//!
//! Both [`NodeId`] and [`EdgeId`] are derived via BLAKE3 over the node/edge
//! structural payload. Same content → same id, deterministically across
//! processes and platforms.
//!
//! Serde serializes both types as a 32-character lowercase hex string (e.g.
//! `"0000000000000000000000000000000a"`) because RON and many other formats do
//! not natively support `u128`.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// NodeId
// ---------------------------------------------------------------------------

/// Stable, content-derived 128-bit identifier for a graph node.
///
/// Derived via BLAKE3 over the node's structural payload. Same content →
/// same id, deterministically across processes and platforms.
///
/// Serializes as a 32-character lowercase hex string so that formats without
/// native `u128` support (e.g. RON) work correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u128);

impl NodeId {
    /// Compute a [`NodeId`] by hashing the given bytes via BLAKE3.
    ///
    /// # Panics
    ///
    /// Never panics in practice: BLAKE3 output is always 32 bytes and we
    /// take a fixed 16-byte prefix.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        let raw = u128::from_le_bytes(hash.as_bytes()[..16].try_into().expect("16 bytes"));
        Self(raw)
    }

    /// Construct a [`NodeId`] from a raw `u128` (e.g., for tests or migration).
    #[must_use]
    pub const fn from_raw(raw: u128) -> Self {
        Self(raw)
    }

    /// Hex-encode this id as a 32-character lowercase hex string.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node:0x{:032x}", self.0)
    }
}

// Serde: serialize as hex string.
impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:032x}", self.0))
    }
}

struct NodeIdVisitor;

impl Visitor<'_> for NodeIdVisitor {
    type Value = NodeId;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a 32-character lowercase hex string for NodeId")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<NodeId, E> {
        u128::from_str_radix(v, 16)
            .map(NodeId)
            .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(NodeIdVisitor)
    }
}

// ---------------------------------------------------------------------------
// EdgeId
// ---------------------------------------------------------------------------

/// Stable, content-derived 128-bit identifier for a graph edge.
///
/// Derived via BLAKE3 over the edge's structural payload. Same content →
/// same id, deterministically across processes and platforms.
///
/// Serializes as a 32-character lowercase hex string so that formats without
/// native `u128` support (e.g. RON) work correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeId(pub u128);

impl EdgeId {
    /// Compute an [`EdgeId`] by hashing the given bytes via BLAKE3.
    ///
    /// # Panics
    ///
    /// Never panics in practice: BLAKE3 output is always 32 bytes and we
    /// take a fixed 16-byte prefix.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        let raw = u128::from_le_bytes(hash.as_bytes()[..16].try_into().expect("16 bytes"));
        Self(raw)
    }

    /// Construct an [`EdgeId`] from a raw `u128` (e.g., for tests or migration).
    #[must_use]
    pub const fn from_raw(raw: u128) -> Self {
        Self(raw)
    }

    /// Hex-encode this id as a 32-character lowercase hex string.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }

    /// Convenience: derive an [`EdgeId`] from `(src, dst)` endpoint pair.
    ///
    /// Mixes both endpoint u128 values so different `(src, dst)` orderings
    /// produce different ids.
    #[must_use]
    pub fn from_endpoints(src: NodeId, dst: NodeId) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..16].copy_from_slice(&src.0.to_le_bytes());
        bytes[16..].copy_from_slice(&dst.0.to_le_bytes());
        Self::from_bytes(&bytes)
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "edge:0x{:032x}", self.0)
    }
}

// Serde: serialize as hex string.
impl Serialize for EdgeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:032x}", self.0))
    }
}

struct EdgeIdVisitor;

impl Visitor<'_> for EdgeIdVisitor {
    type Value = EdgeId;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a 32-character lowercase hex string for EdgeId")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<EdgeId, E> {
        u128::from_str_radix(v, 16)
            .map(EdgeId)
            .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
    }
}

impl<'de> Deserialize<'de> for EdgeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(EdgeIdVisitor)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_blake3_deterministic() {
        let a = NodeId::from_bytes(b"hello world");
        let b = NodeId::from_bytes(b"hello world");
        assert_eq!(a, b, "same input bytes must produce same NodeId");
    }

    #[test]
    fn node_id_different_input_different_id() {
        let a = NodeId::from_bytes(b"hello");
        let b = NodeId::from_bytes(b"world");
        assert_ne!(a, b, "different input bytes must produce different NodeId");
    }

    #[test]
    fn edge_id_blake3_deterministic() {
        let a = EdgeId::from_bytes(b"edge-payload");
        let b = EdgeId::from_bytes(b"edge-payload");
        assert_eq!(a, b, "same input bytes must produce same EdgeId");
    }

    #[test]
    fn edge_id_from_endpoints_uses_both() {
        let src = NodeId::from_raw(1);
        let dst = NodeId::from_raw(2);
        let e1 = EdgeId::from_endpoints(src, dst);
        let e2 = EdgeId::from_endpoints(dst, src); // reversed
        assert_ne!(e1, e2, "swapped endpoints must produce different EdgeId");
    }

    #[test]
    fn node_id_display() {
        let id = NodeId::from_raw(0xdead_beef);
        let s = format!("{id}");
        assert!(
            s.starts_with("node:0x"),
            "display should start with 'node:0x'"
        );
        assert_eq!(s.len(), "node:0x".len() + 32);
    }

    #[test]
    fn edge_id_display() {
        let id = EdgeId::from_raw(0xdead_beef);
        let s = format!("{id}");
        assert!(
            s.starts_with("edge:0x"),
            "display should start with 'edge:0x'"
        );
        assert_eq!(s.len(), "edge:0x".len() + 32);
    }

    #[test]
    fn node_id_to_hex_len() {
        let id = NodeId::from_raw(u128::MAX);
        assert_eq!(id.to_hex().len(), 32);
    }

    #[test]
    fn node_id_serde_round_trip() {
        let id = NodeId::from_raw(0xdead_beef_cafe_babe);
        let json = serde_json::to_string(&id).unwrap();
        let back: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn edge_id_serde_round_trip() {
        let id = EdgeId::from_raw(0x1234_5678_9abc_def0);
        let json = serde_json::to_string(&id).unwrap();
        let back: EdgeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
