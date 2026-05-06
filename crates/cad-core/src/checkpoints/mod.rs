//! `cad_core::checkpoints` — transactional begin / commit / rollback /
//! `restore_to` over the [`OperatorGraph`].
//!
//! Failure class: snapshot-recoverable
//!
//! [`CadGraph`] is the integration point: it owns both an `OperatorGraph`
//! and a [`CheckpointHistory`], and forces all mutations through a
//! `begin_operation` / `commit` (or `rollback`) bracket. Without an open
//! operation, any attempt to mutate via [`CadGraph::graph_mut`] returns
//! [`CheckpointError::MutationOutsideOperation`].
//!
//! # Wire format
//!
//! Snapshots use `kernel/graph-foundation`'s [`GraphSnapshot`] (immutable,
//! Arc-shared, `BTreeMap`-backed for deterministic iteration). The history
//! itself is a `BTreeMap<CheckpointId, Checkpoint>` so iteration order is
//! deterministic — same reason for the `BTreeMap` choice as graph-foundation.

use std::collections::BTreeMap;

use rge_kernel_graph_foundation::{GraphSnapshot, NodeId, SnapshotError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::OperatorGraph;
use crate::operators::{EdgeKind, OperatorNode};

mod participate;

// ---------------------------------------------------------------------------
// CheckpointId
// ---------------------------------------------------------------------------

/// Monotonically-incremented identifier for a committed checkpoint.
///
/// `CheckpointId(0)` is the implicit root of every fresh `CadGraph` — empty
/// graph, no parent. Subsequent commits return `1`, `2`, …
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CheckpointId(pub u64);

impl std::fmt::Display for CheckpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ckpt:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Checkpoint
// ---------------------------------------------------------------------------

/// A committed checkpoint — captures the full graph snapshot, the root at
/// commit time, and a back-pointer to its parent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// This checkpoint's identifier.
    pub id: CheckpointId,
    /// Immutable snapshot of the graph at commit time.
    pub snapshot: GraphSnapshot<OperatorNode, EdgeKind>,
    /// The graph's root at commit time, if any.
    #[allow(clippy::struct_field_names)]
    pub root_at_checkpoint: Option<NodeId>,
    /// Parent checkpoint in the linear history. `None` only on the root
    /// checkpoint (id 0).
    pub parent: Option<CheckpointId>,
    /// Optional human label for the commit.
    pub label: String,
}

// ---------------------------------------------------------------------------
// InProgress (private)
// ---------------------------------------------------------------------------

/// Internal book-keeping while an operation is open. Captured at
/// `begin_operation` time so `rollback` can restore the exact pre-operation
/// state. Stored separately from the committed history so `head` doesn't
/// move until commit.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct InProgress {
    /// Parent checkpoint at the time `begin_operation` was called.
    parent: CheckpointId,
    /// Snapshot of the graph captured at begin time.
    snapshot_at_begin: GraphSnapshot<OperatorNode, EdgeKind>,
    /// Root the graph had at begin time.
    root_at_begin: Option<NodeId>,
}

// ---------------------------------------------------------------------------
// CheckpointHistory
// ---------------------------------------------------------------------------

/// Linear history of committed checkpoints with one optional in-progress
/// transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointHistory {
    checkpoints: BTreeMap<CheckpointId, Checkpoint>,
    head: CheckpointId,
    next_id: u64,
    in_progress: Option<InProgress>,
}

impl CheckpointHistory {
    /// Construct a fresh history with the implicit root checkpoint
    /// `CheckpointId(0)` (empty graph snapshot).
    #[must_use]
    pub fn new(empty_snapshot: GraphSnapshot<OperatorNode, EdgeKind>) -> Self {
        let mut checkpoints = BTreeMap::new();
        let root = Checkpoint {
            id: CheckpointId(0),
            snapshot: empty_snapshot,
            root_at_checkpoint: None,
            parent: None,
            label: "<root>".to_owned(),
        };
        checkpoints.insert(CheckpointId(0), root);
        Self {
            checkpoints,
            head: CheckpointId(0),
            next_id: 1,
            in_progress: None,
        }
    }

