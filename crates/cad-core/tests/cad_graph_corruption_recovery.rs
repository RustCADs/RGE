//! Audit-2 B closure: exercise the `CadGraph::SnapshotParticipate::restore`
//! `RestoreFailed` path under three distinct corruption strategies.
//!
//! The capture/restore wire format is RON (text). RON has no magic header by
//! design, so the equivalent of "wrong magic" is "arbitrary non-RON bytes" —
//! a UTF-8 string that does not parse as RON. The three tests cover:
//!
//! 1. **`cad_graph_restore_rejects_truncated_payload`** — capture a real
//!    payload, truncate to the first 10 bytes, and assert restore yields
//!    `ParticipateError::RestoreFailed { id: cad-core.cad-graph, message:
//!    ... }`.
//! 2. **`cad_graph_restore_rejects_arbitrary_bit_flips`** — capture, flip
//!    a single byte in the middle of the payload, attempt restore. Most
//!    flips invalidate RON syntax or RON's structural-tag invariants;
//!    asserts at least one flip lands in the `RestoreFailed` path
//!    (probabilistic — the test loops over a small set of well-chosen
//!    offsets that target the structural keywords RON depends on).
//! 3. **`cad_graph_restore_rejects_arbitrary_bytes`** — replace the entire
//!    payload with non-RON bytes (`b"this is not RON"`); asserts
//!    `RestoreFailed`. Documents that RON has no magic header — this test
//!    exercises the parse-failure path generically.
//!
//! The audit-2 finding flags this as a defense-in-depth against silent
//! restore corruption: a malformed PIE payload must not silently restore a
//! garbage `CadGraph` — it must surface a `ParticipateError::RestoreFailed`
//! at the boundary so the orchestrator can distinguish capture-time-coherent
//! payloads from broken ones.

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, TransformOp};
use rge_kernel_ecs::participate::{ParticipateError, SnapshotParticipate};

/// Build a non-trivial `CadGraph` with a Cuboid → Transform chain committed,
/// so the captured payload is several hundred bytes of structured RON.
fn build_committed_cad_graph() -> CadGraph {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cu = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.5,
            height: 2.5,
            depth: 3.5,
        }))
        .expect("add cuboid");
    let tx = cad
        .graph_mut()
        .expect("mut2")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [7.0, 0.0, 0.0],
            ..TransformOp::default()
        }))
        .expect("add transform");
    cad.graph_mut()
        .expect("mut3")
        .connect(cu, tx, 0)
        .expect("connect");
    cad.graph_mut().expect("mut4").set_root(tx).expect("root");
    cad.commit("for corruption-recovery").expect("commit");
    cad
}

/// Audit-2 B finding: a truncated payload (e.g. PIE envelope was clipped
/// during transit) must yield `RestoreFailed`, not silently restore a
/// partially-valid `CadGraph`.
#[test]
fn cad_graph_restore_rejects_truncated_payload() {
    let cad = build_committed_cad_graph();
    let bytes = cad.capture().expect("capture");
    assert!(
        bytes.len() > 64,
        "capture should produce a substantial payload; got {} bytes",
        bytes.len()
    );

    // Truncate to the first 10 bytes — far too short to contain a complete
    // RON document.
    let truncated = bytes[..10].to_vec();

    let mut fresh = CadGraph::new();
    let err = fresh
        .restore(&truncated)
        .expect_err("truncated payload must fail to restore");

    assert!(
        matches!(&err, ParticipateError::RestoreFailed { id, .. } if id.as_str() == "cad-core.cad-graph"),
        "expected RestoreFailed (id=cad-core.cad-graph), got {err:?}"
    );

    // The fresh CadGraph must remain untouched (still a brand-new empty
    // graph) — a failed restore must NOT leave the participant in a
    // half-mutated state.
    assert_eq!(fresh.graph().node_count(), 0);
    assert_eq!(fresh.history().len(), 1);
}

