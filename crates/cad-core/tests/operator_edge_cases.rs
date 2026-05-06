//! Audit-2 A2.7 / A2.8 / A2.9 + #4 boundary closure: operator edge-case tests.
//!
//! Four boundary scenarios that the in-source `::tests` modules can no
//! longer host because the source files (`extrude.rs`, `revolve.rs`,
//! `infer.rs`) are at or near the 1000-line split-exemption cap. Per the
//! dispatch's scoping constraint, all new tests for these substrates must
//! live as integration tests under `crates/cad-core/tests/`.
//!
//! 1. **`extrude_op_accepts_epsilon_length`** — A2.7. `ExtrudeOp::new(_,
//!    f32::EPSILON)` must succeed (length > 0); evaluation produces a near-
//!    flat prism with z-values in `[0, EPSILON]`. Documents the boundary
//!    behavior for an extreme-but-valid input.
//! 2. **`revolve_new_and_partial_2pi_have_byte_identical_hash`** — A2.9.
//!    The HANDOFF prose documents that `RevolveOp::new(profile, segs)` is
//!    bit-identical to `RevolveOp::partial(profile, segs, 2π)` (the
//!    backwards-compat claim). This test asserts the claim end-to-end at
//!    the structural-hash level.
//! 3. **`tessellation_with_empty_face_labels_some_zero_vec`** — #4 boundary.
//!    `Tessellation::with_labels(vec![], vec![], vec![])` is the zero-
//!    triangle but-labeled case. Locks in: ctor accepts, `is_labeled() ==
//!    true`, `face_count() == Some(0)`.
//! 4. **`infer_lineage_with_empty_inputs`** — A2.8. Empty-labeled
//!    `Tessellation` round-trips through `infer_lineage` returning `Ok` with
//!    an empty `LineageGraph`.

use std::f32::consts::PI;

use rge_cad_core::{
    infer_lineage, ExtrudeOp, Operator, Polygon2D, RevolveOp, Tessellation, TopologyFaceId,
};

// ---------------------------------------------------------------------------
// 1. ExtrudeOp w/ EPSILON length (audit-2 A2.7)
// ---------------------------------------------------------------------------

