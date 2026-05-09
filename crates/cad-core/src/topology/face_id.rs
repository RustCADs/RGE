//! Owner seed + derived face identity for B-Rep faces.
//!
//! This module ships [`BRepOwnerId`] (the caller-supplied 16-byte owner seed)
//! and [`BRepFaceId`] (the BLAKE3-derived stable face identity).

use serde::{Deserialize, Serialize};

use super::face_tag::{CuboidFaceTag, ExtrudeFaceTag};

// ---------------------------------------------------------------------------
// BRepOwnerId
// ---------------------------------------------------------------------------

/// Opaque, caller-supplied 16-byte owner seed for B-Rep face identity.
///
/// # Why an owner seed exists
///
/// Naively, [`BRepFaceId`] could be derived from `(operator_kind, face_tag)`
/// alone — but that collides the moment a graph contains two cuboids. The
/// owner seed disambiguates: each independent CAD model the caller wants to
/// give a stable identity space gets its own [`BRepOwnerId`].
///
/// # Two non-negotiable constraints (load-bearing for **rebuild stability**)
///
/// 1. **Caller-supplied, not auto-derived from cad-core internals.** v0
///    takes the owner seed as an explicit constructor argument from the
///    caller (test or downstream consumer). cad-core does NOT mint owner
///    seeds and does NOT carry an `From<NodeId> for BRepOwnerId` impl.
/// 2. **The owner seed must NOT be derived from `NodeId` or
///    `effective_hash`.** The whole point of this substrate is to prove
///    rebuild stability across parameter changes. `NodeId` is derived from
///    `(local_hash || port || upstream)` and `effective_hash` from the
///    recursive structural hash of the operator + its parameters; both
///    *change when parameters change* — using either as the owner seed
///    would defeat the rebuild stability property the substrate is built
///    to prove.
///
/// # Wire format
///
/// 16 bytes is enough entropy for the v0 vocabulary substrate; if
/// downstream callers want stronger collision resistance, they can derive
/// their 16-byte owner from a wider source (UUID v4, BLAKE3 over a
/// caller-defined string, etc.) externally. cad-core does not prescribe
/// the upstream derivation, only the byte width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BRepOwnerId([u8; 16]);

impl BRepOwnerId {
    /// Construct a [`BRepOwnerId`] from 16 raw bytes.
    ///
    /// `const` so callers can build well-known sentinel ids at compile
    /// time. Mirrors `kernel/asset-view::AssetViewId` /
    /// `kernel/io-scheduler::IoRequestId` / `kernel/job-system::JobId`
    /// precedent.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns a borrow of the underlying byte array.
    ///
    /// `const` so the borrow flows through `const fn` consumers without
    /// needing a runtime-evaluated copy.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// BRepFaceId
// ---------------------------------------------------------------------------

/// BLAKE3-derived stable face identity. 16 bytes.
///
/// Derivation:
///
/// ```text
/// BRepFaceId = first 16 bytes of BLAKE3(
///     b"rge.cad.brep.face/v1:" || owner.as_bytes() || kind_tag_bytes
/// )
/// ```
///
/// where `kind_tag_bytes` is a deterministic byte encoding of
/// `(operator_kind_string, face_tag_discriminant)`.
///
/// The `operator_kind_string` is a literal byte-string (e.g. `b"cuboid:"`)
/// — NOT [`crate::operators::OpKind`]'s numeric discriminant — so that
/// future variant reordering on `OpKind` does not break stability for
/// callers who already serialized old [`BRepFaceId`]s.
///
/// 16 bytes matches the owner-id width and is overkill for collision
/// resistance at the per-graph scale (a graph with 2^32 unique faces still
/// has ~2^-32 collision probability per pair).
///
/// # Wire stability
///
/// The byte layout is opaque. Callers should treat this type as a pure
/// handle. The derivation contract above is the substrate's stable surface,
/// not the byte order itself. The `v1` suffix in the domain separator
/// reserves room for a future migration; consumers MUST NOT decode the
/// bytes back into operator-kind / face-tag components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BRepFaceId([u8; 16]);

impl BRepFaceId {
    /// Domain separator prefix. **Do not change** without a coordinated
    /// `v2` migration — every previously-serialized [`BRepFaceId`] depends
    /// on these bytes being byte-identical across processes.
    const DOMAIN: &'static [u8] = b"rge.cad.brep.face/v1:";

