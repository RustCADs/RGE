//! Determinism soak: same inputs → byte-identical output, 100 iterations,
//! covering the four primitive / unary operators that lacked a dedicated
//! 100-iter gate (D-Boolean already has one in `cad_boolean_determinism.rs`).
//!
//! # Why a separate file
//!
//! The audit (round 3, 2026-05-09) flagged that Cuboid / Extrude / Revolve /
//! Transform have only single-shot smoke tests. Even though these operators
//! are infallible at the inputs we pick, what the soak proves is that EVERY
//! iteration emits the bit-identical `(positions ++ indices)` byte stream.
//! That property is what the cache-key recursion (`effective_hash`) and every
//! downstream consumer (Tier-2 ECS view, snapshot/restore, exporters) relies
//! on — non-determinism here would silently invalidate cache hits and is
//! exactly the kind of bug 100-iter loops catch but single-shot smokes miss.
//!
//! Float arithmetic for the sin/cos sweep (Revolve) and the
//! quaternion → matrix → vec3 multiply chain (Transform) is the
//! highest-risk surface for hidden non-determinism — historic CSG kernels
//! (and even some glam-pre-1.0 paths) have exhibited iteration-order
//! sensitivity. Pinning a 100-iter byte-identity gate now means a future
//! glam / std-math version bump that re-orders an FMA cannot land silently.
//!
//! Failure here is a real finding: the operator's output cannot be cached
//! by structural hash if its evaluation is not a pure function of its
//! inputs — escalate, do not mask.
//!
//! # Hashing convention
//!
//! Mirrors `cad_boolean_determinism.rs::tessellation_hash`: BLAKE3 over
//! `positions` (each f32 component → 4 LE bytes) followed by `indices`
//! (each u32 → 4 LE bytes). f32 bit patterns are hashed directly so
//! equality is exact (no tolerance smoothing).

use std::f32::consts::PI;

use rge_cad_core::{
    CuboidOp, ExtrudeOp, Operator, OperatorNode, Polygon2D, RevolveOp, Tessellation, TransformOp,
};

// ---------------------------------------------------------------------------
// Shared hash helper — identical contract to the Boolean soak.
// ---------------------------------------------------------------------------

/// BLAKE3 over `(positions || indices)` byte stream. f32 LE bit patterns
/// hashed directly so equality is exact.
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

// ---------------------------------------------------------------------------
// Cuboid — arity-0 generative primitive, 8 verts / 12 tris closed-form.
// ---------------------------------------------------------------------------

/// `CuboidOp::default()` is an 8-vertex unit cube produced via float
/// half-extent multiplication. Single-pass arithmetic with no
/// triangulation choices, so any non-determinism here would be a pure
/// f32-bit-drift bug — reserved exclusively as a regression canary.
#[test]
fn cuboid_op_100_iter_determinism() {
    let op = CuboidOp::default();
    let first = op.evaluate(&[]).expect("first iteration");
    let first_hash = tessellation_hash(&first);

    for i in 1..100 {
        let next = op.evaluate(&[]).expect("iter eval");
        assert_eq!(
            first_hash,
            tessellation_hash(&next),
            "Cuboid non-determinism at iteration {i}: \
             first vertex_count={}, this vertex_count={}",
            first.vertex_count(),
            next.vertex_count()
        );
    }
}

// ---------------------------------------------------------------------------
// Transform — arity-1 affine TRS. Highest-risk operator for hidden
// non-determinism: glam quat→mat4→vec3 chain involves FMA-eligible f32
// multiplies that have historically been re-orderable across glam versions.
// ---------------------------------------------------------------------------

/// 100-iter soak on the full TRS pipeline applied to a fresh Cuboid mesh
/// each iteration. Quaternion components are bit-exact float literals
/// (precomputed for axis = (1,1,0)/√2 at angle = π/3) so the input is
/// bit-stable across iterations — any output drift is then provably a
/// glam internal arithmetic re-ordering, not input recomputation noise.
///
/// Precomputed quaternion components for axis = (1, 1, 0)/√2, angle = π/3:
///
/// ```text
/// half_angle = π/6
/// cos(π/6)   ≈ 0.8660254037844386       (= √3/2)
/// sin(π/6)   = 0.5
/// ax = ay    = 1/√2 ≈ 0.7071067811865476
/// qx = qy    = (1/√2) · 0.5 = √2/4 ≈ 0.3535533905932738
/// qz         = 0.0
/// qw         = √3/2 ≈ 0.8660254037844386
/// ```
///
/// Sanity: `qx² + qy² + qw² = 2·(1/8) + 3/4 = 1.0` (unit quaternion).
#[test]
fn transform_op_100_iter_determinism() {
    // Bit-exact quaternion literals — see doc-comment above for derivation.
    // Using f32 literals: glam's `Quat::from_xyzw` is a no-op constructor,
    // so the bit pattern entering the matrix multiply is fixed by these
    // five bytes-per-component constants below.
    let op = TransformOp {
        translation: [1.0, 2.0, 3.0],
        rotation_quat_xyzw: [
            0.353_553_4_f32, // qx = √2/4 (f32-rounded literal)
            0.353_553_4_f32, // qy = √2/4
            0.0_f32,
            0.866_025_4_f32, // qw = √3/2 (f32-rounded literal)
        ],
        scale: [1.5, 0.5, 2.0],
    };

    // Build the upstream cuboid mesh once and reuse — each iteration must
    // hash identically against the same upstream input.
    let upstream = OperatorNode::Cuboid(CuboidOp::default())
        .evaluate(&[])
        .expect("upstream cuboid");

    let first = op.evaluate(&[&upstream]).expect("first iteration");
    let first_hash = tessellation_hash(&first);

    for i in 1..100 {
        let next = op.evaluate(&[&upstream]).expect("iter eval");
        assert_eq!(
            first_hash,
            tessellation_hash(&next),
            "Transform non-determinism at iteration {i}: \
             first vertex_count={}, this vertex_count={}",
            first.vertex_count(),
            next.vertex_count()
        );
    }
}