/// `ExtrudeOp::new(triangle_profile, f32::EPSILON)` must succeed: `EPSILON`
/// is finite and `> 0`. The resulting prism is degenerately flat
/// (z-values in `[0, EPSILON]`) but every invariant is preserved.
///
/// This locks in the lower-end length boundary.
#[test]
fn extrude_op_accepts_epsilon_length() {
    let triangle_profile =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle profile");

    // EPSILON is finite + > 0, so the constructor MUST succeed.
    let op = ExtrudeOp::new(triangle_profile, f32::EPSILON).expect(
        "ExtrudeOp::new with f32::EPSILON length must succeed (length > 0 is the only contract)",
    );
    // Bit-equality check via to_bits — we genuinely want exact equality
    // (the constructor stored the supplied `f32::EPSILON` unchanged), and
    // float-tolerance comparisons are not appropriate at this boundary.
    assert_eq!(op.length.to_bits(), f32::EPSILON.to_bits());

    // Evaluation produces a valid (but near-flat) tessellation.
    let tess = op
        .evaluate(&[])
        .expect("evaluate of EPSILON-length extrude must succeed");

    // n=3 → 2n=6 vertices, 4n-4=8 triangles.
    assert_eq!(
        tess.vertex_count(),
        6,
        "EPSILON-length extrude of a triangle should still emit 6 vertices"
    );
    assert_eq!(
        tess.triangle_count(),
        8,
        "EPSILON-length extrude of a triangle should still emit 8 triangles"
    );

    // Every vertex z lies in [0, EPSILON]. The bottom ring is at z=0 and the
    // top ring is at z=EPSILON.
    for [_, _, z] in &tess.positions {
        assert!(
            *z >= 0.0 && *z <= f32::EPSILON,
            "EPSILON-length extrude must keep z in [0, EPSILON]; got {z}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. RevolveOp::new ↔ RevolveOp::partial(profile, segs, 2π) hash equality
//    (audit-2 A2.9 — backwards-compat claim from HANDOFF prose)
// ---------------------------------------------------------------------------

/// HANDOFF.md change.md's D-Partial-Revolve entry states "existing
/// `new(profile, segments)` delegates to `partial(p, segs, 2π)` so
/// backwards-compat is bit-identical". This test asserts the bit-identical
/// claim at the `structural_hash` level.
///
/// `structural_hash` includes `angle.to_le_bytes()`, so any drift in the
/// stored angle field would surface here. The constructors clamp to exactly
/// `2π` for both paths (within `1e-5`), so the bytes must match exactly.
#[test]
fn revolve_new_and_partial_2pi_have_byte_identical_hash() {
    let profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
        .expect("revolve square profile");

    let via_new = RevolveOp::new(profile.clone(), 8).expect("new");
    let via_partial = RevolveOp::partial(profile, 8, 2.0 * PI).expect("partial 2π");

    // The angle field must clamp to exactly 2π for both paths.
    assert!(
        (via_new.angle - 2.0 * PI).abs() < f32::EPSILON,
        "via_new.angle = {} must equal 2π exactly post-clamp",
        via_new.angle
    );
    assert!(
        (via_partial.angle - 2.0 * PI).abs() < f32::EPSILON,
        "via_partial.angle = {} must equal 2π exactly post-clamp",
        via_partial.angle
    );

    // Both must claim full revolution.
    assert!(via_new.is_full_revolution());
    assert!(via_partial.is_full_revolution());

    // Structural hashes must be byte-identical — this is the back-compat
    // gate. If hashes diverge, downstream tessellation caches keyed on
    // `effective_hash` would invalidate post-refactor, breaking the
    // "bit-identical to legacy" promise.
    assert_eq!(
        via_new.structural_hash(),
        via_partial.structural_hash(),
        "RevolveOp::new(profile, 8) and RevolveOp::partial(profile, 8, 2π) \
         must produce byte-identical structural_hash — back-compat claim"
    );

    // Cross-check: `partial` with a slightly-off angle (well outside the
    // 1e-5 clamp window) must produce a DIFFERENT hash, proving the hash
    // is angle-sensitive in general (and thus the equality above is
    // meaningful, not a coincidence).
    let via_partial_other =
        RevolveOp::partial(via_new.profile.clone(), 8, 0.5 * PI).expect("partial π/2");
    assert_ne!(
        via_new.structural_hash(),
        via_partial_other.structural_hash(),
        "hash must be angle-sensitive in general (sanity check)"
    );
}

// ---------------------------------------------------------------------------
// 3. Tessellation::with_labels(vec![], vec![], vec![]) boundary (audit-2 #4)
// ---------------------------------------------------------------------------

/// The labeled-but-zero-triangle case is degenerate but must be accepted.
/// Empty positions + empty indices + empty `face_labels` satisfy every
/// invariant: `indices.len() % 3 == 0` (`0 % 3 == 0`), no out-of-bounds, and
/// `face_labels.len() == indices.len() / 3` (`0 == 0`).
#[test]
fn tessellation_with_empty_face_labels_some_zero_vec() {
    let tess = Tessellation::with_labels(Vec::new(), Vec::new(), Vec::new())
        .expect("Tessellation::with_labels with all-empty inputs must succeed");

    assert_eq!(tess.vertex_count(), 0);
    assert_eq!(tess.triangle_count(), 0);
    // Crucial: the labeled-state flag is TRUE because we went through
    // with_labels. Some(empty_vec) is not None.
    assert!(
        tess.is_labeled(),
        "with_labels with empty vec must produce is_labeled() == true"
    );
    assert_eq!(
        tess.face_count(),
        Some(0),
        "labeled-but-zero-triangle mesh must report face_count() == Some(0)"
    );
    // The label slice is present and empty.
    let labels = tess.face_labels().expect("with_labels must yield Some");
    assert_eq!(labels.len(), 0, "label slice must be empty");
}

// ---------------------------------------------------------------------------
// 4. infer_lineage with empty inputs (audit-2 A2.8)
// ---------------------------------------------------------------------------

/// `infer_lineage(empty_labeled_input, empty_output, base_id)` must return
/// `Ok((output_with_labels, LineageGraph))` where:
///
/// * the returned tessellation is labeled (the unified `infer_lineage`
///   upgrades unlabeled output via `label_by_plane` internally — if output
///   is also empty, the labeled-mesh contains no triangles either)
/// * `LineageGraph::is_empty()` returns `true`
#[test]
fn infer_lineage_with_empty_inputs() {
    // Build empty labeled input — the unified infer_lineage requires a
    // labeled input.
    let empty_input =
        Tessellation::with_labels(Vec::new(), Vec::new(), Vec::new()).expect("empty labeled input");
    assert!(empty_input.is_labeled());

    // Output: also empty. Test both labeled and unlabeled paths.
    {
        // Labeled-output path.
        let empty_labeled_output =
            Tessellation::with_labels(Vec::new(), Vec::new(), Vec::new()).expect("empty labeled");
        let (out_tess, lineage) =
            infer_lineage(&empty_input, &empty_labeled_output, 100).expect("labeled-output path");
        assert!(out_tess.is_labeled(), "labeled-output path returns labeled");
        assert_eq!(out_tess.triangle_count(), 0);
        assert!(
            lineage.is_empty(),
            "empty-input + empty-output must produce empty lineage; got {} edges",
            lineage.len()
        );
    }
    {
        // Unlabeled-output path (plane-equation heuristic). The internal
        // `label_by_plane` produces an empty labeled output.
        let empty_unlabeled_output =
            Tessellation::new(Vec::new(), Vec::new()).expect("empty unlabeled");
        assert!(!empty_unlabeled_output.is_labeled());
        let (out_tess, lineage) = infer_lineage(&empty_input, &empty_unlabeled_output, 100)
            .expect("unlabeled-output path");
        // The unified function relabels via label_by_plane → labeled output.
        assert!(out_tess.is_labeled());
        assert_eq!(out_tess.triangle_count(), 0);
        assert!(
            lineage.is_empty(),
            "empty-input + empty-unlabeled-output must produce empty lineage; \
             got {} edges",
            lineage.len()
        );
    }

    // Non-empty input + empty output is the asymmetric edge case: every
    // input face is "Deleted" because no output match exists. This is also
    // covered by the existing topology_lineage_smoke test for the
    // intersection-yields-empty case but pinning it here for the
    // empty-output boundary specifically (one input triangle + zero
    // output triangles).
    let non_empty_positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let non_empty_indices = vec![0_u32, 1, 2];
    let non_empty_input = Tessellation::with_labels(
        non_empty_positions,
        non_empty_indices,
        vec![TopologyFaceId(0)],
    )
    .expect("non-empty labeled input");
    let empty_output = Tessellation::new(Vec::new(), Vec::new()).expect("empty");
    let (_tess, lineage) =
        infer_lineage(&non_empty_input, &empty_output, 0).expect("infer non-empty → empty");
    // Locked-in: every input face that has no output match becomes a
    // Deleted edge. There are no Reinterpreted edges (no output to be
    // reinterpreted from).
    let deleted_count = lineage
        .edges_by_evolution(rge_cad_core::TopologyEvolution::Deleted)
        .count();
    assert!(
        deleted_count >= 1,
        "non-empty → empty lineage should report at least 1 Deleted edge; got {deleted_count}"
    );
}
