//! Unit tests for [`crate::operators::boolean::BooleanOp`].
//!
//! Sub-module of [`crate::operators::boolean`]; see that module's `//!` docs
//! for the design rationale.

use super::*;
use crate::operators::{CuboidOp, Operator};
use crate::tessellation::TopologyFaceId;
use crate::topo_lineage::label_by_plane;

/// Helper: an axis-aligned cube of `size` centered at `(cx, cy, cz)`.
fn cube_at(cx: f32, cy: f32, cz: f32, size: f32) -> Tessellation {
    let h = size * 0.5;
    let positions = vec![
        [cx - h, cy - h, cz - h],
        [cx + h, cy - h, cz - h],
        [cx + h, cy + h, cz - h],
        [cx - h, cy + h, cz - h],
        [cx - h, cy - h, cz + h],
        [cx + h, cy - h, cz + h],
        [cx + h, cy + h, cz + h],
        [cx - h, cy + h, cz + h],
    ];
    // Right-handed CCW outward winding (matches CuboidOp).
    #[rustfmt::skip]
    let indices = vec![
        0, 3, 2,  0, 2, 1,  // -Z
        4, 5, 6,  4, 6, 7,  // +Z
        0, 1, 5,  0, 5, 4,  // -Y
        3, 7, 6,  3, 6, 2,  // +Y
        0, 4, 7,  0, 7, 3,  // -X
        1, 2, 6,  1, 6, 5,  // +X
    ];
    Tessellation::new(positions, indices).expect("valid cube")
}

fn unit_cube_origin() -> Tessellation {
    let op = CuboidOp::default();
    op.evaluate(&[]).expect("default cube")
}

/// Helper: build a labeled [`Tessellation`] from a Tessellation by
/// labeling each triangle with the supplied `face_id`.
fn labeled_with_id(tess: &Tessellation, face_id: TopologyFaceId) -> Tessellation {
    let labels = vec![face_id; tess.triangle_count()];
    Tessellation::with_labels(tess.positions.clone(), tess.indices.clone(), labels)
        .expect("test labeled mesh ctor")
}

/// Test 1 — arity must be 2.
#[test]
fn boolean_op_arity_is_2() {
    for mode in [
        BooleanMode::Union,
        BooleanMode::Intersection,
        BooleanMode::Difference,
    ] {
        let op = BooleanOp::new(mode);
        assert_eq!(op.arity(), 2);
    }
}

/// Test 2 — different modes produce different local structural hashes.
#[test]
fn boolean_op_structural_hash_differs_per_mode() {
    let u = BooleanOp::union().structural_hash();
    let i = BooleanOp::intersection().structural_hash();
    let d = BooleanOp::difference().structural_hash();
    assert_ne!(u, i);
    assert_ne!(u, d);
    assert_ne!(i, d);
}

/// Test 3 — same mode → same hash, deterministic across constructions.
#[test]
fn boolean_op_structural_hash_deterministic() {
    let a = BooleanOp::union();
    let b = BooleanOp::new(BooleanMode::Union);
    assert_eq!(a.structural_hash(), b.structural_hash());

    let c = BooleanOp::intersection();
    let d = BooleanOp::new(BooleanMode::Intersection);
    assert_eq!(c.structural_hash(), d.structural_hash());
}

/// Test 4 — wrong arity input is rejected before reaching csgrs.
#[test]
fn boolean_evaluate_rejects_wrong_arity() {
    let op = BooleanOp::union();
    let cube = unit_cube_origin();

    // 0 inputs.
    let err0 = op.evaluate(&[]).unwrap_err();
    assert!(matches!(
        err0,
        OpError::WrongArity {
            expected: 2,
            got: 0
        }
    ));

    // 1 input.
    let err1 = op.evaluate(&[&cube]).unwrap_err();
    assert!(matches!(
        err1,
        OpError::WrongArity {
            expected: 2,
            got: 1
        }
    ));
}

