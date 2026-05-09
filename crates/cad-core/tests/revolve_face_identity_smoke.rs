//! End-to-end smoke for the sub-7.2-γ B-Rep face-identity substrate
//! (RevolveOp — categorical-mode + segment-driven topology, third operator).
//!
//! These tests are the gate for the dispatch — they prove (1) angle-stability
//! within Partial mode, (2) Side IDs break across the Full/Partial mode
//! boundary, (3) Side IDs break across segment-count changes, (4) cap IDs are
//! stable across segment-count changes (caps depend on profile_count only),
//! and (5) distinct owners produce disjoint identity spaces (mirroring the
//! cuboid/extrude precedent).
//!
//! `RevolveOp` introduces a topology axis no prior dispatch has touched: a
//! categorical mode change (`Full` vs `Partial` revolution) that alters the
//! face *set itself* (Full has no caps; Partial has caps).

use std::f32::consts::PI;

use rge_cad_core::{BRepFaceId, BRepOwnerId, BRepProvider, Polygon2D, RevolveOp};

/// Square on the +X side of the Y-axis — `(1,0)..(2,0)..(2,1)..(1,1)`. CCW.
fn ccw_square() -> Polygon2D {
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ccw +x square")
}

/// Within Partial mode, three angles `(45°, 135°, 180°)` with the same
/// profile and same segment count must preserve every face ID. Angles less
/// than 2π and within the same mode do not feed into the BLAKE3 derivation
/// — only `(side_index, profile_count, segment_count, mode)` for sides and
/// `profile_count` for caps. This is the rebuild-stability assertion of
/// sub-7.2-γ for the angle dimension within Partial mode.
#[test]
fn revolve_partial_face_ids_stable_across_angle_changes_within_partial_mode() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let square = ccw_square();
    let a = RevolveOp::partial(square.clone(), 8, PI / 4.0).expect("a");
    let b = RevolveOp::partial(square.clone(), 8, PI * 0.75).expect("b");
    let c = RevolveOp::partial(square, 8, PI).expect("c");

    let ids_a: Vec<BRepFaceId> = a
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_b: Vec<BRepFaceId> = b
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_c: Vec<BRepFaceId> = c
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(
        ids_a, ids_b,
        "angle change within Partial mode must preserve all face IDs"
    );
    assert_eq!(ids_b, ids_c);
    // n=4 sides + StartCap + EndCap = 6 face IDs.
    assert_eq!(ids_a.len(), 6);
}

/// Full and Partial modes produce disjoint Side identity spaces. Crossing
/// the Full/Partial boundary (e.g. 359° → 360°) flips the mode byte hashed
/// into the BLAKE3 input and breaks Side IDs by construction.
#[test]
fn revolve_face_ids_break_across_full_partial_boundary() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let square = ccw_square();
    let full = RevolveOp::new(square.clone(), 8).expect("full");
    // ~358° — definitively below the 2π clamp boundary so it stays Partial.
    let partial = RevolveOp::partial(square, 8, PI * 1.99).expect("near-full partial");

    let ids_full: Vec<BRepFaceId> = full
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_partial: Vec<BRepFaceId> = partial
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_full.len(), 4, "Full = sides only (n)");
    assert_eq!(ids_partial.len(), 6, "Partial = sides + 2 caps (n + 2)");

    // Side IDs must be disjoint across modes (mode discriminator hashed in).
    // Partial has sides at indices 0..4; Full has sides at indices 0..4.
    for full_side in &ids_full {
        for partial_side in &ids_partial[..4] {
            assert_ne!(
                full_side, partial_side,
                "Side IDs must NOT collide across full/partial boundary"
            );
        }
    }
}

/// `segment_count` is treated as topology in this substrate's identity
/// model. Changing segments from 8 → 16 must produce disjoint Side
/// identity spaces. Mirrors the extrude `profile_count` precedent.
#[test]
fn revolve_face_ids_break_across_segment_count_changes() {
    let owner = BRepOwnerId::from_bytes([0x56; 16]);
    let square = ccw_square();
    let r8 = RevolveOp::new(square.clone(), 8).expect("8 segments");
    let r16 = RevolveOp::new(square, 16).expect("16 segments");

    let ids_8: Vec<BRepFaceId> = r8
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_16: Vec<BRepFaceId> = r16
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_8.len(), 4);
    assert_eq!(ids_16.len(), 4);

    // No Side ID minted under 8 segments may appear in the 16-segment set.
    for id_8 in &ids_8 {
        assert!(
            !ids_16.contains(id_8),
            "Side IDs must NOT be preserved across segment-count changes"
        );
    }
}

/// Caps depend on `profile_count` only — segment_count must NOT
/// over-encode into them. This is the cap-stability substrate-honesty
/// case: an 8-segment partial revolution and a 16-segment partial
/// revolution at the same profile + same angle must produce
/// byte-identical StartCap / EndCap IDs (only the Side IDs differ).
#[test]
fn revolve_partial_caps_stable_across_segment_count_changes() {
    let owner = BRepOwnerId::from_bytes([0x78; 16]);
    let square = ccw_square();
    let p8 = RevolveOp::partial(square.clone(), 8, PI).expect("8 seg, π");
    let p16 = RevolveOp::partial(square, 16, PI).expect("16 seg, π");

    let ids_8 = p8.brep_face_ids(owner);
    let ids_16 = p16.brep_face_ids(owner);

    // Last two are caps (StartCap, EndCap). Side IDs differ (segments
    // differ), but cap IDs are byte-identical (caps depend on
    // profile_count only).
    assert_eq!(
        ids_8[ids_8.len() - 2].1,
        ids_16[ids_16.len() - 2].1,
        "StartCap stable across segments"
    );
    assert_eq!(
        ids_8[ids_8.len() - 1].1,
        ids_16[ids_16.len() - 1].1,
        "EndCap stable across segments"
    );
}

/// Mirrors the cuboid + extrude `*_face_ids_distinct_across_owners`
/// precedent. Different owners produce disjoint identity spaces for the
/// same operator.
#[test]
fn revolve_face_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let op = RevolveOp::partial(ccw_square(), 8, PI / 2.0).expect("op");

    let ids_x: Vec<BRepFaceId> = op
        .brep_face_ids(owner_x)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_y: Vec<BRepFaceId> = op
        .brep_face_ids(owner_y)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    // Disjoint sets — different owners produce different identity spaces
    // (caps included; `owner.as_bytes()` is hashed into every BLAKE3 input
    // regardless of the tag variant).
    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-disambiguation failed: id from owner_x found in owner_y's set"
        );
    }
}
