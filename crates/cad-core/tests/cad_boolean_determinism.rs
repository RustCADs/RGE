//! Determinism soak: same inputs → byte-identical output, 100 iterations.
//!
//! Closes the ADR-112 §"Followups" determinism gate ("Determinism story for
//! the §13.2 1000-edit gate"). Asserts csgrs's BSP construction is
//! deterministic given identical input ordering. If a csgrs version-bump
//! introduces non-determinism, this test catches it on every CI run.
//!
//! The dispatch brief specifies 100 iterations as the per-CI gate; the §13.6
//! 1000-iter gate is reserved for a future periodic soak with the full
//! `cargo test --release` budget.
//!
//! Failure here is a major finding: if csgrs is non-deterministic for
//! trivially identical inputs, that breaks the workspace's structural-hash
//! cache assumption (same inputs → same output) and the architecture must
//! decide between (a) an internal welding/canonicalization pass, (b) pinning
//! csgrs by exact version + checksum, or (c) replacing csgrs.

use rge_cad_core::{BooleanMode, BooleanOp, CuboidOp, Operator, OperatorNode, Tessellation};

/// Hash a [`Tessellation`]'s `(positions || indices)` byte stream. We hash
/// the f32 little-endian bit patterns directly so equality is exact.
fn tessellation_hash(t: &Tessellation) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for [x, y, z] in &t.positions {
        hasher.update(&x.to_le_bytes());
        hasher.update(&y.to_le_bytes());
        hasher.update(&z.to_le_bytes());
    }
    for &idx in &t.indices {
        hasher.update(&idx.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

#[test]
fn boolean_union_is_deterministic_over_100_iterations() {
    // Build deterministic input tessellations once.
    let lhs = OperatorNode::Cuboid(CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    let rhs_op = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let lhs_tess = lhs.evaluate(&[]).expect("lhs eval");
    // For the rhs we shift the cube vertices manually so the boolean has a
    // genuine 3D overlap to clip (not just two coincident cubes).
    let rhs_tess = {
        let base = OperatorNode::Cuboid(rhs_op)
            .evaluate(&[])
            .expect("rhs eval");
        let mut shifted_positions = Vec::with_capacity(base.positions.len());
        for [x, y, z] in &base.positions {
            shifted_positions.push([*x + 0.5, *y + 0.5, *z + 0.5]);
        }
        Tessellation::new(shifted_positions, base.indices.clone()).expect("shifted")
    };

    let op = BooleanOp::union();

    let first_result = op
        .evaluate(&[&lhs_tess, &rhs_tess])
        .expect("first iteration");
    let first_hash = tessellation_hash(&first_result);

    // 100 iterations total — first is the reference, then 99 more.
    for i in 1..100 {
        let next = op.evaluate(&[&lhs_tess, &rhs_tess]).expect("iter eval");
        let next_hash = tessellation_hash(&next);
        assert_eq!(
            first_hash,
            next_hash,
            "non-determinism detected at iteration {i}: \
             first vertex_count={}, this vertex_count={}",
            first_result.vertex_count(),
            next.vertex_count()
        );
    }
}

/// Same as above but for `Difference` mode — confirms determinism extends
/// across all three boolean modes (csgrs implements xor as A−B ∪ B−A so
/// non-determinism in `difference` would cascade to xor too).
#[test]
fn boolean_difference_is_deterministic_over_100_iterations() {
    let lhs = OperatorNode::Cuboid(CuboidOp::default());
    let rhs_op = CuboidOp::default();
    let lhs_tess = lhs.evaluate(&[]).expect("lhs");
    let rhs_tess = {
        let base = OperatorNode::Cuboid(rhs_op).evaluate(&[]).expect("rhs");
        let mut shifted = Vec::with_capacity(base.positions.len());
        for [x, y, z] in &base.positions {
            shifted.push([*x + 0.3, *y + 0.3, *z + 0.3]);
        }
        Tessellation::new(shifted, base.indices.clone()).expect("shifted")
    };
    let op = BooleanOp::new(BooleanMode::Difference);

    let first = op.evaluate(&[&lhs_tess, &rhs_tess]).expect("first");
    let first_hash = tessellation_hash(&first);
    for i in 1..100 {
        let next = op.evaluate(&[&lhs_tess, &rhs_tess]).expect("iter");
        assert_eq!(
            first_hash,
            tessellation_hash(&next),
            "difference non-determinism at iteration {i}"
        );
    }
}