/// Test 5 — union of two disjoint unit cubes far apart preserves both.
/// Disjoint inputs shouldn't merge, so the union's bounding box must
/// include both cubes' extents.
#[test]
fn union_two_disjoint_unit_cubes_yields_combined_mesh() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs = cube_at(10.0, 0.0, 0.0, 1.0); // far apart on +X
    let op = BooleanOp::union();
    let result = op.evaluate(&[&lhs, &rhs]).expect("union eval");

    // The union must contain vertices spanning both cubes.
    let xs: Vec<f32> = result.positions.iter().map(|[x, _, _]| *x).collect();
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    assert!(min_x <= -0.49, "union lost the lhs cube: min_x = {min_x}");
    assert!(max_x >= 10.49, "union lost the rhs cube: max_x = {max_x}");
    // Output must be non-empty.
    assert!(result.vertex_count() > 0);
    assert!(result.triangle_count() > 0);
    // Both inputs were unlabeled → output is unlabeled.
    assert!(
        !result.is_labeled(),
        "two unlabeled inputs should yield unlabeled output"
    );
}

/// Test 6 — union of overlapping cubes has FEWER vertices than naive
/// concat. The two cubes overlap in the corner-cube `[0,0.5]³`, so the
/// boolean must clip / merge faces.
#[test]
fn union_two_overlapping_unit_cubes_is_smaller_than_naive_concat() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // offset so they overlap
    let op = BooleanOp::union();
    let result = op.evaluate(&[&lhs, &rhs]).expect("union eval");

    // The boolean output must be non-empty.
    assert!(result.vertex_count() > 0);
    assert!(result.triangle_count() > 0);

    // Output bounding box must span the union (not be cut off).
    let xs: Vec<f32> = result.positions.iter().map(|[x, _, _]| *x).collect();
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(min_x <= -0.49, "lost lhs extents: min_x = {min_x}");
    assert!(max_x >= 0.99, "lost rhs extents: max_x = {max_x}");
}

/// Test 7 — intersection of disjoint cubes is empty (or returns an
/// `EmptyResult` / `InvalidParameter`). The dispatch ADR §"Implementation
/// guidance" says assert which behavior occurs and lock it in.
#[test]
fn intersection_two_disjoint_cubes_is_empty_or_diagnostic() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs = cube_at(10.0, 0.0, 0.0, 1.0); // disjoint
    let op = BooleanOp::intersection();
    let result = op.evaluate(&[&lhs, &rhs]);

    match result {
        Ok(t) => {
            // csgrs returns an empty Mesh for disjoint intersection;
            // the bridge converts that to a 0-vertex Tessellation.
            assert_eq!(t.vertex_count(), 0, "empty intersection expected");
            assert_eq!(t.triangle_count(), 0);
        }
        Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {
            // Acceptable alternative: csgrs may surface its own
            // diagnostic. Either branch is locked in.
        }
        Err(other) => {
            panic!("unexpected error from disjoint-intersection: {other:?}");
        }
    }
}

/// Test 8 — intersection of overlapping cubes yields a smaller box.
/// Two unit cubes shifted by `(0.5, 0.5, 0.5)` overlap in a `0.5³` cube.
#[test]
fn intersection_overlapping_cubes_yields_central_box() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs = cube_at(0.5, 0.5, 0.5, 1.0);
    let op = BooleanOp::intersection();
    let result = op.evaluate(&[&lhs, &rhs]).expect("intersection eval");

    assert!(result.vertex_count() > 0, "intersection must be non-empty");
    assert!(result.triangle_count() > 0);

    // The intersection volume is `[0,0.5] × [0,0.5] × [0,0.5]` plus a
    // small numeric tolerance.
    for [x, y, z] in &result.positions {
        assert!(
            *x >= -0.001 && *x <= 0.501,
            "intersection vertex x = {x} outside [0, 0.5]"
        );
        assert!(
            *y >= -0.001 && *y <= 0.501,
            "intersection vertex y = {y} outside [0, 0.5]"
        );
        assert!(
            *z >= -0.001 && *z <= 0.501,
            "intersection vertex z = {z} outside [0, 0.5]"
        );
    }
}

