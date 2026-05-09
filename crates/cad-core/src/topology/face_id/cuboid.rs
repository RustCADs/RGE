//! `BRepFaceId` constructor + tests for `CuboidOp` (sub-7.2-α).

use super::{BRepFaceId, BRepOwnerId};
use crate::topology::face_tag::CuboidFaceTag;

impl BRepFaceId {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