    /// The current HEAD (most recently-committed checkpoint).
    #[must_use]
    pub fn head(&self) -> CheckpointId {
        self.head
    }

    /// Look up a committed checkpoint by id.
    #[must_use]
    pub fn checkpoint(&self, id: CheckpointId) -> Option<&Checkpoint> {
        self.checkpoints.get(&id)
    }

    /// Number of committed checkpoints (including the implicit root).
    #[must_use]
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// `true` when only the root checkpoint is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checkpoints.len() <= 1
    }

    /// Iterate over all committed checkpoints in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&CheckpointId, &Checkpoint)> {
        self.checkpoints.iter()
    }

    /// Whether a transaction is currently open.
    #[must_use]
    pub fn is_in_progress(&self) -> bool {
        self.in_progress.is_some()
    }
}

// ---------------------------------------------------------------------------
// CheckpointError
// ---------------------------------------------------------------------------

/// Errors produced by the checkpoint API.
#[derive(Debug, Error)]
pub enum CheckpointError {
    /// A second `begin_operation` call without an intervening
    /// `commit`/`rollback`.
    #[error("an operation is already in progress")]
    AlreadyInProgress,
    /// `commit` or `rollback` called outside of any open operation.
    #[error("no operation in progress")]
    NotInProgress,
    /// Tried to restore to a checkpoint id that does not exist.
    #[error("checkpoint {0} not found")]
    CheckpointNotFound(CheckpointId),
    /// Mutation attempted while no operation is open.
    #[error("mutation outside operation: call begin_operation() first")]
    MutationOutsideOperation,
    /// `restore_to` while a transaction is open.
    #[error("an operation is in progress; commit or rollback first")]
    InProgressMustBeResolved,
    /// Snapshot serialization error from `kernel/graph-foundation`.
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
}

// ---------------------------------------------------------------------------
// CadGraph
// ---------------------------------------------------------------------------

/// Transactional wrapper combining an [`OperatorGraph`] with its
/// [`CheckpointHistory`].
///
/// All mutations must occur inside a `begin_operation` / `commit` (or
/// `rollback`) bracket. Reads are always allowed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CadGraph {
    graph: OperatorGraph,
    history: CheckpointHistory,
}

impl Default for CadGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CadGraph {
    /// Construct a fresh `CadGraph` whose history contains only the implicit
    /// root checkpoint (empty graph).
    #[must_use]
    pub fn new() -> Self {
        let graph = OperatorGraph::new();
        let snapshot = GraphSnapshot::from_graph(graph.inner());
        let history = CheckpointHistory::new(snapshot);
        Self { graph, history }
    }

    /// Read-only view of the underlying operator graph.
    #[must_use]
    pub fn graph(&self) -> &OperatorGraph {
        &self.graph
    }

    /// Mutable view of the underlying operator graph — only valid inside an
    /// open operation.
    ///
    /// # Errors
    ///
    /// [`CheckpointError::MutationOutsideOperation`] if no operation is open.
    pub fn graph_mut(&mut self) -> Result<&mut OperatorGraph, CheckpointError> {
        if self.history.in_progress.is_none() {
            return Err(CheckpointError::MutationOutsideOperation);
        }
        Ok(&mut self.graph)
    }

    /// HEAD of the checkpoint history.
    #[must_use]
    pub fn head(&self) -> CheckpointId {
        self.history.head
    }

    /// Read-only access to the checkpoint history.
    #[must_use]
    pub fn history(&self) -> &CheckpointHistory {
        &self.history
    }

