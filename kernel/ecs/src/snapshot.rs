//! Deterministic snapshot/restore for registered [`SnapshotComponent`]s.
//!
//! # Design
//!
//! Snapshot is **opt-in**: only component types explicitly registered via
//! [`World::register_snapshot_component`] are included. Transient handles
//! (GPU resources, file descriptors) implement [`Component`] but not
//! [`SnapshotComponent`] — they are silently skipped.
//!
//! # Wire format (version 2)
//!
//! ```text
//! magic:           [u8; 4]   = b"RGES"
//! version:         u16 LE    = 2
//! entity_count:    u32 LE
//! for each entity (sorted by EntityId / ULID u128 ascending):
//!   entity_id:     u128 LE
//!   comp_count:    u32 LE
//!   for each component (sorted by snapshot_name() ascending):
//!     name_len:    u32 LE
//!     name_bytes:  [u8; name_len]
//!     payload_len: u32 LE
//!     payload:     [u8; payload_len]   (postcard-encoded component value)
//! ```
//!
//! Version history: v1 used RON for the per-component payload. v2 switched
//! the payload to postcard (compact binary serde format) for ~5–10× size
//! reduction; the framing (magic + LE integers + name + payload-len) is
//! unchanged. v1 snapshots are not readable by v2 — bump-only migration.
//!
//! All integers are little-endian. The entity sort is over the raw `u128`
//! ULID value, which is monotonically increasing, giving a stable order.
//! The component sort is lexicographic over `snapshot_name()` bytes.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::component::Component;
use crate::entity::EntityId;
use crate::world::World;

// ---------------------------------------------------------------------------
// SnapshotComponent trait
// ---------------------------------------------------------------------------

/// Marker trait extending [`Component`] with serde requirements.
///
/// Register the component with [`World::register_snapshot_component`] before
/// any [`World::serialize_snapshot`] call. Components without registration
/// are silently skipped during snapshot — this is intentional per PLAN §6.13's
/// "selective serialization" model.
///
/// # Example
///
/// ```rust
/// # use serde::{Serialize, Deserialize};
/// # use rge_kernel_ecs::{Component, snapshot::SnapshotComponent};
/// #[derive(Serialize, Deserialize)]
/// struct Health(f32);
/// impl Component for Health {}
/// impl SnapshotComponent for Health {}
/// ```
pub trait SnapshotComponent: Component + Serialize + DeserializeOwned {
    /// Stable name for this component in the snapshot stream.
    ///
    /// Determines sort order and acts as the type identifier in the snapshot
    /// bytes. Override for migration / cross-version compatibility; by default
    /// returns [`std::any::type_name::<Self>()`].
    fn snapshot_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

// ---------------------------------------------------------------------------
// SnapshotError
// ---------------------------------------------------------------------------

/// Errors produced by snapshot serialization and deserialization.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// A serde (postcard) error during encode or decode.
    #[error("serde error: {0}")]
    Serde(String),

    /// The snapshot bytes do not start with the expected `RGES` magic.
    #[error("invalid magic bytes: expected 'RGES', got {0:?}")]
    BadMagic([u8; 4]),

    /// The snapshot was written with an unsupported format version.
    #[error("unsupported snapshot version: {0}")]
    BadVersion(u16),

    /// The byte stream ended unexpectedly.
    #[error("truncated snapshot at offset {0}")]
    Truncated(usize),

    /// The snapshot contains a component type that is not registered on the
    /// target `World`. The restore path emits a `tracing::warn` and skips
    /// unregistered components rather than failing, so this variant is only
    /// produced when the caller explicitly opts into strict mode.
    #[error("unknown component type `{0}` in snapshot — register on the target World")]
    UnknownComponent(String),
}

// ---------------------------------------------------------------------------
// Type-erased function pointers used by the snapshot registry
// ---------------------------------------------------------------------------

/// Serialize `any` (which is a `&C`) to postcard bytes.
type SnapshotSerializeFn = fn(any: &(dyn Any + Send + Sync)) -> Result<Vec<u8>, SnapshotError>;

/// Deserialize postcard bytes into a `Box<dyn Any + Send + Sync>` holding a `C`.
type SnapshotDeserializeFn = fn(bytes: &[u8]) -> Result<Box<dyn Any + Send + Sync>, SnapshotError>;

/// Bundle of type-erased snapshot functions for one component type.
pub(crate) struct SnapshotFns {
    /// Type-erased serialize function.
    pub(crate) serialize: SnapshotSerializeFn,
    /// Type-erased deserialize function.
    pub(crate) deserialize: SnapshotDeserializeFn,
    /// Stable component name used as the key in the snapshot stream.
    pub(crate) name: &'static str,
}