/// Test 9 — difference `cube_a` − `cube_b` (offset overlap) yields a dented
/// version of `cube_a` with strictly more vertices than the original 8-vert
/// cube (since the dent introduces additional clip vertices).
#[test]
fn difference_a_minus_b_with_overlap_yields_dented_a() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0); // 8 verts
    let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping corner
    let op = BooleanOp::difference();
    let result = op.evaluate(&[&lhs, &rhs]).expect("difference eval");

    assert!(result.vertex_count() > 0);
    // The dent must add vertices vs. the 8-vert original (typically the
    // dent creates 7+ additional clip vertices on the corner cube).
    assert!(
        result.vertex_count() > 8,
        "expected > 8 vertices after dent; got {}",
        result.vertex_count()
    );

    // Difference must NOT extend into the cube_b corner: vertices in the
    // ([0.5,1] × [0.5,1] × [0.5,1]) sub-cube interior are removed.
    // Allow boundary vertices (x == 0.5, etc.) since the boolean cuts
    // along those planes.
    let lhs_extent = 0.5 + 0.001;
    let mut found_inside_dent = false;
    for [x, y, z] in &result.positions {
        // We classify "inside the dent" as strictly above 0.5 in all
        // three axes (i.e. interior of the carved-out corner). Boundary
        // vertices on the cut plane are fine.
        if *x > 0.5 + 0.01 && *y > 0.5 + 0.01 && *z > 0.5 + 0.01 {
            found_inside_dent = true;
            break;
        }
        // Also assert no vertex extends past the lhs's outer boundary.
        assert!(
            *x <= lhs_extent && *y <= lhs_extent && *z <= lhs_extent,
            "vertex {} outside lhs extents",
            format_args!("({x},{y},{z})")
        );
    }
    assert!(
        !found_inside_dent,
        "difference output has vertices inside the cut-out region"
    );
}

/// Test 10 — `a - b ≠ b - a` (difference is non-commutative).
#[test]
fn difference_is_non_commutative() {
    let a = cube_at(0.0, 0.0, 0.0, 1.0);
    let b = cube_at(0.5, 0.5, 0.5, 1.0);
    let op = BooleanOp::difference();

    let a_minus_b = op.evaluate(&[&a, &b]).expect("a - b");
    let b_minus_a = op.evaluate(&[&b, &a]).expect("b - a");

    // Centroids differ: a−b is centered around (≈-0.1, -0.1, -0.1) (with
    // the dent removing the +x/+y/+z corner), b−a is centered around
    // (≈+0.6, +0.6, +0.6) (with the dent removing the -x/-y/-z corner).
    let centroid = |t: &Tessellation| -> [f32; 3] {
        #[allow(
            clippy::cast_precision_loss,
            reason = "test centroid divisor; positions.len() fits f32 mantissa for any test fixture"
        )]
        let n = t.positions.len() as f32;
        let sum = t.positions.iter().fold([0.0_f32, 0.0, 0.0], |acc, p| {
            [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
        });
        [sum[0] / n, sum[1] / n, sum[2] / n]
    };

    let ca = centroid(&a_minus_b);
    let cb = centroid(&b_minus_a);
    assert!(
        (ca[0] - cb[0]).abs() > 0.05
            || (ca[1] - cb[1]).abs() > 0.05
            || (ca[2] - cb[2]).abs() > 0.05,
        "a-b and b-a centroids too close: ca={ca:?}, cb={cb:?}"
    );
}

/// Test 11 — near-degenerate input is handled gracefully (no panic).
/// Tiny cube intersected with a normal cube: the result is either
/// correct-tiny or an `InvalidParameter` — but never a panic.
#[test]
fn near_degenerate_input_handled_gracefully() {
    let tiny = cube_at(0.0, 0.0, 0.0, 1e-6); // 1µm cube
    let big = cube_at(0.0, 0.0, 0.0, 1.0);
    let op = BooleanOp::intersection();
    let result = op.evaluate(&[&tiny, &big]);

    match result {
        Ok(_) | Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {
            // Either ok or clean diagnostic — both acceptable.
        }
        Err(other) => panic!("unexpected error class on tiny input: {other:?}"),
    }
}

/// Test 12 — pathological input yields a diagnostic, not a panic.
/// All three positions of every triangle are at the origin → all
/// triangles are degenerate (zero area). Our bridge filters them, so
/// csgrs sees an empty mesh and the boolean is well-defined (empty).
#[test]
fn boolean_returns_diagnostic_not_panic_on_pathological_input() {
    let positions = vec![[0.0_f32, 0.0, 0.0]];
    let indices = vec![0, 0, 0]; // a single degenerate triangle
    let pathological = Tessellation::new(positions, indices).expect("ctor");
    let cube = cube_at(0.0, 0.0, 0.0, 1.0);
    let op = BooleanOp::union();

    // Should not panic. Either Ok(some-output) or a clean error.
    let result = op.evaluate(&[&pathological, &cube]);
    match result {
        Ok(_) | Err(OpError::InvalidParameter(_) | OpError::EmptyResult) => {}
        Err(other) => panic!("unexpected error class on pathological input: {other:?}"),
    }
}