    /// Begin a new operation. Captures a snapshot of the current graph so
    /// `rollback` can restore exactly.
    ///
    /// # Errors
    ///
    /// [`CheckpointError::AlreadyInProgress`] if a transaction is already
    /// open.
    pub fn begin_operation(&mut self) -> Result<(), CheckpointError> {
        if self.history.in_progress.is_some() {
            return Err(CheckpointError::AlreadyInProgress);
        }
        let snapshot = GraphSnapshot::from_graph(self.graph.inner());
        self.history.in_progress = Some(InProgress {
            parent: self.history.head,
            snapshot_at_begin: snapshot,
            root_at_begin: self.graph.root(),
        });
        Ok(())
    }

    /// Commit the open operation. Captures a fresh snapshot of the current
    /// graph as the new HEAD checkpoint and returns its [`CheckpointId`].
    ///
    /// # Errors
    ///
    /// [`CheckpointError::NotInProgress`] if no operation is open.
    pub fn commit(&mut self, label: impl Into<String>) -> Result<CheckpointId, CheckpointError> {
        let in_progress = self
            .history
            .in_progress
            .take()
            .ok_or(CheckpointError::NotInProgress)?;

        let new_id = CheckpointId(self.history.next_id);
        self.history.next_id = self.history.next_id.saturating_add(1);

        let snapshot = GraphSnapshot::from_graph(self.graph.inner());
        let ckpt = Checkpoint {
            id: new_id,
            snapshot,
            root_at_checkpoint: self.graph.root(),
            parent: Some(in_progress.parent),
            label: label.into(),
        };
        self.history.checkpoints.insert(new_id, ckpt);
        self.history.head = new_id;
        Ok(new_id)
    }

    /// Roll back the open operation, restoring the graph to its state at
    /// `begin_operation` time.
    ///
    /// # Errors
    ///
    /// [`CheckpointError::NotInProgress`] if no operation is open.
    ///
    /// # Panics
    ///
    /// Never panics in practice: the snapshot was captured by `begin_operation`
    /// from a valid graph, so the restored root (if any) is guaranteed to
    /// reference a node still present.
    pub fn rollback(&mut self) -> Result<(), CheckpointError> {
        let in_progress = self
            .history
            .in_progress
            .take()
            .ok_or(CheckpointError::NotInProgress)?;
        self.graph
            .replace_inner(in_progress.snapshot_at_begin.to_graph());
        // Restore root to the value captured at begin time.
        if let Some(root) = in_progress.root_at_begin {
            // set_root validates the node exists; restored graph contains it
            // by construction.
            self.graph.set_root(root).expect("restored root must exist");
        }
        Ok(())
    }

    /// Restore the graph to a historical checkpoint. HEAD updates to that
    /// checkpoint; any in-progress transaction must be resolved first
    /// (`commit` or `rollback`) — surface as
    /// [`CheckpointError::InProgressMustBeResolved`].
    ///
    /// # Errors
    ///
    /// * [`CheckpointError::InProgressMustBeResolved`] if a transaction is
    ///   open.
    /// * [`CheckpointError::CheckpointNotFound`] if `id` is not in the
    ///   history.
    ///
    /// # Panics
    ///
    /// Never panics in practice: every checkpoint's snapshot is internally
    /// consistent (root references a node that lives in the same snapshot).
    pub fn restore_to(&mut self, id: CheckpointId) -> Result<(), CheckpointError> {
        if self.history.in_progress.is_some() {
            return Err(CheckpointError::InProgressMustBeResolved);
        }
        let ckpt = self
            .history
            .checkpoints
            .get(&id)
            .ok_or(CheckpointError::CheckpointNotFound(id))?
            .clone();
        self.graph.replace_inner(ckpt.snapshot.to_graph());
        if let Some(root) = ckpt.root_at_checkpoint {
            self.graph.set_root(root).expect("restored root must exist");
        }
        self.history.head = id;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Test 1 — commit advances HEAD by 1 each time.
    #[test]
    fn commit_advances_head() {
        let mut cad = CadGraph::new();
        assert_eq!(cad.head(), CheckpointId(0));
        cad.begin_operation().expect("begin");
        let c1 = cad.commit("first").expect("commit");
        assert_eq!(c1, CheckpointId(1));
        assert_eq!(cad.head(), CheckpointId(1));
        cad.begin_operation().expect("begin2");
        let c2 = cad.commit("second").expect("commit2");
        assert_eq!(c2, CheckpointId(2));
        assert_eq!(cad.head(), CheckpointId(2));
    }

    /// Test 2 — rollback restores graph to begin-time state.
    #[test]
    fn rollback_restores_graph() {
        let mut cad = CadGraph::new();
        assert_eq!(cad.graph().node_count(), 0);
        cad.begin_operation().expect("begin");
        cad.graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(1.0))
            .expect("add");
        assert_eq!(cad.graph().node_count(), 1);
        cad.rollback().expect("rollback");
        assert_eq!(
            cad.graph().node_count(),
            0,
            "rollback must clear the cuboid"
        );
    }

