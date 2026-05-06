//! `SnapshotParticipate` trait and [`PieSnapshot`] aggregator — per PLAN.md §6.13.
//!
//! # Overview
//!
//! The Play-in-Editor (PIE) snapshot is the union of two layers:
//!
//! 1. **ECS world bytes** — produced by [`World::serialize_snapshot`], covering
//!    all entities whose components are registered via
//!    [`World::register_snapshot_component`].
//!
//! 2. **Per-subsystem participant payloads** — opaque `Vec<u8>` blobs contributed
//!    by every Tier-2 subsystem (audio, physics, particles, gfx, cad-projection)
//!    and Tier-3 plugin that implements [`SnapshotParticipate`].
//!
//! [`PieSnapshot::capture`] + [`PieSnapshot::restore`] orchestrate both layers
//! atomically from the caller's perspective.
//!
//! # Wire format
//!
//! ```text
//! magic:             [u8; 4]   = b"RGEP"
//! version:           u16 LE    = 1
//! world_bytes_len:   u32 LE
//! world_bytes:       [u8; world_bytes_len]
//! participant_count: u32 LE
//! per participant (sorted by ParticipantId ascending):
//!   id_len:          u32 LE
//!   id_bytes:        [u8; id_len]   (UTF-8)
//!   payload_len:     u32 LE
//!   payload:         [u8; payload_len]
//! ```
//!
//! All integers are little-endian. Participants are written in ascending
//! [`ParticipantId`] lexicographic order, so `to_bytes()` is deterministic
//! regardless of the order participants were registered.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::snapshot::SnapshotError;
use crate::world::World;

// ---------------------------------------------------------------------------
// ParticipantId
// ---------------------------------------------------------------------------

/// Stable identifier for a snapshot participant.
///
/// String-based for cross-version identity stability. Convention:
/// `"<subsystem>.<concrete-impl>"` — e.g. `"audio.kira-mixer"`,
/// `"physics.rapier-rigid-bodies"`.
///
/// Must be unique within a [`PieSnapshot`]'s participant set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ParticipantId(pub String);

impl ParticipantId {
    /// Construct a [`ParticipantId`] from any `Into<String>`.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// ParticipateError
// ---------------------------------------------------------------------------

/// Errors arising from PIE snapshot capture/restore orchestration.
#[derive(Debug, Error)]
pub enum ParticipateError {
    /// The ECS world snapshot layer failed.
    #[error("ECS world snapshot error: {0}")]
    World(#[from] SnapshotError),

    /// A participant's [`SnapshotParticipate::capture`] call failed.
    #[error("participant `{id}` capture failed: {message}")]
    CaptureFailed {
        /// The participant whose capture failed.
        id: ParticipantId,
        /// Human-readable error message from the participant.
        message: String,
    },

    /// A participant's [`SnapshotParticipate::restore`] call failed.
    #[error("participant `{id}` restore failed: {message}")]
    RestoreFailed {
        /// The participant whose restore failed.
        id: ParticipantId,
        /// Human-readable error message from the participant.
        message: String,
    },

    /// The snapshot contains a payload for an id that has no registered
    /// restore handler on the receiving side.
    #[error(
        "participant `{0}` referenced in snapshot has no registered restore handler on this side"
    )]
    UnknownParticipant(ParticipantId),

    /// Two participants in the registration set share the same [`ParticipantId`].
    #[error("duplicate participant id `{0}` in registration set")]
    DuplicateParticipant(ParticipantId),

    /// Envelope serialization/deserialization error (UTF-8 decoding, etc.).
    #[error("envelope serialization: {0}")]
    Serde(String),

    /// The byte slice does not start with `RGEP`.
    #[error("invalid envelope magic bytes: expected `RGEP`, got {0:?}")]
    BadMagic([u8; 4]),

    /// The envelope was written with an unsupported version number.
    #[error("unsupported envelope version: {0}")]
    BadVersion(u16),

    /// The byte stream ended before all declared fields were read.
    #[error("truncated envelope at offset {0}")]
    Truncated(usize),

