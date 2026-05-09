//! End-to-end smoke for the sub-7.2-β B-Rep face-identity substrate
//! (ExtrudeOp — variable-topology second operator).
//!
//! These tests are the gate for the dispatch — they prove that
//! `BRepFaceId`s seeded by a caller-supplied `BRepOwnerId` are byte-identical
//! across `length` parameter rebuilds of `ExtrudeOp` (rebuild stability for
//! a fixed profile), that profile-count changes break face identity for the
//! `Side` variants by construction (substrate honesty under topology
//! change), and that distinct owners produce disjoint identity spaces
//! (owner disambiguation, mirroring the cuboid precedent).

use rge_cad_core::{BRepFaceId, BRepOwnerId, BRepProvider, ExtrudeOp, Polygon2D};

/// Same profile, three different `length` values, six `BRepFaceId`s
/// byte-identical across all three rebuilds.
///
/// This is the rebuild-stability assertion of sub-7.2-β: changing only
/// `length` (the prism's height) MUST NOT alter the derived face identity,
/// because the BLAKE3 derivation feeds only `(domain, owner, kind, tag)` —
/// none of which vary with `length`. (For the `Side` variants, `tag`
/// includes `edge_index` and `profile_count`; both unchanged here.)
#[test]
fn extrude_face_ids_stable_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let square = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("ccw unit square");
    let a = ExtrudeOp::new(square.clone(), 1.0).expect("a");
    let b = ExtrudeOp::new(square.clone(), 2.0).expect("b");
    let c = ExtrudeOp::new(square, 0.5).expect("c");

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

    assert_eq!(ids_a, ids_b);
    assert_eq!(ids_b, ids_c);
    // 4 sides + Bottom cap + Top cap = 6 face IDs.
    assert_eq!(ids_a.len(), 6);
}

/// Square (N=4) vs pentagon (N=5) — different topology, identity must NOT
/// be silently preserved.
///
/// This is the substrate-honesty test for sub-7.2-β. Bottom and Top IDs
/// MAY (and per spec DO) match between the two because their BLAKE3 input
/// ends after the discriminant byte — there is no `profile_count` field
/// hashed in for caps. But every `Side` ID MUST differ between square and
/// pentagon because `profile_count` is in the `Side` tag and is hashed
/// into the BLAKE3 input. The square's 4 side IDs and the pentagon's 5
/// side IDs are disjoint sets.
#[test]
fn extrude_face_ids_break_when_profile_count_changes() {
    let owner = BRepOwnerId::from_bytes([0xef; 16]);
    let square = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("ccw unit square");
    // Regular pentagon at unit radius — 5 finite CCW points.
    let pentagon = Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("regular pentagon");
    let extr_sq = ExtrudeOp::new(square, 1.0).expect("square extrude");
    let extr_pen = ExtrudeOp::new(pentagon, 1.0).expect("pentagon extrude");

    let ids_sq: Vec<BRepFaceId> = extr_sq
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_pen: Vec<BRepFaceId> = extr_pen
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_sq.len(), 6); // 4 sides + Bottom + Top
    assert_eq!(ids_pen.len(), 7); // 5 sides + Bottom + Top

    // Bottom (index 0) and Top (index 1) IDs are EXPECTED to match across
    // the two — caps' BLAKE3 input ends after the discriminant byte and
    // does NOT hash `profile_count` in. This is part of the v0 substrate
    // contract pinned in `ExtrudeFaceTag`'s docstring (caps stable across
    // both length AND profile_count changes).
    assert_eq!(
        ids_sq[0], ids_pen[0],
        "Bottom IDs must match across topology change"
    );
    assert_eq!(
        ids_sq[1], ids_pen[1],
        "Top IDs must match across topology change"
    );

    // The break-point: every Side ID differs between square and pentagon.
    // The square has 4 sides at indices 2..6 and the pentagon has 5 sides
    // at indices 2..7. NO pair of side IDs across the two should collide,
    // because `profile_count` (4 vs 5) is hashed into each side's BLAKE3
    // input.
    for sq_side in &ids_sq[2..] {
        for pen_side in &ids_pen[2..] {
            assert_ne!(
                sq_side, pen_side,
                "side IDs must not collide across topology change"
            );
        }
    }
}

/// Different `BRepOwnerId`s produce disjoint identity spaces — no
/// `BRepFaceId` minted under one owner collides with any minted under
/// another. Mirrors the cuboid `cuboid_face_ids_distinct_across_owners`
/// precedent.
#[test]
fn extrude_face_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let square = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("ccw unit square");
    let op = ExtrudeOp::new(square, 1.0).expect("op");

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
    // for the same operator (caps included; `owner.as_bytes()` is hashed
    // into every BLAKE3 input regardless of the tag variant).
    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-disambiguation failed: id from owner_x found in owner_y's set"
        );
    }
}
