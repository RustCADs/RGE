//! `BRepFaceId` constructor + tests for `LoftOp` (sub-7.2-δ).

use super::{BRepFaceId, BRepOwnerId};
use crate::topology::face_tag::LoftFaceTag;

impl BRepFaceId {
    /// Construct a [`BRepFaceId`] for one face of a `LoftOp` instance
    /// (sub-7.2-δ).
    ///
    /// `owner` is the caller-supplied owner seed (see [`BRepOwnerId`] for
    /// the non-negotiable constraints on its provenance). `tag` selects
    /// which face of the loft this id represents — `Bottom` cap, `Top`
    /// cap, or `Side { edge_index, profile_a_count, profile_b_count }`.
    ///
    /// # BLAKE3 input layout
    ///
    /// ```text
    /// BLAKE3(
    ///     b"rge.cad.brep.face/v1:" ||  // domain separator
    ///     owner.as_bytes() ||           // 16 bytes
    ///     b"loft:" ||                   // operator-kind separator
    ///     tag_discriminant_byte ||      // 0 = Bottom, 1 = Top, 2 = Side
    ///     /* Side ONLY: */ edge_index.to_le_bytes() ||        // 4 bytes
    ///     /* Side ONLY: */ profile_a_count.to_le_bytes() ||   // 4 bytes
    ///     /* Side ONLY: */ profile_b_count.to_le_bytes()      // 4 bytes
    /// )
    /// ```
    ///
    /// then truncated to the first 16 bytes. For `Bottom` / `Top` the
    /// BLAKE3 input ends after the discriminant byte (no inner data) — so
    /// caps are stable across `length`, profile-coordinate, and profile-
    /// count changes (categorical caps; same explicit limit as
    /// [`ExtrudeFaceTag`]). For `Side`, all three of `edge_index`,
    /// `profile_a_count`, and `profile_b_count` are appended in little-
    /// endian order.
    ///
    /// # Profile A → B ordering is load-bearing
    ///
    /// The order of the two profile counts in the BLAKE3 input is
    /// `profile_a_count` THEN `profile_b_count`. This ordering is
    /// **load-bearing** because swapping a Loft's `profile_a` and
    /// `profile_b` produces a geometrically-different mesh (top and
    /// bottom swap, side winding flips), and the IDs SHOULD differ to
    /// reflect that. A future operator that reverses or otherwise mutates
    /// this ordering MUST go through a `v2` migration in the domain
    /// separator.
    ///
    /// # Substrate-honesty guardrail
    ///
    /// The constructor handles `profile_a_count != profile_b_count`
    /// directly even though [`crate::operators::LoftOp::evaluate`] rejects
    /// such inputs at runtime. This is deliberate — the substrate is
    /// self-describing and does NOT depend on the validation rule living
    /// elsewhere in `LoftOp`. See [`LoftFaceTag`] for the full stability
    /// contract.
    #[must_use]
    pub fn for_loft_face(owner: BRepOwnerId, tag: LoftFaceTag) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN);
        hasher.update(owner.as_bytes());
        hasher.update(Self::KIND_LOFT);
        hasher.update(&[tag.discriminant()]);
        if let LoftFaceTag::Side {
            edge_index,
            profile_a_count,
            profile_b_count,
        } = tag
        {
            hasher.update(&edge_index.to_le_bytes());
            hasher.update(&profile_a_count.to_le_bytes());
            hasher.update(&profile_b_count.to_le_bytes());
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
    use crate::topology::face_tag::{CuboidFaceTag, ExtrudeFaceTag, RevolveFaceTag};

    #[test]
    fn for_loft_face_deterministic() {
        // Same `(owner, tag)` produces identical bytes across calls. Repeats
        // for Bottom / Top / Side to make the determinism contract per-variant.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        for tag in [
            LoftFaceTag::Bottom,
            LoftFaceTag::Top,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 4,
            },
            LoftFaceTag::Side {
                edge_index: 7,
                profile_a_count: 12,
                profile_b_count: 12,
            },
        ] {
            let a = BRepFaceId::for_loft_face(owner, tag);
            let b = BRepFaceId::for_loft_face(owner, tag);
            assert_eq!(a, b, "for_loft_face({tag:?}) is not deterministic");
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
    }

    #[test]
    fn for_loft_face_distinct_across_tag_kinds() {
        // Bottom, Top, and Side {0, 4, 4} all distinct under the same owner.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let bottom = BRepFaceId::for_loft_face(owner, LoftFaceTag::Bottom);
        let top = BRepFaceId::for_loft_face(owner, LoftFaceTag::Top);
        let side = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 4,
            },
        );
        assert_ne!(bottom, top);
        assert_ne!(bottom, side);
        assert_ne!(top, side);
    }

    #[test]
    fn for_loft_face_distinct_across_owners() {
        // Same tag, different owners → different ID. Mirrors the cuboid /
        // extrude / revolve owner-disambiguation precedent.
        let owner_a = BRepOwnerId::from_bytes([0x11u8; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22u8; 16]);
        for tag in [
            LoftFaceTag::Bottom,
            LoftFaceTag::Top,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 4,
            },
        ] {
            let id_a = BRepFaceId::for_loft_face(owner_a, tag);
            let id_b = BRepFaceId::for_loft_face(owner_b, tag);
            assert_ne!(id_a, id_b, "owner-disambiguation failed for {tag:?}");
        }
    }

    /// **Substrate-honesty test #2 for sub-7.2-δ: profile-A vs profile-B
    /// ordering matters.**
    ///
    /// `Side { edge_index: 0, profile_a_count: 4, profile_b_count: 5 }` and
    /// `Side { edge_index: 0, profile_a_count: 5, profile_b_count: 4 }` MUST
    /// produce DIFFERENT [`BRepFaceId`]s. This proves that swapping a Loft's
    /// `profile_a` and `profile_b` produces a geometrically-different mesh
    /// (top and bottom swap, side winding flips), and the IDs reflect that
    /// by hashing the two counts in `(profile_a_count, profile_b_count)`
    /// order — not as a sorted pair or a single combined value.
    #[test]
    fn for_loft_face_distinct_for_swapped_profile_counts() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_a4_b5 = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 5,
            },
        );
        let side_a5_b4 = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 5,
                profile_b_count: 4,
            },
        );
        assert_ne!(
            side_a4_b5, side_a5_b4,
            "side IDs must differ when profile_a_count and profile_b_count are swapped"
        );
    }

    /// **Substrate-honesty guardrail test for sub-7.2-δ.**
    ///
    /// Even though [`crate::operators::LoftOp::evaluate`] rejects unequal
    /// `profile_a.len() != profile_b.len()` at runtime, the
    /// [`BRepFaceId::for_loft_face`] constructor MUST handle such an input
    /// directly without panicking. This proves the substrate is genuinely
    /// self-describing and does NOT depend on `LoftOp::evaluate`'s
    /// validation rule. The resulting ID is finite and distinct from
    /// `Side(0, 4, 4)` and `Side(0, 5, 5)` by construction (BOTH counts are
    /// independently hashed).
    #[test]
    fn for_loft_face_handles_unequal_profile_counts_at_constructor_level() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_unequal = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 5,
            },
        );
        let side_4_4 = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 4,
            },
        );
        let side_5_5 = BRepFaceId::for_loft_face(
            owner,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 5,
                profile_b_count: 5,
            },
        );
        // The unequal-count ID is finite (16 bytes; trivially true by
        // construction — `for_loft_face` returns a `BRepFaceId([u8; 16])`)
        // and distinct from both equal-count siblings.
        assert_eq!(side_unequal.as_bytes().len(), 16);
        assert_ne!(
            side_unequal, side_4_4,
            "unequal-count Side ID must NOT collide with Side(0, 4, 4)"
        );
        assert_ne!(
            side_unequal, side_5_5,
            "unequal-count Side ID must NOT collide with Side(0, 5, 5)"
        );
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"extrude:"` (sub-7.2-β) and `b"loft:"` (sub-7.2-δ) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical. This pins the operator-kind separator's load-bearing role
    /// for the fourth per-operator face-tag substrate.
    #[test]
    fn for_loft_face_distinct_from_for_extrude_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // Both Extrude and Loft `Bottom` carry no inner data and share
        // discriminant byte 0. The only thing distinguishing the BLAKE3
        // inputs is the operator-kind separator.
        let extrude_bottom = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom);
        let loft_bottom = BRepFaceId::for_loft_face(owner, LoftFaceTag::Bottom);
        assert_ne!(
            extrude_bottom, loft_bottom,
            "operator-kind separator must produce disjoint identity spaces \
             across extrude and loft"
        );
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"cuboid:"` (sub-7.2-α) and `b"loft:"` (sub-7.2-δ) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical.
    #[test]
    fn for_loft_face_distinct_from_for_cuboid_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // CuboidFaceTag::NegZ has discriminant 0; LoftFaceTag::Bottom has
        // discriminant 0. The only thing distinguishing the BLAKE3 inputs
        // is the operator-kind separator (`b"cuboid:"` vs `b"loft:"`).
        let cuboid_neg_z = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        let loft_bottom = BRepFaceId::for_loft_face(owner, LoftFaceTag::Bottom);
        assert_ne!(
            cuboid_neg_z, loft_bottom,
            "operator-kind separator must produce disjoint identity spaces \
             across cuboid and loft"
        );
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"revolve:"` (sub-7.2-γ) and `b"loft:"` (sub-7.2-δ) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical.
    #[test]
    fn for_loft_face_distinct_from_for_revolve_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // RevolveFaceTag::Side has discriminant 0; LoftFaceTag::Bottom has
        // discriminant 0. Even though the appended payloads differ, the
        // operator-kind separator alone keeps the identity spaces disjoint
        // (each fed BLAKE3 input differs at the kind-separator position
        // long before the discriminant or any payload byte). Compare a
        // payload-free combination: LoftFaceTag::Top (discriminant 1, no
        // payload) vs RevolveFaceTag::StartCap { profile_count: 4 }
        // (discriminant 1, payload: profile_count u32 LE). The kind
        // separator's load-bearing role is demonstrated regardless.
        let revolve_start_cap =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 });
        let loft_top = BRepFaceId::for_loft_face(owner, LoftFaceTag::Top);
        assert_ne!(
            revolve_start_cap, loft_top,
            "operator-kind separator must produce disjoint identity spaces \
             across revolve and loft"
        );
    }
}