    /// Test 3 — commit persists changes across the bracket.
    #[test]
    fn commit_persists_changes() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        cad.graph_mut()
            .expect("mut")
            .add_operator(cuboid_node(1.0))
            .expect("add");
        cad.commit("with cuboid").expect("commit");
        assert_eq!(cad.graph().node_count(), 1, "commit persists the cuboid");
    }

    /// Test 4 — `restore_to` earlier checkpoint reverts graph state.
    #[test]
    fn restore_to_earlier_checkpoint() {
        let mut cad = CadGraph::new();

        // Commit C1 with one cuboid.
        cad.begin_operation().expect("begin1");
        cad.graph_mut()
            .expect("mut1")
            .add_operator(cuboid_node(1.0))
            .expect("add cuboid");
        let c1 = cad.commit("C1: cuboid").expect("commit1");
        assert_eq!(cad.graph().node_count(), 1);

        // Commit C2 with cuboid + transform (chain).
        cad.begin_operation().expect("begin2");
        let cu = cad
            .graph_mut()
            .expect("mut2")
            .add_operator(cuboid_node(2.0))
            .expect("add cuboid2");
        let tx = cad
            .graph_mut()
            .expect("mut2b")
            .add_operator(translate_node(3.0))
            .expect("add tx");
        cad.graph_mut()
            .expect("mut2c")
            .connect(cu, tx, 0)
            .expect("connect");
        let _c2 = cad.commit("C2: cuboid+transform").expect("commit2");
        // C2 has the C1 cuboid + the new cuboid + transform = 3 nodes.
        assert_eq!(cad.graph().node_count(), 3);

        // Restore to C1; should be back to one cuboid.
        cad.restore_to(c1).expect("restore");
        assert_eq!(cad.head(), c1);
        assert_eq!(cad.graph().node_count(), 1);
    }

    /// Test 5 — `restore_to` unknown checkpoint id errors.
    #[test]
    fn restore_to_unknown_id_errors() {
        let mut cad = CadGraph::new();
        let err = cad.restore_to(CheckpointId(999)).unwrap_err();
        assert!(matches!(err, CheckpointError::CheckpointNotFound(_)));
    }

    /// Test 6 — `graph_mut()` outside an operation errors.
    #[test]
    fn mutation_outside_operation_errors() {
        let mut cad = CadGraph::new();
        let err = cad.graph_mut().unwrap_err();
        assert!(matches!(err, CheckpointError::MutationOutsideOperation));
    }

    /// Test 7 — commit without begin errors.
    #[test]
    fn commit_without_begin_errors() {
        let mut cad = CadGraph::new();
        let err = cad.commit("no-begin").unwrap_err();
        assert!(matches!(err, CheckpointError::NotInProgress));
    }

    /// Test 8 — rollback without begin errors.
    #[test]
    fn rollback_without_begin_errors() {
        let mut cad = CadGraph::new();
        let err = cad.rollback().unwrap_err();
        assert!(matches!(err, CheckpointError::NotInProgress));
    }
}