/// Audit-2 B finding: a single bit-flip somewhere deep in the RON payload
/// must yield `RestoreFailed`, not silently restore a garbage `CadGraph`.
///
/// RON is structural — most byte flips invalidate either UTF-8 (yielding the
/// `not valid UTF-8` arm of the participant impl) or the RON parse (yielding
/// the `ron deserialize` arm). The test loops over several offset/value pairs
/// targeting the structural keywords (`CadGraph`, `kind:`, parens) and
/// asserts that at least one combination triggers `RestoreFailed`.
///
/// **Strategy comment** for future regressions: RON's parser is forgiving
/// inside string literals — flipping a byte in the middle of a `"label"`
/// field may parse fine. The fixture uses a non-trivial `CadGraph` with
/// `connect` so the payload contains structural punctuation (parens,
/// commas, identifiers) where flips are more likely to land in
/// "must-not-be-corrupted" territory.
#[test]
fn cad_graph_restore_rejects_arbitrary_bit_flips() {
    let cad = build_committed_cad_graph();
    let bytes = cad.capture().expect("capture");

    // We try several flip locations + flip values. We assert AT LEAST ONE
    // combination produces a RestoreFailed error. Comprehensive coverage of
    // every byte position is unnecessary — we want to confirm the path is
    // reachable + that it carries the correct ParticipantId, not to prove
    // RON's parser is bulletproof.
    let offsets: &[usize] = &[
        // First byte — typically structural in RON.
        0,
        // Middle of the payload — likely deep inside a Transform / Cuboid block.
        bytes.len() / 4,
        bytes.len() / 2,
        (3 * bytes.len()) / 4,
        // Last byte — typically a closing `)` or `]`.
        bytes.len().saturating_sub(1),
    ];
    let mut at_least_one_flip_caught = false;

    for &off in offsets {
        // Flip every 4th high-impact byte value. We skip the original byte's
        // value since flipping by zero is a no-op; XOR with 0xFF flips all
        // bits, XOR with 0x01 / 0x10 / 0x80 flips one bit per pattern.
        for &mask in &[0xFF_u8, 0x80, 0x10, 0x01] {
            let mut corrupted = bytes.clone();
            corrupted[off] ^= mask;

            let mut fresh = CadGraph::new();
            let result = fresh.restore(&corrupted);
            match result {
                Err(ParticipateError::RestoreFailed { id, .. }) => {
                    assert_eq!(
                        id.as_str(),
                        "cad-core.cad-graph",
                        "RestoreFailed must carry cad-core.cad-graph participant id"
                    );
                    // Verify untouched state: a failed restore must not
                    // leave the participant half-mutated.
                    assert_eq!(fresh.graph().node_count(), 0);
                    assert_eq!(fresh.history().len(), 1);
                    at_least_one_flip_caught = true;
                }
                Err(other) => {
                    panic!(
                        "expected RestoreFailed or Ok on bit-flip at {off:#x} ^ {mask:#x}; \
                         got {other:?}"
                    );
                }
                Ok(()) => {
                    // Possible — RON is forgiving inside string literals or
                    // when the flip happens to produce another valid RON
                    // document. We DO NOT assert success on every flip;
                    // only that AT LEAST ONE corrupting flip lands in the
                    // RestoreFailed path.
                }
            }
        }
    }

    assert!(
        at_least_one_flip_caught,
        "no bit-flip out of {} candidates produced a RestoreFailed; \
         either RON's parser became too permissive or the test's offset \
         coverage is too narrow",
        offsets.len() * 4
    );
}

/// Audit-2 B finding: completely arbitrary (non-RON) bytes must yield
/// `RestoreFailed`. RON has no magic-header check; this test exercises the
/// parse-failure path generically (rather than the wrong-magic path which
/// the format does not have).
///
/// **Documentation**: the equivalent test for a magic-bearing format would
/// flip the magic-byte tag; for RON we feed obviously non-RON content
/// (`b"this is not RON"`). The result is the same: `restore` must reject.
#[test]
fn cad_graph_restore_rejects_arbitrary_bytes() {
    let mut fresh = CadGraph::new();
    let err = fresh
        .restore(b"this is not RON")
        .expect_err("non-RON bytes must fail to restore");

    assert!(
        matches!(&err, ParticipateError::RestoreFailed { id, .. } if id.as_str() == "cad-core.cad-graph"),
        "expected RestoreFailed (id=cad-core.cad-graph), got {err:?}"
    );

    // Repeat with a clearly-binary-non-UTF8 payload: a stretch of 0xFF bytes
    // exercises the "payload not valid UTF-8" arm of the participant impl
    // (a different RestoreFailed branch — both still return RestoreFailed
    // with the same participant id).
    let mut fresh2 = CadGraph::new();
    let binary_garbage: Vec<u8> = vec![0xFF; 64];
    let err2 = fresh2
        .restore(&binary_garbage)
        .expect_err("non-UTF8 bytes must fail to restore");
    assert!(
        matches!(&err2, ParticipateError::RestoreFailed { id, .. } if id.as_str() == "cad-core.cad-graph"),
        "expected RestoreFailed (UTF-8 path), got {err2:?}"
    );

    // And empty bytes — degenerate case worth covering.
    let mut fresh3 = CadGraph::new();
    let err3 = fresh3
        .restore(b"")
        .expect_err("empty payload must fail to restore");
    assert!(
        matches!(&err3, ParticipateError::RestoreFailed { id, .. } if id.as_str() == "cad-core.cad-graph"),
        "expected RestoreFailed (empty payload), got {err3:?}"
    );
}