    /// Implementor-surfaced error from a custom participant.
    #[error("custom error from participant: {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// SnapshotParticipate trait
// ---------------------------------------------------------------------------

/// Subsystem-level snapshot trait — per PLAN.md §6.13.
///
/// Implemented by stateful Tier-2 subsystems (audio, physics, particles, gfx,
/// cad-projection) and Tier-3 plugins to participate in the unified
/// Play-in-Editor snapshot.
///
/// Each participant produces an opaque `Vec<u8>` payload at capture time;
/// [`restore`](SnapshotParticipate::restore) is the exact inverse. Determinism
/// is the **implementor's responsibility** — the [`PieSnapshot`] envelope sorts
/// participants by [`ParticipantId`] for deterministic byte output regardless of
/// registration order, but the payload bytes themselves must also be stable.
///
/// # Contract
///
/// - [`participant_id`](SnapshotParticipate::participant_id) must return the
///   same value on every call for a given instance.
/// - [`capture`](SnapshotParticipate::capture) followed by
///   [`restore`](SnapshotParticipate::restore) on a fresh instance must produce
///   a state byte-identical to the original — no randomness, no timestamps.
pub trait SnapshotParticipate {
    /// Stable identifier for this participant.
    ///
    /// MUST be unique across the participant set for a given snapshot. Convention:
    /// `"<subsystem>.<concrete-impl>"`.
    fn participant_id(&self) -> ParticipantId;

    /// Capture the participant's state to an opaque byte payload.
    ///
    /// # Errors
    ///
    /// Returns [`ParticipateError::Custom`] for implementor-defined failures, or
    /// any other appropriate variant when the failure is more structural.
    fn capture(&self) -> Result<Vec<u8>, ParticipateError>;

    /// Restore the participant's state from a byte payload previously produced
    /// by [`capture`](SnapshotParticipate::capture).
    ///
    /// # Errors
    ///
    /// Returns [`ParticipateError::Custom`] (or a more specific variant) when
    /// the payload is malformed or otherwise unrestorable.
    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError>;
}

// ---------------------------------------------------------------------------
// PieSnapshot
// ---------------------------------------------------------------------------

/// Magic bytes at the start of every PIE envelope.
const MAGIC: &[u8; 4] = b"RGEP";
/// Current PIE envelope format version.
const VERSION: u16 = 1;

/// Aggregate Play-in-Editor snapshot.
///
/// Contains the serialized ECS world state (produced by
/// [`World::serialize_snapshot`]) plus deterministic per-participant byte
/// payloads keyed by [`ParticipantId`].
///
/// See the [module-level docs](self) for the wire format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieSnapshot {
    /// Bytes produced by [`World::serialize_snapshot`].
    pub world_bytes: Vec<u8>,
    /// Per-participant byte payloads keyed by stable id.
    ///
    /// [`BTreeMap`] guarantees deterministic iteration order during
    /// [`to_bytes`](PieSnapshot::to_bytes).
    pub participants: BTreeMap<ParticipantId, Vec<u8>>,
}

impl PieSnapshot {
    /// Build a snapshot by capturing the ECS world and each participant's state.
    ///
    /// `participants` is processed in caller order; duplicate
    /// [`ParticipantId`]s trigger [`ParticipateError::DuplicateParticipant`].
    ///
    /// # Errors
    ///
    /// - [`ParticipateError::World`] — ECS world serialization failed.
    /// - [`ParticipateError::DuplicateParticipant`] — two participants share the
    ///   same id.
    /// - [`ParticipateError::CaptureFailed`] — a participant's
    ///   [`SnapshotParticipate::capture`] returned an error.
    pub fn capture(
        world: &World,
        participants: &[&dyn SnapshotParticipate],
    ) -> Result<Self, ParticipateError> {
        let world_bytes = world.serialize_snapshot()?;

        let mut map: BTreeMap<ParticipantId, Vec<u8>> = BTreeMap::new();
        for p in participants {
            let id = p.participant_id();
            if map.contains_key(&id) {
                return Err(ParticipateError::DuplicateParticipant(id));
            }
            let payload = p.capture().map_err(|e| ParticipateError::CaptureFailed {
                id: id.clone(),
                message: e.to_string(),
            })?;
            map.insert(id, payload);
        }

        Ok(Self {
            world_bytes,
            participants: map,
        })
    }