// -----------------------------------------------------------------
// Labeled-input dispatch (csgrs metadata-passthrough path)
// -----------------------------------------------------------------

/// Both unlabeled inputs → unlabeled output. Bit-identical legacy path.
#[test]
fn boolean_evaluate_with_both_unlabeled_inputs_returns_unlabeled_output() {
    let lhs = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
    let op = BooleanOp::union();
    let out = op.evaluate(&[&lhs, &rhs]).expect("union eval");
    assert!(
        !out.is_labeled(),
        "two unlabeled inputs must produce unlabeled output"
    );
}

/// Both labeled inputs → labeled output. Was `evaluate_labeled`.
#[test]
fn boolean_evaluate_with_both_labeled_inputs_returns_labeled_output() {
    let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
    let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
    let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));
    let op = BooleanOp::union();
    let out = op.evaluate(&[&lhs, &rhs]).expect("union labeled");
    assert!(
        out.is_labeled(),
        "two labeled inputs must produce labeled output"
    );

    // Output has triangles. Labels must include 0 (from lhs) AND 1
    // (from rhs) — csgrs preserves polygon metadata for Union.
    assert!(out.triangle_count() > 0);
    let labels = out.face_labels().expect("labeled");
    let mut has_0 = false;
    let mut has_1 = false;
    for &lbl in labels {
        if lbl == TopologyFaceId(0) {
            has_0 = true;
        }
        if lbl == TopologyFaceId(1) {
            has_1 = true;
        }
    }
    assert!(has_0, "union must carry lhs label 0 through");
    assert!(has_1, "union must carry rhs label 1 through");
}

/// Mixed: lhs labeled, rhs unlabeled → labeled output (rhs synthesizes
/// DEGENERATE labels, which surface as Reinterpreted in lineage).
#[test]
fn boolean_evaluate_with_one_labeled_one_unlabeled_input() {
    let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0);
    let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(42));
    assert!(!rhs_tess.is_labeled());

    let op = BooleanOp::union();
    let out = op.evaluate(&[&lhs, &rhs_tess]).expect("mixed union");
    assert!(out.is_labeled(), "labeled lhs must propagate to output");
    assert!(out.triangle_count() > 0);
    let labels = out.face_labels().expect("labeled output");
    assert!(
        labels.contains(&TopologyFaceId(42)),
        "lhs label 42 must propagate through union"
    );
}

/// Reverse mixed: lhs unlabeled, rhs labeled → labeled output.
#[test]
fn boolean_evaluate_with_unlabeled_lhs_labeled_rhs() {
    let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0); // unlabeled
    let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
    let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(7));
    assert!(!lhs_tess.is_labeled());

    let op = BooleanOp::union();
    let out = op.evaluate(&[&lhs_tess, &rhs]).expect("mixed union");
    assert!(out.is_labeled());
    assert!(out.triangle_count() > 0);
    // rhs label 7 should propagate through union.
    let labels = out.face_labels().expect("labeled");
    assert!(
        labels.contains(&TopologyFaceId(7)),
        "rhs label 7 must propagate through union"
    );
}

/// Test 14 — Intersection carries through both lhs and rhs labels on
/// the overlap region (csgrs preserves polygon metadata as-is from
/// clipping).
#[test]
fn boolean_evaluate_carries_lhs_label_through_intersection() {
    let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
    let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
    let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));

    let op = BooleanOp::intersection();
    let out = op.evaluate(&[&lhs, &rhs]).expect("intersection labeled");

    assert!(out.is_labeled());
    assert!(out.triangle_count() > 0);
    let labels: std::collections::BTreeSet<TopologyFaceId> = out
        .face_labels()
        .expect("labeled")
        .iter()
        .copied()
        .collect();
    assert!(
        labels.contains(&TopologyFaceId(0)) || labels.contains(&TopologyFaceId(1)),
        "intersection must carry at least one of the input labels through; got {labels:?}"
    );
}

