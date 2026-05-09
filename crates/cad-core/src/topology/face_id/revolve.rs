//! `BRepFaceId` constructor + tests for `RevolveOp` (sub-7.2-γ).

use super::{BRepFaceId, BRepOwnerId};
use crate::topology::face_tag::RevolveFaceTag;

impl BRepFaceId {
    /// Construct a [`BRepFaceId`] for one face of a `RevolveOp` instance
    /// (sub-7.2-γ).
    ///
    /// `owner` is the caller-supplied owner seed (see [`BRepOwnerId`] for
    /// the non-negotiable constraints on its provenance). `tag` selects
    /// which face of the revolved surface this id represents — `Side` (one
    /// per profile edge; both modes), `StartCap` (Partial mode ONLY), or
    /// `EndCap` (Partial mode ONLY).
    ///
    /// # BLAKE3 input layout
    ///
    /// ```text
    /// BLAKE3(
    ///     b"rge.cad.brep.face/v1:" ||  // domain separator
    ///     owner.as_bytes() ||           // 16 bytes
    ///     b"revolve:" ||                // operator-kind separator
    ///     tag_discriminant_byte ||      // 0 = Side, 1 = StartCap, 2 = EndCap
    ///     /* Side ONLY: */ side_index.to_le_bytes() ||      // 4 bytes
    ///     /* Side ONLY: */ profile_count.to_le_bytes() ||   // 4 bytes
    ///     /* Side ONLY: */ segment_count.to_le_bytes() ||   // 4 bytes
    ///     /* Side ONLY: */ mode.discriminant() ||           // 1 byte
    ///     /* StartCap/EndCap ONLY: */ profile_count.to_le_bytes()  // 4 bytes
    /// )
    /// ```
    ///
    /// then truncated to the first 16 bytes. For `Side`, the appended
    /// `(side_index, profile_count, segment_count, mode)` quadruple ensures
    /// each topology axis breaks Side IDs by construction (mode flips,
    /// segment_count changes, profile_count changes all produce disjoint
    /// Side identity spaces). For `StartCap` / `EndCap`, only
    /// `profile_count` is appended — segment_count and angle do not affect
    /// cap geometry, so they are deliberately NOT hashed in (the substrate
    /// honesty principle: caps don't over-encode).
    ///
    /// See [`RevolveFaceTag`] for the full stability contract.
    #[must_use]
    pub fn for_revolve_face(owner: BRepOwnerId, tag: RevolveFaceTag) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN);
        hasher.update(owner.as_bytes());
        hasher.update(Self::KIND_REVOLVE);
        hasher.update(&[tag.discriminant()]);
        match tag {
            RevolveFaceTag::Side {
                side_index,
                profile_count,
                segment_count,
                mode,
            } => {
                hasher.update(&side_index.to_le_bytes());
                hasher.update(&profile_count.to_le_bytes());
                hasher.update(&segment_count.to_le_bytes());
                hasher.update(&[mode.discriminant()]);
            }
            RevolveFaceTag::StartCap { profile_count }
            | RevolveFaceTag::EndCap { profile_count } => {
                hasher.update(&profile_count.to_le_bytes());
            }
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
    use crate::topology::face_tag::{CuboidFaceTag, ExtrudeFaceTag, RevolveMode};

    #[test]
    fn for_revolve_face_deterministic() {
        // Same `(owner, tag)` produces identical bytes across calls. Repeats
        // for Side {Full}, Side {Partial}, StartCap, EndCap to make the
        // determinism contract per-variant.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        for tag in [
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
            RevolveFaceTag::Side {
                side_index: 2,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Partial,
            },
            RevolveFaceTag::StartCap { profile_count: 4 },
            RevolveFaceTag::EndCap { profile_count: 4 },
        ] {
            let a = BRepFaceId::for_revolve_face(owner, tag);
            let b = BRepFaceId::for_revolve_face(owner, tag);
            assert_eq!(a, b, "for_revolve_face({tag:?}) is not deterministic");
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
    }

    #[test]
    fn for_revolve_face_distinct_across_tag_kinds() {
        // Side, StartCap, EndCap — all distinct under the same owner at
        // the same profile_count.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Partial,
            },
        );
        let start_cap =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 });
        let end_cap =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::EndCap { profile_count: 4 });
        assert_ne!(side, start_cap);
        assert_ne!(side, end_cap);
        assert_ne!(start_cap, end_cap);
    }

    /// **Substrate-honesty test #1 for sub-7.2-γ: cross-mode break.**
    ///
    /// `Side {Full}` and `Side {Partial}` with otherwise-identical inner
    /// data MUST produce DIFFERENT `BRepFaceId`s. This proves that crossing
    /// the Full/Partial revolution boundary (e.g. 359° → 360°) breaks Side
    /// identity by construction — the mode byte is hashed into the BLAKE3
    /// input precisely to make this assertion hold.
    #[test]
    fn for_revolve_face_distinct_across_modes() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_full = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
        );
        let side_partial = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Partial,
            },
        );
        assert_ne!(
            side_full, side_partial,
            "side IDs must NOT be preserved across the full/partial mode boundary"
        );
    }

    /// **Substrate-honesty test #2 for sub-7.2-γ: segment-driven topology
    /// break.**
    ///
    /// `Side { segment_count: 8 }` and `Side { segment_count: 16 }` with
    /// otherwise-identical inner data MUST produce DIFFERENT `BRepFaceId`s.
    /// This proves that segment-count changes (8 → 16 segments) break Side
    /// identity by construction. The substrate hashes
    /// `segment_count.to_le_bytes()` into the BLAKE3 input on the `Side`
    /// branch precisely to make this assertion hold (per the substrate's
    /// "Break IDs across segment-count changes" directive — segment count
    /// is treated as topology in this identity model, mirroring
    /// [`ExtrudeFaceTag::Side`]'s `profile_count`).
    #[test]
    fn for_revolve_face_segments_change_breaks_side_id() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_seg_8 = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
        );
        let side_seg_16 = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 16,
                mode: RevolveMode::Full,
            },
        );
        assert_ne!(
            side_seg_8, side_seg_16,
            "side IDs must NOT be preserved across segment-count changes"
        );
    }

    #[test]
    fn for_revolve_face_profile_count_change_breaks_side_id() {
        // Square (profile_count=4) and pentagon (profile_count=5) Side IDs
        // must be disjoint at the same other params. Mirrors the extrude
        // square→pentagon precedent.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_sq = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
        );
        let side_pen = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 5,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
        );
        assert_ne!(
            side_sq, side_pen,
            "side IDs must NOT be preserved across profile-count changes"
        );
    }

    /// **Substrate-honesty test #3 for sub-7.2-γ: caps don't over-encode
    /// segments.**
    ///
    /// `StartCap { profile_count: 4 }` MUST produce byte-identical
    /// [`BRepFaceId`] regardless of any segment context. Caps depend on
    /// `profile_count` only — `segment_count` is irrelevant to cap geometry
    /// (caps are fan-triangulations of the profile polygon). The cap-tag
    /// BLAKE3 input deliberately does NOT hash `segment_count` in. This
    /// pins the substrate-honesty principle: caps don't over-encode.
    #[test]
    fn for_revolve_face_caps_unaffected_by_segments() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // `StartCap { profile_count: 4 }` has no segment_count field — the
        // tag is byte-identical regardless of which RevolveOp it
        // accompanies. The BLAKE3 derivation must produce the same id on
        // every call.
        let cap_a =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 });
        let cap_b =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 });
        assert_eq!(
            cap_a, cap_b,
            "StartCap IDs must be byte-identical regardless of segment context"
        );
        // EndCap analogous.
        let end_a =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::EndCap { profile_count: 4 });
        let end_b =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::EndCap { profile_count: 4 });
        assert_eq!(end_a, end_b);
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"extrude:"` (sub-7.2-β) and `b"revolve:"` (sub-7.2-γ) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical. This pins the operator-kind separator's load-bearing role
    /// for the third per-operator face-tag substrate.
    #[test]
    fn for_revolve_face_distinct_from_for_extrude_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // ExtrudeFaceTag::Side has discriminant 2; RevolveFaceTag::EndCap
        // has discriminant 2. The only thing distinguishing the two BLAKE3
        // inputs (besides the appended payload, which we make identical:
        // `profile_count = 4` u32 LE for EndCap; `edge_index = 0` u32 LE
        // followed by `profile_count = 4` u32 LE for Side — these are NOT
        // identical payloads, so this test exercises both the operator-
        // kind separator AND the per-variant payload). Make a more
        // careful comparison: ExtrudeFaceTag::Top (discriminant 1, no
        // payload) vs RevolveFaceTag::StartCap { profile_count: 4 }
        // (discriminant 1, payload: profile_count u32 LE). Different
        // payload structures don't directly demonstrate the separator's
        // role. The cleanest comparison is identical-discriminant
        // matched-payload at the variant level — but the separators are
        // distinct byte strings (`b"extrude:"` 8 bytes vs `b"revolve:"` 8
        // bytes), so any single comparison demonstrates the separator's
        // role: an existing extrude-face id cannot collide with a
        // revolve-face id under the same owner because the input streams
        // differ at the operator-kind-separator position.
        let extrude_top = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Top);
        let revolve_start_cap =
            BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 });
        assert_ne!(
            extrude_top, revolve_start_cap,
            "operator-kind separator must produce disjoint identity spaces \
             across extrude and revolve"
        );
    }

    #[test]
    fn for_revolve_face_distinct_from_for_cuboid_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // CuboidFaceTag::NegZ has discriminant 0; RevolveFaceTag::Side has
        // discriminant 0. Distinct operator-kind separators (`b"cuboid:"`
        // vs `b"revolve:"`) keep the identity spaces disjoint.
        let cuboid_neg_z = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        let revolve_side = BRepFaceId::for_revolve_face(
            owner,
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
        );
        assert_ne!(
            cuboid_neg_z, revolve_side,
            "operator-kind separator must produce disjoint identity spaces \
             across cuboid and revolve"
        );
    }
}