    /// Restore the ECS world and every participant whose id appears in this snapshot.
    ///
    /// - Participants present in `participants_by_id` for ids **in** the snapshot
    ///   are restored.
    /// - Participants present in `participants_by_id` for ids **not** in the
    ///   snapshot are left untouched — a superset of handlers is fine.
    /// - Ids **in** the snapshot but absent from `participants_by_id` trigger
    ///   [`ParticipateError::UnknownParticipant`].
    ///
    /// # Errors
    ///
    /// - [`ParticipateError::World`] — ECS world restore failed.
    /// - [`ParticipateError::UnknownParticipant`] — snapshot id has no handler.
    /// - [`ParticipateError::RestoreFailed`] — a participant's
    ///   [`SnapshotParticipate::restore`] returned an error.
    pub fn restore(
        &self,
        world: &mut World,
        participants_by_id: &mut [(&ParticipantId, &mut dyn SnapshotParticipate)],
    ) -> Result<(), ParticipateError> {
        world.restore_from_snapshot(&self.world_bytes)?;

        // Build a lookup from id → mutable trait object.
        let mut handler_map: BTreeMap<&ParticipantId, &mut dyn SnapshotParticipate> =
            participants_by_id
                .iter_mut()
                .map(|(id, p)| (*id, &mut **p))
                .collect();

        for (id, payload) in &self.participants {
            match handler_map.get_mut(id) {
                Some(participant) => {
                    participant
                        .restore(payload)
                        .map_err(|e| ParticipateError::RestoreFailed {
                            id: id.clone(),
                            message: e.to_string(),
                        })?;
                }
                None => {
                    return Err(ParticipateError::UnknownParticipant(id.clone()));
                }
            }
        }

        Ok(())
    }

    /// Serialize the PIE envelope to a deterministic byte buffer.
    ///
    /// Participants are written in ascending [`ParticipantId`] lexicographic
    /// order, so identical logical state always produces byte-identical output
    /// regardless of registration order.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        // Pre-size: header(10) + world_len(4) + world_bytes + part_count(4)
        //           + per-part: id_len(4) + id + payload_len(4) + payload
        let mut buf = Vec::with_capacity(
            10 + 4
                + self.world_bytes.len()
                + 4
                + self
                    .participants
                    .values()
                    .map(|v| 8 + v.len())
                    .sum::<usize>()
                + self.participants.keys().map(|k| k.0.len()).sum::<usize>(),
        );

