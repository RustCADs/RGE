//! `SnapshotParticipate` implementation for [`CadGraph`].
//!
//! Failure class: snapshot-recoverable
//!
//! Closes PLAN §13.2 gate "all stateful Tier-2 has `SnapshotParticipate`" for
//! the cad-graph layer. Without this, PIE snapshots captured the
//! cad-projection participant (and any `BRepHandle.cad_node` references in
//! the world bytes) but NOT the cad-graph itself — a user mutating the
//! cad-graph between capture and restore would create orphan
//! `BRepHandle.cad_node` references after restore (silent inconsistency
//! window).
//!
//! # Wire format
//!
//! Capture/restore use **RON** (the same self-describing text format used by
//! `kernel/graph-foundation`'s [`crate::checkpoints::Checkpoint::snapshot`]
//! ([`rge_kernel_graph_foundation::GraphSnapshot::to_ron`])). Postcard would
//! be more compact and matches `cad-projection`'s `EntityCadMap` payload
//! style, but `OperatorNode` derives `#[serde(tag = "kind")]` for
//! forward-compatibility — and postcard explicitly does not support
//! internally-tagged enum deserialization (it's a non-self-describing
//! format). RON is self-describing and round-trips the tagged enum cleanly.
//!
//! # Convention
//!
//! Callers SHOULD register `CadGraph` and `CadProjection` together in the
//! same `PieSnapshot::capture` / `restore` call. After restoring, callers
//! SHOULD invoke [`crate::CadProjection::validate_handles`] (in
//! `rge-cad-projection`) with the restored cad-graph to detect any orphan
//! handles — those indicate a divergent-state PIE payload (graph and
//! projection captured at different times).

use rge_kernel_ecs::participate::{ParticipantId, ParticipateError, SnapshotParticipate};

use crate::checkpoints::CadGraph;

/// Stable participant id for [`CadGraph`] in PIE snapshots.
///
/// The matching `cad-projection.brep-handles` participant SHOULD be restored
/// in the same `PieSnapshot::restore` call so post-restore the cad-graph and
/// the projection's `BRepHandle.cad_node` references stay coherent.
pub(crate) const CAD_GRAPH_PARTICIPANT_ID: &str = "cad-core.cad-graph";

impl SnapshotParticipate for CadGraph {
    fn participant_id(&self) -> ParticipantId {
        ParticipantId::new(CAD_GRAPH_PARTICIPANT_ID)
    }

    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        let s = ron::to_string(self).map_err(|e| ParticipateError::CaptureFailed {
            id: self.participant_id(),
            message: format!("ron serialize CadGraph: {e}"),
        })?;
        Ok(s.into_bytes())
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let s = std::str::from_utf8(bytes).map_err(|e| ParticipateError::RestoreFailed {
            id: self.participant_id(),
            message: format!("CadGraph payload not valid UTF-8: {e}"),
        })?;
        let restored: CadGraph = ron::from_str(s).map_err(|e| ParticipateError::RestoreFailed {
            id: self.participant_id(),
            message: format!("ron deserialize CadGraph: {e}"),
        })?;
        *self = restored;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — Pairing-4 closure (silent inconsistency window)
//
// The substrate goal: `CadGraph` round-trips losslessly through the
// `SnapshotParticipate` trait so PIE snapshots include cad-graph state
// alongside the cad-projection participant payload — closing the
// silent-inconsistency window where `BRepHandle.cad_node` references could
// orphan after restore.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_ecs::participate::SnapshotParticipate;
    use rge_kernel_ecs::{ParticipantId, PieSnapshot, World};

    use super::CAD_GRAPH_PARTICIPANT_ID;
    use crate::checkpoints::{CadGraph, CheckpointId};
    use crate::operators::{CuboidOp, OperatorNode, TransformOp};

    fn cuboid_node(w: f32) -> OperatorNode {
        OperatorNode::Cuboid(CuboidOp {
            width: w,
            height: 1.0,
            depth: 1.0,
        })
    }

    fn translate_node(dx: f32) -> OperatorNode {
        OperatorNode::Transform(TransformOp {
            translation: [dx, 0.0, 0.0],
            ..TransformOp::default()
        })
    }

    /// Empty `CadGraph::new()` round-trips: capture, restore into a fresh
    /// `CadGraph`, head still `CheckpointId(0)` with no nodes.
    #[test]
    fn cad_graph_capture_round_trips_empty_state() {
        let cad = CadGraph::new();
        let bytes = cad.capture().expect("capture");
        let mut fresh = CadGraph::new();
        // Mutate fresh first to prove `restore` overwrites whatever was there.
        fresh.begin_operation().expect("begin");
        fresh
            .graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(7.0))
            .expect("add");
        fresh.commit("about to be overwritten").expect("commit");
        assert_eq!(fresh.head(), CheckpointId(1));

