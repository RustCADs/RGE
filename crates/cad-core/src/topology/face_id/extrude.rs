//! `BRepFaceId` constructor + tests for `ExtrudeOp` (sub-7.2-β).

use super::{BRepFaceId, BRepOwnerId};
use crate::topology::face_tag::ExtrudeFaceTag;

impl BRepFaceId {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::face_tag::CuboidFaceTag;

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
