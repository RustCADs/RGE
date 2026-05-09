//! Per-operator face-tag enums.
//!
//! In sub-7.2-α only [`CuboidFaceTag`] existed. Sub-7.2-β adds
//! [`ExtrudeFaceTag`] — the second per-operator face-tag enum, this time
//! with **variable topology** (`N + 2` faces depending on profile vertex
//! count). Per-operator face-tag enums for the remaining operators
//! (`RevolveOp` / `BooleanOp` / `LoftOp` / `SweepOp` / `TransformOp`) are
//! explicitly OUT OF SCOPE for sub-7.2-β and land in subsequent
//! sub-dispatches when each operator's `BRepProvider` impl ships.

use serde::{Deserialize, Serialize};

/// Face-tag enumeration for [`crate::operators::CuboidOp`].
///
/// The variant order matches the canonical face-emission order of
/// `CuboidOp::evaluate`:
///
/// ```text
/// 0: NegZ  (back, -Z normal)
/// 1: PosZ  (front, +Z normal)
/// 2: NegY  (bottom, -Y normal)
/// 3: PosY  (top, +Y normal)
/// 4: NegX  (left, -X normal)
/// 5: PosX  (right, +X normal)
/// ```
///
/// **Do not reorder** these variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CuboidFaceTag {
    /// `-Z` face — back of the box (outward normal `(0, 0, -1)`).
    NegZ,
    /// `+Z` face — front of the box (outward normal `(0, 0, +1)`).
    PosZ,
    /// `-Y` face — bottom of the box (outward normal `(0, -1, 0)`).
    NegY,
    /// `+Y` face — top of the box (outward normal `(0, +1, 0)`).
    PosY,
    /// `-X` face — left side of the box (outward normal `(-1, 0, 0)`).
    NegX,
    /// `+X` face — right side of the box (outward normal `(+1, 0, 0)`).
    PosX,
}

impl CuboidFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_cuboid_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// NegZ = 0, PosZ = 1, NegY = 2, PosY = 3, NegX = 4, PosX = 5
    /// ```
    ///
    /// These discriminants are part of the stable id substrate's wire surface
    /// and MUST NOT change without a `v2` migration in the domain separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            CuboidFaceTag::NegZ => 0,
            CuboidFaceTag::PosZ => 1,
            CuboidFaceTag::NegY => 2,
            CuboidFaceTag::PosY => 3,
            CuboidFaceTag::NegX => 4,
            CuboidFaceTag::PosX => 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminant_matches_canonical_emission_order() {
        // The frozen discriminants must match the order CuboidOp emits faces
        // in `evaluate` (-Z, +Z, -Y, +Y, -X, +X). This test pins the contract.
        assert_eq!(CuboidFaceTag::NegZ.discriminant(), 0);
        assert_eq!(CuboidFaceTag::PosZ.discriminant(), 1);
        assert_eq!(CuboidFaceTag::NegY.discriminant(), 2);
        assert_eq!(CuboidFaceTag::PosY.discriminant(), 3);
        assert_eq!(CuboidFaceTag::NegX.discriminant(), 4);
        assert_eq!(CuboidFaceTag::PosX.discriminant(), 5);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        for tag in [
            CuboidFaceTag::NegZ,
            CuboidFaceTag::PosZ,
            CuboidFaceTag::NegY,
            CuboidFaceTag::PosY,
            CuboidFaceTag::NegX,
            CuboidFaceTag::PosX,
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: CuboidFaceTag = ron::from_str(&s).expect("deserialize");
            assert_eq!(tag, decoded);
        }
    }

    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn non_exhaustive_pattern_match_compiles() {
        let tag = CuboidFaceTag::NegZ;
        let _label = match tag {
            CuboidFaceTag::NegZ => "neg-z",
            CuboidFaceTag::PosZ => "pos-z",
            CuboidFaceTag::NegY => "neg-y",
            CuboidFaceTag::PosY => "pos-y",
            CuboidFaceTag::NegX => "neg-x",
            CuboidFaceTag::PosX => "pos-x",
            _ => "future-variant",
        };
    }

    // -----------------------------------------------------------------------
    // ExtrudeFaceTag tests (sub-7.2-β)
    // -----------------------------------------------------------------------

    #[test]
    fn extrude_face_tag_serde_round_trip() {
        for tag in [
            ExtrudeFaceTag::Bottom,
            ExtrudeFaceTag::Top,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
            ExtrudeFaceTag::Side {
                edge_index: 3,
                profile_count: 4,
            },
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 5,
            },
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: ExtrudeFaceTag = ron::from_str(&s).expect("deserialize");
            assert_eq!(tag, decoded);
        }
    }

    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn extrude_face_tag_non_exhaustive_pattern_compiles() {
        let tag = ExtrudeFaceTag::Bottom;
        let _label = match tag {
            ExtrudeFaceTag::Bottom => "bottom",
            ExtrudeFaceTag::Top => "top",
            ExtrudeFaceTag::Side { .. } => "side",
            _ => "future-variant",
        };
    }

    #[test]
    fn extrude_side_distinct_for_distinct_edge_indices() {
        // Constructor-level — verify `Side { edge_index: 0, count: 4 }` and
        // `Side { edge_index: 1, count: 4 }` produce distinct tag values via
        // PartialEq. This pins the tag-level distinctness independently of
        // the BLAKE3 derivation that face_id.rs tests cover.
        let s0 = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 4,
        };
        let s1 = ExtrudeFaceTag::Side {
            edge_index: 1,
            profile_count: 4,
        };
        assert_ne!(s0, s1);

        // Cross-check: same (edge_index, profile_count) ARE equal.
        let s0_again = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 4,
        };
        assert_eq!(s0, s0_again);

        // Same edge_index, different profile_count, also distinct.
        let s0_other_count = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 5,
        };
        assert_ne!(s0, s0_other_count);

        // Bottom and Top are distinct from any Side and from each other.
        assert_ne!(ExtrudeFaceTag::Bottom, ExtrudeFaceTag::Top);
        assert_ne!(ExtrudeFaceTag::Bottom, s0);
        assert_ne!(ExtrudeFaceTag::Top, s0);
    }
}