        fresh.restore(&bytes).expect("restore");
        assert_eq!(
            fresh.head(),
            CheckpointId(0),
            "restored empty cad-graph head reverts to root"
        );
        assert_eq!(
            fresh.graph().node_count(),
            0,
            "restored empty cad-graph has no nodes"
        );
        assert_eq!(
            fresh.history().len(),
            1,
            "only the implicit root checkpoint remains"
        );
    }

    /// Committed cuboid round-trips: root + node count + checkpoint history
    /// all preserved across capture/restore.
    #[test]
    fn cad_graph_capture_round_trips_committed_cuboid() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cu = cad
            .graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(2.5))
            .expect("add");
        cad.graph_mut().expect("mut2").set_root(cu).expect("root");
        let c1 = cad.commit("C1: cuboid").expect("commit");
        let head_before = cad.head();
        let nodes_before = cad.graph().node_count();
        let history_len_before = cad.history().len();
        let root_before = cad.graph().root();

        let bytes = cad.capture().expect("capture");
        let mut fresh = CadGraph::new();
        fresh.restore(&bytes).expect("restore");

        assert_eq!(fresh.head(), head_before, "head preserved");
        assert_eq!(fresh.head(), c1, "head matches the committed checkpoint id");
        assert_eq!(
            fresh.graph().node_count(),
            nodes_before,
            "node count preserved"
        );
        assert_eq!(
            fresh.history().len(),
            history_len_before,
            "history length preserved (root + C1)"
        );
        assert_eq!(fresh.graph().root(), root_before, "root preserved");
        assert!(
            fresh.history().checkpoint(c1).is_some(),
            "C1 reachable post-restore"
        );
    }

    /// In-progress operation round-trips: capture mid-transaction and verify
    /// the in-progress flag (and rollback potential) survives.
    #[test]
    fn cad_graph_capture_round_trips_in_progress_operation() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        cad.graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(1.0))
            .expect("add");
        // No commit — we capture mid-operation.
        assert!(cad.history().is_in_progress());
        assert_eq!(cad.graph().node_count(), 1);

        let bytes = cad.capture().expect("capture in-progress");
        let mut fresh = CadGraph::new();
        fresh.restore(&bytes).expect("restore");
        assert!(
            fresh.history().is_in_progress(),
            "in-progress flag preserved across capture/restore"
        );
        assert_eq!(fresh.graph().node_count(), 1, "uncommitted node preserved");

        // Rollback potential is preserved: the snapshot_at_begin is empty,
        // so rollback drops the in-progress cuboid.
        fresh.rollback().expect("rollback");
        assert_eq!(
            fresh.graph().node_count(),
            0,
            "rollback after restore drops the uncommitted cuboid"
        );
        assert!(!fresh.history().is_in_progress());
    }

    /// Full checkpoint history (3 commits with distinct labels) round-trips
    /// losslessly.
    #[test]
    fn cad_graph_capture_preserves_full_checkpoint_history() {
        let mut cad = CadGraph::new();

        cad.begin_operation().expect("begin1");
        cad.graph_mut()
            .expect("mut1")
            .add_operator(cuboid_node(1.0))
            .expect("add1");
        let c1 = cad.commit("C1: first commit").expect("commit1");

        cad.begin_operation().expect("begin2");
        cad.graph_mut()
            .expect("mut2")
            .add_operator(cuboid_node(2.0))
            .expect("add2");
        let c2 = cad.commit("C2: second commit").expect("commit2");

        cad.begin_operation().expect("begin3");
        cad.graph_mut()
            .expect("mut3")
            .add_operator(cuboid_node(3.0))
            .expect("add3");
        let c3 = cad.commit("C3: third commit").expect("commit3");

        let bytes = cad.capture().expect("capture");
        let mut fresh = CadGraph::new();
        fresh.restore(&bytes).expect("restore");

        assert_eq!(fresh.head(), c3, "head at most-recent commit");
        assert_eq!(
            fresh.history().len(),
            4,
            "root + C1 + C2 + C3 = 4 checkpoints"
        );

        let ck1 = fresh.history().checkpoint(c1).expect("C1 present");
        assert_eq!(ck1.label, "C1: first commit");
        let ck2 = fresh.history().checkpoint(c2).expect("C2 present");
        assert_eq!(ck2.label, "C2: second commit");
        let ck3 = fresh.history().checkpoint(c3).expect("C3 present");
        assert_eq!(ck3.label, "C3: third commit");
    }

    /// Capture is byte-deterministic across constructions: the same logical
    /// state captured twice yields byte-identical output.
    #[test]
    fn cad_graph_capture_is_deterministic_across_constructions() {
        // Construct two separate CadGraph instances containing the same
        // logical state.
        let mut cad_a = CadGraph::new();
        cad_a.begin_operation().expect("begin a");
        let cu_a = cad_a
            .graph_mut()
            .expect("mut a")
            .add_operator(cuboid_node(1.5))
            .expect("add a");
        let tx_a = cad_a
            .graph_mut()
            .expect("mut a2")
            .add_operator(translate_node(2.0))
            .expect("add tx a");
        cad_a
            .graph_mut()
            .expect("mut a3")
            .connect(cu_a, tx_a, 0)
            .expect("connect a");
        cad_a
            .graph_mut()
            .expect("mut a4")
            .set_root(tx_a)
            .expect("root a");
        cad_a.commit("only commit").expect("commit a");

        let mut cad_b = CadGraph::new();
        cad_b.begin_operation().expect("begin b");
        let cu_b = cad_b
            .graph_mut()
            .expect("mut b")
            .add_operator(cuboid_node(1.5))
            .expect("add b");
        let tx_b = cad_b
            .graph_mut()
            .expect("mut b2")
            .add_operator(translate_node(2.0))
            .expect("add tx b");
        cad_b
            .graph_mut()
            .expect("mut b3")
            .connect(cu_b, tx_b, 0)
            .expect("connect b");
        cad_b
            .graph_mut()
            .expect("mut b4")
            .set_root(tx_b)
            .expect("root b");
        cad_b.commit("only commit").expect("commit b");

        let bytes_a = cad_a.capture().expect("capture a");
        let bytes_b = cad_b.capture().expect("capture b");

        // Hash both with BLAKE3 and compare — byte-identical means equal hash.
        let hash_a = blake3::hash(&bytes_a);
        let hash_b = blake3::hash(&bytes_b);
        assert_eq!(
            hash_a, hash_b,
            "two captures of byte-equal state must yield byte-identical bytes"
        );
        assert_eq!(bytes_a, bytes_b, "captured bytes must be byte-identical");
    }

    /// Restore replaces state completely: post-capture mutations are wiped,
    /// and head reverts to the capture-time checkpoint.
    #[test]
    fn cad_graph_restore_replaces_state_completely() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin1");
        cad.graph_mut()
            .expect("mut1")
            .add_operator(cuboid_node(1.0))
            .expect("add1");
        let c1 = cad.commit("C1: original").expect("commit1");
        let head_at_capture = cad.head();
        let nodes_at_capture = cad.graph().node_count();

        let bytes = cad.capture().expect("capture");

        // Now mutate cad post-capture: add nodes + commit a new checkpoint.
        cad.begin_operation().expect("begin2");
        cad.graph_mut()
            .expect("mut2")
            .add_operator(cuboid_node(99.0))
            .expect("add2");
        cad.graph_mut()
            .expect("mut3")
            .add_operator(cuboid_node(100.0))
            .expect("add3");
        let c2 = cad.commit("C2: post-capture").expect("commit2");
        assert_ne!(cad.head(), head_at_capture, "head moved post-capture");
        assert!(cad.graph().node_count() > nodes_at_capture);

        // Restore from the capture; mutations should be gone.
        cad.restore(&bytes).expect("restore");
        assert_eq!(cad.head(), c1, "head reverts to capture-time C1");
        assert_eq!(
            cad.graph().node_count(),
            nodes_at_capture,
            "node count back to capture-time set"
        );
        assert!(
            cad.history().checkpoint(c2).is_none(),
            "post-capture C2 is gone after restore"
        );
    }

    /// Full PIE round-trip via `PieSnapshot::capture`/`restore`. Wraps
    /// `CadGraph` as a participant; restores into a fresh world + fresh
    /// `CadGraph`. The cad-graph state survives the full `PieSnapshot`
    /// envelope, not just direct postcard.
    #[test]
    fn cad_graph_round_trip_via_pie_snapshot() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cu = cad
            .graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(3.5))
            .expect("add");
        cad.graph_mut().expect("mut2").set_root(cu).expect("root");
        let c1 = cad.commit("C1: cuboid for PIE").expect("commit");

        // Capture via PieSnapshot — the world has no entities; cad is the
        // sole participant.
        let world = World::new();
        let snap =
            PieSnapshot::capture(&world, &[&cad as &dyn SnapshotParticipate]).expect("pie capture");
        assert_eq!(snap.participants.len(), 1, "exactly one participant");
        let pid = ParticipantId::new(CAD_GRAPH_PARTICIPANT_ID);
        assert!(
            snap.participants.contains_key(&pid),
            "cad-graph participant present"
        );

        // Restore into a fresh world + fresh cad-graph.
        let mut fresh_world = World::new();
        let mut fresh_cad = CadGraph::new();
        snap.restore(
            &mut fresh_world,
            &mut [(&pid, &mut fresh_cad as &mut dyn SnapshotParticipate)],
        )
        .expect("pie restore");

        // State recovered.
        assert_eq!(fresh_cad.head(), c1);
        assert_eq!(fresh_cad.graph().node_count(), 1);
        assert_eq!(fresh_cad.graph().root(), Some(cu));
    }
}