    /// Operator-kind tag for `CuboidOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability.
    const KIND_CUBOID: &'static [u8] = b"cuboid:";

    /// Operator-kind tag for `ExtrudeOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability. Distinct from
    /// [`Self::KIND_CUBOID`] so the two operator-kind identity spaces are
    /// disjoint even under the same `(owner, tag-discriminant)` pair.
    const KIND_EXTRUDE: &'static [u8] = b"extrude:";

    /// Construct a [`BRepFaceId`] for one face of a `CuboidOp` instance.
    ///
    /// `owner` is the caller-supplied owner seed (see [`BRepOwnerId`] for
    /// the non-negotiable constraints on its provenance). `tag` selects
    /// which of the 6 cuboid faces this id represents.
    ///
    /// This is the sub-7.2-α entry point. The companion sub-7.2-β
    /// constructor [`Self::for_extrude_face`] handles `ExtrudeOp`. Per-
    /// operator constructors for `RevolveOp` / `BooleanOp` / `LoftOp` /
    /// `SweepOp` / `TransformOp` are out of scope for sub-7.2-β.
    #[must_use]
    pub fn for_cuboid_face(owner: BRepOwnerId, tag: CuboidFaceTag) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN);
        hasher.update(owner.as_bytes());
        hasher.update(Self::KIND_CUBOID);
        hasher.update(&[tag.discriminant()]);
        let full = hasher.finalize();
        let mut truncated = [0u8; 16];
        truncated.copy_from_slice(&full.as_bytes()[..16]);
        Self(truncated)
    }

    /// Construct a [`BRepFaceId`] for one face of an `ExtrudeOp` instance
    /// (sub-7.2-β).
    ///
    /// `owner` is the caller-supplied owner seed (see [`BRepOwnerId`] for
    /// the non-negotiable constraints on its provenance). `tag` selects
    /// which face of the extrusion this id represents — `Bottom` cap,
    /// `Top` cap, or `Side { edge_index, profile_count }`.
    ///
    /// # BLAKE3 input layout
    ///
    /// ```text
    /// BLAKE3(
    ///     b"rge.cad.brep.face/v1:" ||  // domain separator
    ///     owner.as_bytes() ||           // 16 bytes
    ///     b"extrude:" ||                // operator-kind separator
    ///     tag_discriminant_byte ||      // 0 = Bottom, 1 = Top, 2 = Side
    ///     /* Side ONLY: */ edge_index.to_le_bytes() ||    // 4 bytes
    ///     /* Side ONLY: */ profile_count.to_le_bytes()    // 4 bytes
    /// )
    /// ```
    ///
    /// then truncated to the first 16 bytes. For `Bottom` / `Top` the
    /// BLAKE3 input ends after the discriminant byte (no inner data) — so
    /// caps are stable across `length` AND `profile_count` changes. For
    /// `Side`, both `edge_index` and `profile_count` are appended in
    /// little-endian order; profile-count changes break `Side` IDs by
    /// construction (see [`ExtrudeFaceTag`] docs for the full stability
    /// contract).
    #[must_use]
    pub fn for_extrude_face(owner: BRepOwnerId, tag: ExtrudeFaceTag) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN);
        hasher.update(owner.as_bytes());
        hasher.update(Self::KIND_EXTRUDE);
        hasher.update(&[tag.discriminant()]);
        if let ExtrudeFaceTag::Side {
            edge_index,
            profile_count,
        } = tag
        {
            hasher.update(&edge_index.to_le_bytes());
            hasher.update(&profile_count.to_le_bytes());
        }
        let full = hasher.finalize();
        let mut truncated = [0u8; 16];
        truncated.copy_from_slice(&full.as_bytes()[..16]);
        Self(truncated)
    }

    /// Returns a borrow of the underlying 16-byte identity.
    ///
    /// Exposed for low-level use cases (test cross-checks, byte-level
    /// equality assertions). Most callers should compare [`BRepFaceId`]
    /// values directly via `==`.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_id_round_trips_through_bytes() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let owner = BRepOwnerId::from_bytes(bytes);
        assert_eq!(owner.as_bytes(), &bytes);
    }

    #[test]
    fn owner_id_serde_round_trips() {
        let owner = BRepOwnerId::from_bytes([0xa5u8; 16]);
        let s = ron::to_string(&owner).expect("serialize");
        let decoded: BRepOwnerId = ron::from_str(&s).expect("deserialize");
        assert_eq!(owner, decoded);
    }

    #[test]
    fn for_cuboid_face_is_deterministic() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let id_a = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        let id_b = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        assert_eq!(id_a, id_b);
        assert_eq!(id_a.as_bytes(), id_b.as_bytes());
    }

    #[test]
    fn for_cuboid_face_distinguishes_all_six_tags() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let mut ids = Vec::new();
        for tag in [
            CuboidFaceTag::NegZ,
            CuboidFaceTag::PosZ,
            CuboidFaceTag::NegY,
            CuboidFaceTag::PosY,
            CuboidFaceTag::NegX,
            CuboidFaceTag::PosX,
        ] {
            ids.push(BRepFaceId::for_cuboid_face(owner, tag));
        }
        // 6 distinct ids — no two tags map to the same id under the same owner.
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "tag {i} collides with tag {j}");
            }
        }
    }

    #[test]
    fn for_cuboid_face_changes_when_owner_changes() {
        let owner_a = BRepOwnerId::from_bytes([0x11u8; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22u8; 16]);
        let id_a = BRepFaceId::for_cuboid_face(owner_a, CuboidFaceTag::NegZ);
        let id_b = BRepFaceId::for_cuboid_face(owner_b, CuboidFaceTag::NegZ);
        assert_ne!(id_a, id_b);
    }

    /// The domain separator must do its job: BLAKE3 over the bare
    /// `(owner || kind || tag)` payload (without the `b"rge.cad.brep.face/v1:"`
    /// prefix) MUST produce a different byte sequence than [`BRepFaceId::for_cuboid_face`].
    /// This guards against accidental collision with other BLAKE3-derived id
    /// schemes in the workspace.
    #[test]
    fn domain_separator_makes_id_distinct_from_undecorated_blake3() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let tag = CuboidFaceTag::NegZ;
        let with_separator = BRepFaceId::for_cuboid_face(owner, tag);

        let mut undecorated = blake3::Hasher::new();
        undecorated.update(owner.as_bytes());
        undecorated.update(b"cuboid:");
        undecorated.update(&[tag.discriminant()]);
        let undecorated_full = undecorated.finalize();
        let mut undecorated_truncated = [0u8; 16];
        undecorated_truncated.copy_from_slice(&undecorated_full.as_bytes()[..16]);

        assert_ne!(with_separator.as_bytes(), &undecorated_truncated);
    }

    /// Cross-check: [`BRepFaceId::for_cuboid_face`] truncates to the first
    /// 16 bytes of the full BLAKE3-32 output computed with the documented
    /// derivation. This pins the truncation rule + the byte order in the
    /// domain string, so any accidental refactor that changes either
    /// (e.g. taking the last 16 bytes, prefixing the owner before the
    /// domain, etc.) fails this test.
    #[test]
    fn for_cuboid_face_truncates_blake3_first_16_bytes() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let tag = CuboidFaceTag::PosX;
        let actual = BRepFaceId::for_cuboid_face(owner, tag);

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rge.cad.brep.face/v1:");
        hasher.update(owner.as_bytes());
        hasher.update(b"cuboid:");
        hasher.update(&[tag.discriminant()]);
        let full = hasher.finalize();
        let mut expected = [0u8; 16];
        expected.copy_from_slice(&full.as_bytes()[..16]);

        assert_eq!(actual.as_bytes(), &expected);
    }

    #[test]
    fn face_id_serde_round_trips() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let id = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::PosZ);
        let s = ron::to_string(&id).expect("serialize");
        let decoded: BRepFaceId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, decoded);
    }

    #[test]
    fn owner_zero_max_distinct() {
        let zero = BRepOwnerId::from_bytes([0u8; 16]);
        let max = BRepOwnerId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
        let id_zero = BRepFaceId::for_cuboid_face(zero, CuboidFaceTag::NegZ);
        let id_max = BRepFaceId::for_cuboid_face(max, CuboidFaceTag::NegZ);
        assert_ne!(id_zero, id_max);
    }

    // -----------------------------------------------------------------------
    // for_extrude_face tests (sub-7.2-β)
    // -----------------------------------------------------------------------

    #[test]
    fn for_extrude_face_deterministic() {
        // Same `(owner, tag)` produces identical bytes across calls. Repeats
        // for Bottom / Top / Side to make the determinism contract per-variant.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        for tag in [
            ExtrudeFaceTag::Bottom,
            ExtrudeFaceTag::Top,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
            ExtrudeFaceTag::Side {
                edge_index: 7,
                profile_count: 12,
            },
        ] {
            let a = BRepFaceId::for_extrude_face(owner, tag);
            let b = BRepFaceId::for_extrude_face(owner, tag);
            assert_eq!(a, b, "for_extrude_face({tag:?}) is not deterministic");
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
    }

    #[test]
    fn for_extrude_face_distinct_across_tags() {
        // Bottom, Top, and Side {0, 4} all distinct under the same owner.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let bottom = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom);
        let top = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Top);
        let side = BRepFaceId::for_extrude_face(
            owner,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
        );
        assert_ne!(bottom, top);
        assert_ne!(bottom, side);
        assert_ne!(top, side);
    }

    #[test]
    fn for_extrude_face_distinct_across_owners() {
        // Same tag, different owners → different ID. Mirrors the cuboid
        // owner-disambiguation precedent.
        let owner_a = BRepOwnerId::from_bytes([0x11u8; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22u8; 16]);
        for tag in [
            ExtrudeFaceTag::Bottom,
            ExtrudeFaceTag::Top,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
        ] {
            let id_a = BRepFaceId::for_extrude_face(owner_a, tag);
            let id_b = BRepFaceId::for_extrude_face(owner_b, tag);
            assert_ne!(id_a, id_b, "owner-disambiguation failed for {tag:?}");
        }
    }

    /// **Substrate-honesty test for sub-7.2-β.**
    ///
    /// `Side { edge_index: 0, profile_count: 4 }` and
    /// `Side { edge_index: 0, profile_count: 5 }` MUST produce DIFFERENT
    /// `BRepFaceId`s. This proves that profile-count changes (e.g.
    /// square → pentagon) break face identity by construction — they are
    /// NOT silently preserved by magic. The substrate hashes
    /// `profile_count.to_le_bytes()` into the BLAKE3 input on the `Side`
    /// branch precisely to make this assertion hold.
    #[test]
    fn for_extrude_face_count_change_breaks_side_id() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_count_4 = BRepFaceId::for_extrude_face(
            owner,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
        );
        let side_count_5 = BRepFaceId::for_extrude_face(
            owner,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 5,
            },
        );
        assert_ne!(
            side_count_4, side_count_5,
            "side IDs must NOT be preserved across profile-count changes"
        );
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"cuboid:"` (sub-7.2-α) and `b"extrude:"` (sub-7.2-β) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical. This pins the operator-kind separator's load-bearing
    /// role: future operators can be added without colliding with prior
    /// substrates as long as their kind-byte-string is unique.
    #[test]
    fn for_extrude_face_distinct_from_for_cuboid_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // Both tags have discriminant byte 0 (Bottom = 0; NegZ = 0). The
        // only thing distinguishing the two BLAKE3 inputs is the operator-
        // kind separator. If `b"extrude:"` and `b"cuboid:"` accidentally
        // produced the same id under that condition, the substrate's
        // operator-kind separator would not be load-bearing.
        let cuboid_neg_z = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        let extrude_bottom = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom);
        assert_ne!(
            cuboid_neg_z, extrude_bottom,
            "operator-kind separator must produce disjoint identity spaces"
        );
    }
}
