//! `editor_state::face_selection` — face-level selection set.
//!
//! Coordination state, not authoritative content (per PLAN.md §1.15).
//! Mirrors the existing [`Selection`] shape but selects FACES within
//! entities rather than just entities.
//!
//! Each [`FaceSelection`] is the tuple `(entity, owner, face_id)` —
//! explicit owner makes the selection self-contained and meaningful
//! even if the entity's `BRepHandle.brep_owner` later changes.
//!
//! Resolvability through the current projected world is a separate
//! concern owned by `cad-projection::CadProjection::face_resolves_in_projection`;
//! [`FaceSelectionSet::partition`] accepts a predicate so callers can
//! wire that resolvability check (or any other) at decision points.
//! Nothing is automatically pruned.
//!
//! [`Selection`]: crate::Selection

use std::collections::BTreeSet;

use rge_cad_core::{BRepFaceId, BRepOwnerId};
use rge_kernel_ecs::EntityId;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EntityId serde helpers
//
// `rge-kernel-ecs` does not enable `ulid`'s optional `serde` feature, so
// `EntityId` has no `Serialize`/`Deserialize` impl. Bridge by serialising via
// the `Ulid` value (which does implement serde when the `ulid/serde` feature
// is enabled in this crate's `Cargo.toml`). Mirrors the `selection.rs`
// pattern verbatim — duplicated rather than centralised so each module
// stays self-contained at this stage of substrate growth.
// ---------------------------------------------------------------------------

/// Serde-transparent newtype used only inside the `BTreeSet` serialisation.
/// Ordering is preserved: `EntityId: Ord` matches `Ulid: Ord` (ULID order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct EntityIdSerde(ulid::Ulid);

impl From<EntityId> for EntityIdSerde {
    fn from(id: EntityId) -> Self {
        Self(id.ulid())
    }
}

impl From<EntityIdSerde> for EntityId {
    fn from(s: EntityIdSerde) -> Self {
        EntityId::from_ulid(s.0)
    }
}

// ---------------------------------------------------------------------------
// FaceSelection
// ---------------------------------------------------------------------------

/// A single face-level selection: an entity + its owner-seeded identity
/// space + the specific face within that space.
///
/// `Ord` is derived to support deterministic iteration in
/// `BTreeSet<FaceSelection>`. The order is: entity first (ULID order via
/// the same [`EntityIdSerde`] bridge as [`crate::Selection`]), then
/// owner bytes, then face_id bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceSelection {
    /// The entity carrying the face.
    pub entity: EntityId,
    /// The owner-seeded identity space the face_id was minted under.
    /// Stored explicitly so the selection survives a later
    /// `BRepHandle.brep_owner` mutation on the entity.
    pub owner: BRepOwnerId,
    /// The stable [`BRepFaceId`] identifying the face within `owner`'s
    /// identity space.
    pub face_id: BRepFaceId,
}

// ---------------------------------------------------------------------------
// FaceSelection serde
//
// `EntityId` is not directly serde-able; we round-trip through
// `(EntityIdSerde, BRepOwnerId, BRepFaceId)` so the wire format is a
// simple tuple. The owner / face_id types already implement serde via
// their `derive` in `cad-core::topology::face_id`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct FaceSelectionSerde {
    entity: EntityIdSerde,
    owner: BRepOwnerId,
    face_id: BRepFaceId,
}

impl From<FaceSelection> for FaceSelectionSerde {
    fn from(f: FaceSelection) -> Self {
        Self {
            entity: EntityIdSerde::from(f.entity),
            owner: f.owner,
            face_id: f.face_id,
        }
    }
}

impl From<FaceSelectionSerde> for FaceSelection {
    fn from(f: FaceSelectionSerde) -> Self {
        Self {
            entity: EntityId::from(f.entity),
            owner: f.owner,
            face_id: f.face_id,
        }
    }
}

impl Serialize for FaceSelection {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FaceSelectionSerde::from(*self).serialize(s)
    }
}

impl<'de> Deserialize<'de> for FaceSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        FaceSelectionSerde::deserialize(d).map(FaceSelection::from)
    }
}

// ---------------------------------------------------------------------------
// FaceSelectionSet
// ---------------------------------------------------------------------------

/// The set of face-level selections the user has accumulated.
/// Coordination state — does NOT own component data; only references
/// entities + face identities by ID.
///
/// Backed by `BTreeSet<FaceSelection>` for deterministic iteration order
/// (required for inspector display + audit-log recording).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FaceSelectionSet {
    selections: BTreeSet<FaceSelection>,
}