// ---------------------------------------------------------------------------
// Extrude — arity-0 sweep of a 2D convex polygon along +Z. Fan-triangulated
// caps + side-wall quad strips. Hard-coded regular pentagon literal so the
// profile bit pattern is fixed across iterations.
// ---------------------------------------------------------------------------

/// 100-iter soak on a regular-pentagon → 1.5-unit prism extrusion. Pentagon
/// literals match the existing `extrude::tests::ccw_pentagon()` fixture so
/// any drift here would also surface in unit tests, but only a
/// 100-iter loop catches per-iteration f32 instability.
#[test]
fn extrude_op_100_iter_determinism() {
    // Regular pentagon centered at origin, unit circumradius.
    // Identical literals to the in-source fixture in extrude.rs.
    let profile = Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("ccw pentagon");

    let op = ExtrudeOp::new(profile, 1.5).expect("extrude op");
    let first = op.evaluate(&[]).expect("first iteration");
    let first_hash = tessellation_hash(&first);

    for i in 1..100 {
        let next = op.evaluate(&[]).expect("iter eval");
        assert_eq!(
            first_hash,
            tessellation_hash(&next),
            "Extrude non-determinism at iteration {i}: \
             first vertex_count={}, this vertex_count={}",
            first.vertex_count(),
            next.vertex_count()
        );
    }
}

// ---------------------------------------------------------------------------
// Revolve — arity-0 sweep around the Y-axis. Two sub-soaks:
//   (a) full 2π revolution (no caps, concave-permissive code path)
//   (b) partial π revolution (fan-triangulated caps, convex-required path)
// Both exercise the sin/cos angular-step loop, which is the operator's
// highest-risk f32 surface.
// ---------------------------------------------------------------------------

/// Square profile on the +X side of the Y-axis. Hard-coded literals so the
/// profile bit pattern is fixed. Strictly convex (required by the partial
/// revolve cap-triangulation path).
fn plus_x_square_profile() -> Polygon2D {
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
        .expect("+X-side unit square")
}

/// 100-iter soak run twice: first against `RevolveOp::new(.., 8)` (full
/// revolution path, no caps), then against `RevolveOp::partial(.., 8, π)`
/// (partial revolution path, fan caps). Each sub-soak independently asserts
/// byte-identical hash across all 100 iterations of its own configuration.
#[test]
fn revolve_op_100_iter_determinism() {
    // -- Sub-soak (a): full 2π revolution -----------------------------------
    {
        let op = RevolveOp::new(plus_x_square_profile(), 8).expect("full revolve");
        let first = op.evaluate(&[]).expect("first iteration (full)");
        let first_hash = tessellation_hash(&first);
        for i in 1..100 {
            let next = op.evaluate(&[]).expect("iter eval (full)");
            assert_eq!(
                first_hash,
                tessellation_hash(&next),
                "Revolve(full 2π) non-determinism at iteration {i}: \
                 first vertex_count={}, this vertex_count={}",
                first.vertex_count(),
                next.vertex_count()
            );
        }
    }

    // -- Sub-soak (b): partial π revolution ---------------------------------
    {
        let op = RevolveOp::partial(plus_x_square_profile(), 8, PI).expect("partial revolve");
        let first = op.evaluate(&[]).expect("first iteration (partial)");
        let first_hash = tessellation_hash(&first);
        for i in 1..100 {
            let next = op.evaluate(&[]).expect("iter eval (partial)");
            assert_eq!(
                first_hash,
                tessellation_hash(&next),
                "Revolve(partial π) non-determinism at iteration {i}: \
                 first vertex_count={}, this vertex_count={}",
                first.vertex_count(),
                next.vertex_count()
            );
        }
    }
}