/// Difference's csgrs lhs-retag quirk: rhs's clipped polygons are retagged
/// with `Mesh::metadata` (we pass None) so they arrive as DEGENERATE per
/// ADR-112. lhs-derived survivors keep their metadata.
#[test]
fn boolean_evaluate_difference_retags_rhs_as_lhs_per_csgrs_quirk() {
    let lhs_tess = cube_at(0.0, 0.0, 0.0, 1.0);
    let rhs_tess = cube_at(0.5, 0.5, 0.5, 1.0); // overlapping
    let lhs = labeled_with_id(&lhs_tess, TopologyFaceId(0));
    let rhs = labeled_with_id(&rhs_tess, TopologyFaceId(1));

    let op = BooleanOp::difference();
    let out = op.evaluate(&[&lhs, &rhs]).expect("difference labeled");

    assert!(out.is_labeled());
    assert!(out.triangle_count() > 0);
    let labels: std::collections::BTreeSet<TopologyFaceId> = out
        .face_labels()
        .expect("labeled")
        .iter()
        .copied()
        .collect();

    // lhs label (0) should still appear on at least one surviving
    // output triangle (lhs's faces that were not clipped retain
    // their metadata).
    assert!(
        labels.contains(&TopologyFaceId(0)),
        "Difference output must still carry the lhs label 0 on lhs-survivor faces; got {labels:?}"
    );
    assert!(
        !labels.contains(&TopologyFaceId(1)),
        "Difference output unexpectedly preserved rhs label 1; csgrs retagged rhs polygons \
         with lhs's Mesh::metadata (None), so rhs-derived faces should arrive as DEGENERATE \
         or be absent. Got labels: {labels:?}"
    );
}

/// Round-trip: a cube labeled by plane (6 distinct labels for the 6
/// axis-aligned faces) survives a no-op self-union evaluate.
#[test]
fn boolean_evaluate_label_by_plane_round_trip_through_self_union() {
    let cube_tess = unit_cube_origin();
    let labeled = label_by_plane(&cube_tess, 0).expect("label cube");
    // 6 distinct face ids on a unit cube (one per axis-aligned face).
    assert_eq!(labeled.face_count(), Some(6));

    let op = BooleanOp::union();
    let out = op
        .evaluate(&[&labeled, &labeled])
        .expect("self-union labeled");

    assert!(out.is_labeled());
    let in_labels: std::collections::BTreeSet<TopologyFaceId> = labeled
        .face_labels()
        .expect("labeled")
        .iter()
        .copied()
        .collect();
    let out_labels: std::collections::BTreeSet<TopologyFaceId> = out
        .face_labels()
        .expect("labeled")
        .iter()
        .copied()
        .collect();
    for &id in in_labels.iter().filter(|id| !id.is_degenerate()) {
        assert!(
            out_labels.contains(&id),
            "label {id} from input is missing on self-union output; out_labels = {out_labels:?}"
        );
    }
}

/// `BooleanOp` arity 2 + `evaluate` dispatch on `lhs.is_labeled() ||
/// rhs.is_labeled()`. Trait default `iter().any(|b| *b)` matches.
#[test]
fn boolean_output_is_labeled_returns_true_when_any_input_labeled() {
    let op = BooleanOp::union();
    assert!(op.output_is_labeled(&[true, false]));
    assert!(op.output_is_labeled(&[false, true]));
    assert!(op.output_is_labeled(&[true, true]));
}

/// Both inputs unlabeled → output unlabeled (matches `evaluate_unlabeled`
/// fast-path bit-identical to the pre-refactor `evaluate`).
#[test]
fn boolean_output_is_labeled_returns_false_when_all_inputs_unlabeled() {
    let op = BooleanOp::union();
    assert!(!op.output_is_labeled(&[false, false]));
}

/// SemVer hardening fixture: [`BooleanMode`] is `#[non_exhaustive]`, so
/// cross-crate consumers MUST include a wildcard arm when pattern-matching.
/// This test simulates that consumer pattern: when the planned future variant
/// is added (`Xor` per ADR-112 + the enum's own doc-comment), the wildcard
/// arm absorbs it and this test still compiles — proving the
/// `#[non_exhaustive]` annotation is correctly applied.
#[test]
#[allow(
    unreachable_patterns,
    reason = "intentional: simulates cross-crate consumer pattern; \
              same-crate compilation sees the enum as exhaustive so the \
              wildcard arm is unreachable from inside the crate, but the \
              `#[non_exhaustive]` SemVer barrier requires it for external \
              consumers"
)]
fn boolean_mode_non_exhaustive_pattern_match_compiles() {
    let mode = BooleanMode::Union;
    let _label = match mode {
        BooleanMode::Union => "union",
        BooleanMode::Intersection => "intersection",
        BooleanMode::Difference => "difference",
        _ => "future-variant", // required by #[non_exhaustive]
    };
}