        // Magic + version.
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());

        // World bytes.
        #[allow(clippy::cast_possible_truncation)]
        buf.extend_from_slice(&(self.world_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.world_bytes);

        // Participant count (BTreeMap iterates in ascending key order).
        #[allow(clippy::cast_possible_truncation)]
        buf.extend_from_slice(&(self.participants.len() as u32).to_le_bytes());

        for (id, payload) in &self.participants {
            let id_bytes = id.0.as_bytes();
            #[allow(clippy::cast_possible_truncation)]
            buf.extend_from_slice(&(id_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(id_bytes);
            #[allow(clippy::cast_possible_truncation)]
            buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            buf.extend_from_slice(payload);
        }

        buf
    }

    /// Deserialize a PIE envelope from bytes, validating magic, version, and
    /// all length-prefixed fields.
    ///
    /// # Errors
    ///
    /// - [`ParticipateError::BadMagic`] — first 4 bytes are not `RGEP`.
    /// - [`ParticipateError::BadVersion`] — version field is not 1.
    /// - [`ParticipateError::Truncated`] — byte stream ends before all declared
    ///   fields are consumed.
    /// - [`ParticipateError::Serde`] — a participant id is not valid UTF-8.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParticipateError> {
        let mut pos = 0usize;

        /// Read exactly `$n` bytes, advancing `pos`. Returns `Truncated` if
        /// the slice is too short.
        macro_rules! read_bytes {
            ($n:expr) => {{
                let end = pos + $n;
                if end > bytes.len() {
                    return Err(ParticipateError::Truncated(pos));
                }
                let slice = &bytes[pos..end];
                pos = end;
                slice
            }};
        }

        macro_rules! read_u16 {
            () => {{
                let b = read_bytes!(2);
                u16::from_le_bytes([b[0], b[1]])
            }};
        }

        macro_rules! read_u32 {
            () => {{
                let b = read_bytes!(4);
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }};
        }

        // Validate magic.
        let magic_slice = read_bytes!(4);
        let mut magic = [0u8; 4];
        magic.copy_from_slice(magic_slice);
        if &magic != MAGIC {
            return Err(ParticipateError::BadMagic(magic));
        }

        // Validate version.
        let version = read_u16!();
        if version != VERSION {
            return Err(ParticipateError::BadVersion(version));
        }

        // World bytes.
        let world_len = read_u32!() as usize;
        let world_bytes = read_bytes!(world_len).to_vec();

        // Participants.
        let participant_count = read_u32!() as usize;
        let mut participants: BTreeMap<ParticipantId, Vec<u8>> = BTreeMap::new();

        for _ in 0..participant_count {
            let id_len = read_u32!() as usize;
            let id_bytes = read_bytes!(id_len);
            let id_str = std::str::from_utf8(id_bytes)
                .map_err(|e| ParticipateError::Serde(e.to_string()))?;
            let id = ParticipantId::new(id_str);

            let payload_len = read_u32!() as usize;
            let payload = read_bytes!(payload_len).to_vec();

            participants.insert(id, payload);
        }

        Ok(Self {
            world_bytes,
            participants,
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    // -----------------------------------------------------------------------
    // ParticipantId
    // -----------------------------------------------------------------------

    #[test]
    fn participant_id_new_accepts_any_into_string() {
        let from_str_literal = ParticipantId::new("audio.kira-mixer");
        let from_string = ParticipantId::new(String::from("physics.rapier"));
        assert_eq!(from_str_literal.as_str(), "audio.kira-mixer");
        assert_eq!(from_string.as_str(), "physics.rapier");
    }

    #[test]
    fn participant_id_display_is_inner_string() {
        let id = ParticipantId::new("subsystem.impl");
        assert_eq!(id.to_string(), "subsystem.impl");
    }

    // -----------------------------------------------------------------------
    // Empty PieSnapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn empty_pie_snapshot_round_trips_via_to_from_bytes() {
        let world = World::new();
        let snap = PieSnapshot::capture(&world, &[]).expect("capture empty");
        let bytes = snap.to_bytes();
        let snap2 = PieSnapshot::from_bytes(&bytes).expect("from_bytes");
        assert_eq!(
            snap, snap2,
            "empty snapshot must be byte-identical after round-trip"
        );
    }

    // -----------------------------------------------------------------------
    // Determinism
    // -----------------------------------------------------------------------

    struct ConstantParticipant {
        id: ParticipantId,
        data: Vec<u8>,
    }

    impl SnapshotParticipate for ConstantParticipant {
        fn participant_id(&self) -> ParticipantId {
            self.id.clone()
        }

        fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
            Ok(self.data.clone())
        }

        fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
            self.data = bytes.to_vec();
            Ok(())
        }
    }

    #[test]
    fn to_bytes_is_deterministic_across_two_captures() {
        let world = World::new();
        let p = ConstantParticipant {
            id: ParticipantId::new("test.constant"),
            data: vec![1, 2, 3, 4],
        };
        let snap1 =
            PieSnapshot::capture(&world, &[&p as &dyn SnapshotParticipate]).expect("capture1");
        let snap2 =
            PieSnapshot::capture(&world, &[&p as &dyn SnapshotParticipate]).expect("capture2");
        assert_eq!(
            snap1.to_bytes(),
            snap2.to_bytes(),
            "two captures of same state must produce byte-identical bytes"
        );
    }

    // -----------------------------------------------------------------------
    // Magic / version validation
    // -----------------------------------------------------------------------

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = b"NOPE\x01\x00".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // world_len = 0
        bytes.extend_from_slice(&0u32.to_le_bytes()); // participant_count = 0
        let err = PieSnapshot::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, ParticipateError::BadMagic(_)));
    }

    #[test]
    fn bad_version_rejected() {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"RGEP");
        bytes.extend_from_slice(&99u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // world_len = 0
        bytes.extend_from_slice(&0u32.to_le_bytes()); // participant_count = 0
        let err = PieSnapshot::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, ParticipateError::BadVersion(99)));
    }

    #[test]
    fn truncated_envelope_rejected() {
        let world = World::new();
        let snap = PieSnapshot::capture(&world, &[]).expect("capture");
        let bytes = snap.to_bytes();
        // Truncate by 1 byte.
        let truncated = &bytes[..bytes.len() - 1];
        // An all-zero world_bytes length from truncation means we may hit
        // Truncated at various offsets; just check it's an error.
        // With the empty world bytes and no participants the last byte is
        // part of the participant_count u32 — removing it causes Truncated.
        let err = PieSnapshot::from_bytes(truncated).unwrap_err();
        assert!(
            matches!(err, ParticipateError::Truncated(_)),
            "expected Truncated, got {err:?}"
        );
    }
}