// Manual Serialize/Deserialize: round-trip via `BTreeSet<FaceSelectionSerde>`
// so EntityId bridging happens consistently.
impl Serialize for FaceSelectionSet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let set: BTreeSet<FaceSelectionSerde> = self
            .selections
            .iter()
            .copied()
            .map(FaceSelectionSerde::from)
            .collect();
        set.serialize(s)
    }
}

impl<'de> Deserialize<'de> for FaceSelectionSet {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let set = BTreeSet::<FaceSelectionSerde>::deserialize(d)?;
        Ok(Self {
            selections: set.into_iter().map(FaceSelection::from).collect(),
        })
    }
}

impl FaceSelectionSet {
    /// Construct an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a face selection. Returns `true` if newly added.
    pub fn add(&mut self, selection: FaceSelection) -> bool {
        self.selections.insert(selection)
    }

    /// Remove a face selection. Returns `true` if it was present.
    pub fn remove(&mut self, selection: &FaceSelection) -> bool {
        self.selections.remove(selection)
    }

    /// True if `selection` is in the set.
    #[must_use]
    pub fn contains(&self, selection: &FaceSelection) -> bool {
        self.selections.contains(selection)
    }

    /// Clear all selections.
    pub fn clear(&mut self) {
        self.selections.clear();
    }

    /// Number of selections.
    #[must_use]
    pub fn len(&self) -> usize {
        self.selections.len()
    }

    /// True if no selection is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    /// Iterate held selections in deterministic ascending order.
    pub fn iter(&self) -> impl Iterator<Item = &FaceSelection> {
        self.selections.iter()
    }