// ---------------------------------------------------------------------------
// Concrete implementations of the type-erased functions
// ---------------------------------------------------------------------------

fn make_serialize<C: SnapshotComponent>() -> SnapshotSerializeFn {
    |any| {
        let c = any
            .downcast_ref::<C>()
            .ok_or_else(|| SnapshotError::Serde("downcast failed in serialize".into()))?;
        postcard::to_allocvec(c).map_err(|e| SnapshotError::Serde(e.to_string()))
    }
}

fn make_deserialize<C: SnapshotComponent>() -> SnapshotDeserializeFn {
    |bytes| {
        let c: C =
            postcard::from_bytes::<C>(bytes).map_err(|e| SnapshotError::Serde(e.to_string()))?;
        Ok(Box::new(c) as Box<dyn Any + Send + Sync>)
    }
}

// ---------------------------------------------------------------------------
// World snapshot API
// ---------------------------------------------------------------------------

/// Magic bytes at the start of every snapshot.
const MAGIC: &[u8; 4] = b"RGES";
/// Current format version.
const VERSION: u16 = 2;

impl World {
    /// Register a component type for snapshot inclusion.
    ///
    /// Subsequent [`serialize_snapshot`](Self::serialize_snapshot) calls will
    /// include this component's data. Calling twice for the same `C` is
    /// idempotent.
    pub fn register_snapshot_component<C: SnapshotComponent>(&mut self) {
        self.snapshot_fns
            .entry(TypeId::of::<C>())
            .or_insert_with(|| SnapshotFns {
                serialize: make_serialize::<C>(),
                deserialize: make_deserialize::<C>(),
                name: C::snapshot_name(),
            });
    }

    /// Capture a deterministic, byte-identical snapshot of all registered
    /// components across all live entities.
    ///
    /// Entities are serialized in ascending [`EntityId`] (ULID `u128`) order.
    /// Components within each entity are serialized in ascending
    /// `snapshot_name()` lexicographic order. Both orderings are stable across
    /// runs, ensuring byte-identical output for identical logical state.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError::Serde`] when postcard serialization fails for
    /// any registered component.
    pub fn serialize_snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        // Collect entities sorted by EntityId (ULID u128 ascending).
        let mut sorted_entities: Vec<EntityId> = self.entity_map.keys().copied().collect();
        sorted_entities.sort_by_key(|e| e.ulid().0);

        // Build a name → SnapshotFns lookup sorted by name for determinism.
        // We index by TypeId in the registry but sort by name for the wire
        // format so that payload order matches name sort order.
        let mut name_sorted: Vec<(&TypeId, &SnapshotFns)> = self.snapshot_fns.iter().collect();
        name_sorted.sort_by_key(|(_, fns)| fns.name);

        let mut buf: Vec<u8> =
            Vec::with_capacity(6 + 4 + sorted_entities.len() * (16 + 4 + name_sorted.len() * 32));

        // Header.
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        buf.extend_from_slice(&(sorted_entities.len() as u32).to_le_bytes());