// ---------------------------------------------------------------------------
// ExtrudeFaceTag (sub-7.2-β)
// ---------------------------------------------------------------------------

/// Face-tag enumeration for [`crate::operators::ExtrudeOp`].
///
/// `ExtrudeOp` has **variable topology**: a profile of `N` vertices produces
/// `N + 2` faces in the canonical emission order
/// `Bottom (1 face) → Top (1 face) → Side(0..N-1) (N faces)`. The variant
/// order matches that emission order; the discriminant pinned in
/// [`ExtrudeFaceTag::discriminant`] freezes
/// `Bottom = 0`, `Top = 1`, `Side = 2`. The inner data of `Side` is
/// BLAKE3-hashed (NOT used as the discriminant byte).
///
/// # Stability contract (load-bearing)
///
/// 1. **Bottom and Top IDs are stable across `length` parameter changes.**
///    The substrate hashes only the discriminant byte for these two
///    variants, not the operator's parameters, so changing `length` from
///    `1.0` to `2.0` does NOT invalidate face identity for the caps.
/// 2. **`Side { edge_index, profile_count }` IDs are stable when both
///    `edge_index` and `profile_count` are unchanged.** Changing only
///    `length` does not invalidate any side's identity, mirroring the cap
///    behaviour.
/// 3. **Profile-count changes break `Side` IDs by construction.** The
///    `profile_count` field is hashed into the BLAKE3 input; a square
///    (`profile_count = 4`) and a pentagon (`profile_count = 5`) produce
///    disjoint side-identity spaces because the input bytes differ. This is
///    the load-bearing design choice — topology changes are NOT silently
///    preserved.
/// 4. **Profile-vertex-order rotation at the same count preserves `Side`
///    IDs.** The substrate does NOT inspect profile coordinates, so a
///    profile rotated from `[A, B, C, D]` to `[B, C, D, A]` will produce
///    the same `Side(0)` ID. This is an explicit limit of the v0
///    substrate; coordinate-aware identity (rotation detection, vertex
///    matching across re-ordering) is OUT OF SCOPE for sub-7.2-β.
///
/// **Do not reorder** the variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExtrudeFaceTag {
    /// `-Z` cap (bottom of the prism, outward normal `(0, 0, -1)`).
    ///
    /// The discriminant for `Bottom` is `0`. The BLAKE3 input ends after
    /// the discriminant byte — no inner data — so `Bottom` IDs are stable
    /// across `length` AND `profile_count` changes.
    Bottom,
    /// `+Z` cap (top of the prism, outward normal `(0, 0, +1)`).
    ///
    /// The discriminant for `Top` is `1`. The BLAKE3 input ends after the
    /// discriminant byte — no inner data — so `Top` IDs are stable across
    /// `length` AND `profile_count` changes.
    Top,
    /// One side wall of the prism, indexed by the profile edge it spans.
    ///
    /// `edge_index` is the index of the profile edge `(i, i + 1)` mod
    /// `profile_count`, with the canonical emission order
    /// `Side(0), Side(1), ..., Side(profile_count - 1)`. `profile_count` is
    /// the total profile vertex count and is hashed into the BLAKE3 input
    /// alongside `edge_index` so topology changes (square → pentagon) break
    /// face identity by construction.
    ///
    /// The discriminant for `Side` is `2`. The BLAKE3 input appends
    /// `edge_index.to_le_bytes()` (4 bytes) followed by
    /// `profile_count.to_le_bytes()` (4 bytes) after the discriminant byte.
    Side {
        /// Index of the profile edge `(i, i + 1) mod profile_count` this
        /// side wall spans. Range `0..profile_count`.
        edge_index: u32,
        /// Total profile vertex count. Hashed into the BLAKE3 input so
        /// topology changes (e.g. square → pentagon) break face identity
        /// for `Side` variants by construction.
        profile_count: u32,
    },
}

impl ExtrudeFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_extrude_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// Bottom = 0, Top = 1, Side = 2
    /// ```
    ///
    /// The inner data of `Side` (`edge_index`, `profile_count`) is NOT used
    /// as the discriminant — it is appended to the BLAKE3 input separately.
    /// These discriminants are part of the stable id substrate's wire
    /// surface and MUST NOT change without a `v2` migration in the domain
    /// separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            ExtrudeFaceTag::Bottom => 0,
            ExtrudeFaceTag::Top => 1,
            ExtrudeFaceTag::Side { .. } => 2,
        }
    }
}