    /// Partition the set into two sets based on a caller-supplied predicate.
    ///
    /// The first returned set is "survivors" (items where `predicate` returned
    /// `true`); the second is "invalidated" (items where `predicate` returned
    /// `false`).
    ///
    /// The typical caller wires this with `cad-projection`'s
    /// `face_resolves_in_projection` query:
    ///
    /// ```ignore
    /// let (survivors, invalidated) = face_selections.partition(|fs| {
    ///     projection.face_resolves_in_projection(
    ///         fs.entity, fs.owner, fs.face_id, world, graph,
    ///     )
    /// });
    /// ```
    ///
    /// Nothing is automatically pruned — callers decide what to do with the
    /// invalidated set (drop / surface to UI / log / reproject / etc.). The
    /// substrate is honest about the gap that
    /// `docs/architecture/FILLET_OUTPUT_IDENTITY.md` documents: filleted
    /// output is identity-opaque, so any [`FaceSelection`] on filleted output
    /// will land in `invalidated`.
    #[must_use]
    pub fn partition<F: Fn(&FaceSelection) -> bool>(&self, predicate: F) -> (Self, Self) {
        let mut survivors = Self::new();
        let mut invalidated = Self::new();
        for sel in &self.selections {
            if predicate(sel) {
                survivors.add(*sel);
            } else {
                invalidated.add(*sel);
            }
        }
        (survivors, invalidated)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_cad_core::CuboidFaceTag;

    use super::*;

    const OWNER_A: BRepOwnerId = BRepOwnerId::from_bytes([0xa1; 16]);
    const OWNER_B: BRepOwnerId = BRepOwnerId::from_bytes([0xb2; 16]);

    /// All 6 cuboid face tags, deterministic across runs. The tests use a
    /// `usize` index into this slice when they want a few distinct face
    /// identities under the same owner.
    const CUBOID_FACE_TAGS: [CuboidFaceTag; 6] = [
        CuboidFaceTag::NegZ,
        CuboidFaceTag::PosZ,
        CuboidFaceTag::NegY,
        CuboidFaceTag::PosY,
        CuboidFaceTag::NegX,
        CuboidFaceTag::PosX,
    ];

    fn face_id(owner: BRepOwnerId, tag_idx: usize) -> BRepFaceId {
        BRepFaceId::for_cuboid_face(owner, CUBOID_FACE_TAGS[tag_idx % CUBOID_FACE_TAGS.len()])
    }

    fn entity() -> EntityId {
        EntityId::new()
    }

    fn make(e: EntityId, owner: BRepOwnerId, tag_idx: usize) -> FaceSelection {
        FaceSelection {
            entity: e,
            owner,
            face_id: face_id(owner, tag_idx),
        }
    }

    #[test]
    fn face_selection_round_trips_via_ron() {
        let sel = make(entity(), OWNER_A, 0);

        let serialized = ron::to_string(&sel).expect("serialize FaceSelection");
        let deserialized: FaceSelection =
            ron::from_str(&serialized).expect("deserialize FaceSelection");

        assert_eq!(
            sel, deserialized,
            "round-trip must produce identical FaceSelection"
        );
    }

    #[test]
    fn face_selection_set_round_trips_via_ron() {
        let mut set = FaceSelectionSet::new();
        let e = entity();
        for tag_idx in 0..4 {
            set.add(make(e, OWNER_A, tag_idx));
        }

        let serialized = ron::to_string(&set).expect("serialize FaceSelectionSet");
        let deserialized: FaceSelectionSet =
            ron::from_str(&serialized).expect("deserialize FaceSelectionSet");

        assert_eq!(
            set, deserialized,
            "round-trip must produce identical FaceSelectionSet"
        );
        assert_eq!(deserialized.len(), 4);
        for tag_idx in 0..4 {
            assert!(deserialized.contains(&make(e, OWNER_A, tag_idx)));
        }
    }

    #[test]
    fn face_selection_set_partition_with_always_true_predicate() {
        let mut set = FaceSelectionSet::new();
        let e = entity();
        for tag_idx in 0..5 {
            set.add(make(e, OWNER_A, tag_idx));
        }

        let (survivors, invalidated) = set.partition(|_| true);
        assert_eq!(survivors.len(), 5);
        assert!(invalidated.is_empty());
    }

    #[test]
    fn face_selection_set_partition_with_always_false_predicate() {
        let mut set = FaceSelectionSet::new();
        let e = entity();
        for tag_idx in 0..5 {
            set.add(make(e, OWNER_A, tag_idx));
        }

        let (survivors, invalidated) = set.partition(|_| false);
        assert!(survivors.is_empty());
        assert_eq!(invalidated.len(), 5);
    }

    #[test]
    fn face_selection_set_partition_with_split_predicate() {
        // Build a set with two owners; predicate keeps OWNER_A only.
        let mut set = FaceSelectionSet::new();
        let e = entity();
        for tag_idx in 0..3 {
            set.add(make(e, OWNER_A, tag_idx));
            set.add(make(e, OWNER_B, tag_idx));
        }

        let (survivors, invalidated) = set.partition(|fs| fs.owner == OWNER_A);
        assert_eq!(survivors.len(), 3);
        assert_eq!(invalidated.len(), 3);
        for s in survivors.iter() {
            assert_eq!(s.owner, OWNER_A);
        }
        for s in invalidated.iter() {
            assert_eq!(s.owner, OWNER_B);
        }
    }

    #[test]
    fn face_selection_set_partition_preserves_total_count() {
        let mut set = FaceSelectionSet::new();
        let e = entity();
        for tag_idx in 0..6 {
            set.add(make(e, OWNER_A, tag_idx));
        }
        let original_len = set.len();

        // A predicate that keeps roughly half (parity of the first byte
        // of the face_id, which is opaque but stable).
        let (survivors, invalidated) = set.partition(|fs| fs.face_id.as_bytes()[0] % 2 == 0);
        assert_eq!(
            survivors.len() + invalidated.len(),
            original_len,
            "partition must preserve total count"
        );
    }

    #[test]
    fn face_selection_set_iteration_is_deterministic() {
        let mut set = FaceSelectionSet::new();
        let e = entity();
        // Insert in a non-canonical order.
        for tag_idx in [4_usize, 0, 2, 5, 1, 3].iter().copied() {
            set.add(make(e, OWNER_A, tag_idx));
        }

        let pass1: Vec<FaceSelection> = set.iter().copied().collect();
        let pass2: Vec<FaceSelection> = set.iter().copied().collect();
        assert_eq!(
            pass1, pass2,
            "two iterations of the same set must yield the same order"
        );
        // And the order is sorted (ascending by FaceSelection's derived Ord).
        let mut sorted = pass1.clone();
        sorted.sort();
        assert_eq!(pass1, sorted, "iter must match BTreeSet sorted order");
    }

    #[test]
    fn add_is_idempotent() {
        let mut set = FaceSelectionSet::new();
        let s = make(entity(), OWNER_A, 0);
        assert!(set.add(s), "first add returns true");
        assert!(!set.add(s), "duplicate add returns false");
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn remove_returns_presence() {
        let mut set = FaceSelectionSet::new();
        let s = make(entity(), OWNER_A, 0);
        assert!(!set.remove(&s));
        set.add(s);
        assert!(set.remove(&s));
        assert!(set.is_empty());
    }

    #[test]
    fn contains_reflects_membership() {
        let mut set = FaceSelectionSet::new();
        let s = make(entity(), OWNER_A, 0);
        assert!(!set.contains(&s));
        set.add(s);
        assert!(set.contains(&s));
    }

    #[test]
    fn clear_empties_set() {
        let mut set = FaceSelectionSet::new();
        set.add(make(entity(), OWNER_A, 0));
        set.add(make(entity(), OWNER_A, 1));
        set.clear();
        assert!(set.is_empty());
    }

    #[test]
    fn default_is_empty() {
        let set = FaceSelectionSet::default();
        assert!(set.is_empty());
    }
}