        for entity in &sorted_entities {
            let Some(loc) = self.entity_map.get(entity) else {
                continue;
            };
            let arch = &self.archetypes[loc.archetype_index];

            // Collect components this entity has, filtered to registered
            // snapshot types. Sort by name for determinism.
            let mut entity_comps: Vec<(&'static str, Vec<u8>)> = Vec::new();
            for (type_id, fns) in &name_sorted {
                if let Some(any_ref) = arch.get_erased(**type_id, loc.row) {
                    match (fns.serialize)(any_ref) {
                        Ok(payload) => {
                            entity_comps.push((fns.name, payload));
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            // Write entity header.
            buf.extend_from_slice(&entity.ulid().0.to_le_bytes());
            #[allow(clippy::cast_possible_truncation)]
            buf.extend_from_slice(&(entity_comps.len() as u32).to_le_bytes());

            for (name, payload) in entity_comps {
                let name_bytes = name.as_bytes();
                #[allow(clippy::cast_possible_truncation)]
                buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(name_bytes);
                #[allow(clippy::cast_possible_truncation)]
                buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                buf.extend_from_slice(&payload);
            }
        }

        Ok(buf)
    }

    /// Restore from a snapshot. Despawns all current entities first (clean
    /// slate), then re-spawns each entity from the snapshot stream and inserts
    /// the registered components.
    ///
    /// Components in the stream whose type is not registered on this `World`
    /// are skipped with a [`tracing::warn`] for visibility.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError::BadMagic`], [`SnapshotError::BadVersion`], or
    /// [`SnapshotError::Truncated`] on malformed input. Returns
    /// [`SnapshotError::Serde`] when postcard deserialization fails.
    pub fn restore_from_snapshot(&mut self, bytes: &[u8]) -> Result<(), SnapshotError> {
        let mut pos = 0usize;

        macro_rules! read_bytes {
            ($n:expr) => {{
                let end = pos + $n;
                if end > bytes.len() {
                    return Err(SnapshotError::Truncated(pos));
                }
                let slice = &bytes[pos..end];
                pos = end;
                slice
            }};
        }

        macro_rules! read_u32 {
            () => {{
                let b = read_bytes!(4);
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }};
        }

        macro_rules! read_u16 {
            () => {{
                let b = read_bytes!(2);
                u16::from_le_bytes([b[0], b[1]])
            }};
        }

        macro_rules! read_u128 {
            () => {{
                let b = read_bytes!(16);
                let mut arr = [0u8; 16];
                arr.copy_from_slice(b);
                u128::from_le_bytes(arr)
            }};
        }

        // Validate magic.
        let magic_bytes = read_bytes!(4);
        let mut magic = [0u8; 4];
        magic.copy_from_slice(magic_bytes);
        if &magic != MAGIC {
            return Err(SnapshotError::BadMagic(magic));
        }

        // Validate version.
        let version = read_u16!();
        if version != VERSION {
            return Err(SnapshotError::BadVersion(version));
        }

        // Clean slate.
        let all_entities: Vec<EntityId> = self.entity_map.keys().copied().collect();
        for e in all_entities {
            self.despawn(e);
        }

        // Build name → (TypeId, deserializeFn) lookup from registered fns.
        let name_to_fns: BTreeMap<&str, (TypeId, SnapshotDeserializeFn)> = self
            .snapshot_fns
            .iter()
            .map(|(tid, fns)| (fns.name, (*tid, fns.deserialize)))
            .collect();

        let entity_count = read_u32!() as usize;

        for _ in 0..entity_count {
            let raw_ulid = read_u128!();
            let ulid = ulid::Ulid(raw_ulid);
            let entity_id = EntityId::from_ulid(ulid);

            // Spawn entity with the original EntityId preserved.
            // We use spawn() and then remap — but the kernel spawn() generates
            // a new ULID. We need to preserve the original ID for round-trip
            // correctness. Use the internal spawn_with_id helper.
            self.spawn_with_id(entity_id);

            let comp_count = read_u32!() as usize;

            for _ in 0..comp_count {
                let name_len = read_u32!() as usize;
                let name_bytes = read_bytes!(name_len);
                let name = std::str::from_utf8(name_bytes)
                    .map_err(|e| SnapshotError::Serde(e.to_string()))?;

                let payload_len = read_u32!() as usize;
                let payload = read_bytes!(payload_len);

                match name_to_fns.get(name) {
                    Some((type_id, deserialize)) => {
                        let boxed = deserialize(payload)?;
                        self.insert_erased(entity_id, boxed, *type_id);
                    }
                    None => {
                        tracing::warn!(
                            target: "rge::kernel::ecs::snapshot",
                            component = name,
                            "snapshot component not registered on this World — skipping"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper: read-back tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::component::Component;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Pos {
        x: f32,
        y: f32,
        z: f32,
    }
    impl Component for Pos {}
    impl SnapshotComponent for Pos {}

    #[test]
    fn magic_and_version_in_header() {
        let mut w = World::new();
        w.register_snapshot_component::<Pos>();
        let bytes = w.serialize_snapshot().unwrap();
        assert_eq!(&bytes[0..4], b"RGES");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2u16);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = b"NOPE\x01\x00\x00\x00\x00\x00".to_vec();
        // pad to avoid Truncated
        bytes.extend_from_slice(&[0u8; 20]);
        let mut w = World::new();
        let err = w.restore_from_snapshot(&bytes).unwrap_err();
        assert!(matches!(err, SnapshotError::BadMagic(_)));
    }

    #[test]
    fn bad_version_rejected() {
        // Build a valid header with version=99.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"RGES");
        bytes.extend_from_slice(&99u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // entity_count = 0
        let mut w = World::new();
        let err = w.restore_from_snapshot(&bytes).unwrap_err();
        assert!(matches!(err, SnapshotError::BadVersion(99)));
    }
}
