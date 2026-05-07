//! Generic graph container: nodes keyed by [`NodeId`], directed edges keyed
//! by [`EdgeId`]. Iteration is deterministic (BTreeMap-backed).
//!
//! Domain-specific traversal algorithms are explicitly out of scope — write
//! your own using the provided [`Graph::nodes`] / [`Graph::edges`] /
//! [`Graph::outgoing`] / [`Graph::incoming`] iterators.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::{EdgeId, NodeId};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by mutating operations on a [`Graph`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    /// A lookup by [`NodeId`] found no entry.
    #[error("node {0} not found")]
    NodeNotFound(NodeId),
    /// A lookup by [`EdgeId`] found no entry.
    #[error("edge {0} not found")]
    EdgeNotFound(EdgeId),
    /// An insert attempted to reuse an already-present [`NodeId`].
    #[error("duplicate node id {0}")]
    DuplicateNode(NodeId),
    /// An insert attempted to reuse an already-present [`EdgeId`].
    #[error("duplicate edge id {0}")]
    DuplicateEdge(EdgeId),
    /// Edge endpoints reference nodes not currently in the graph.
    #[error("edge endpoints not in graph: src={src} dst={dst}")]
    DanglingEndpoint {
        /// Source node id that was missing.
        src: NodeId,
        /// Destination node id that was missing.
        dst: NodeId,
    },
}

// ---------------------------------------------------------------------------
// EdgeRecord
// ---------------------------------------------------------------------------

/// An edge together with its source, destination, and payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeRecord<E> {
    /// Source (origin) node of the directed edge.
    pub src: NodeId,
    /// Destination (target) node of the directed edge.
    pub dst: NodeId,
    /// Domain-specific edge payload.
    pub data: E,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

/// Generic graph: nodes keyed by [`NodeId`], directed edges keyed by
/// [`EdgeId`]. Iteration is deterministic (BTreeMap-backed).
///
/// Domain-specific traversal algorithms are explicitly out of scope — write
/// your own using `nodes()`/`edges()`/`outgoing(...)`/`incoming(...)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph<N, E> {
    nodes: BTreeMap<NodeId, N>,
    edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    /// Forward adjacency: src → set of outgoing `EdgeId`s.
    outgoing: BTreeMap<NodeId, BTreeSet<EdgeId>>,
    /// Reverse adjacency: dst → set of incoming `EdgeId`s.
    incoming: BTreeMap<NodeId, BTreeSet<EdgeId>>,
    /// Cached workspace-wide maximum out-degree (Tier-B per ADR-115
    /// phase-2). O(1) on `insert_edge` (max of cache vs new src
    /// `outgoing.len()`); partial recomputation on `remove_edge` /
    /// `remove_node` only when the cached value was potentially on the
    /// affected node. See [`Graph::max_out_fanout`] for the public
    /// accessor + per-mutation cost analysis.
    max_out_fanout: u32,
    /// Cached workspace-wide maximum in-degree (Tier-B per ADR-115
    /// phase-2). Symmetric companion to `max_out_fanout` for incoming
    /// edges. See [`Graph::max_in_fanout`] for the public accessor +
    /// per-mutation cost analysis.
    max_in_fanout: u32,
}

impl<N: Clone, E: Clone> Graph<N, E> {
    /// Construct an empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            incoming: BTreeMap::new(),
            max_out_fanout: 0,
            max_in_fanout: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Node operations
    // -----------------------------------------------------------------------

    /// Insert a new node.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::DuplicateNode`] when `id` is already present.
    pub fn insert_node(&mut self, id: NodeId, node: N) -> Result<(), GraphError> {
        if self.nodes.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    /// Replace the payload of an existing node. Returns the old value.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::NodeNotFound`] when `id` is not present.
    pub fn replace_node(&mut self, id: NodeId, node: N) -> Result<N, GraphError> {
        let slot = self
            .nodes
            .get_mut(&id)
            .ok_or(GraphError::NodeNotFound(id))?;
        Ok(std::mem::replace(slot, node))
    }

    /// Remove a node and all edges that touch it (incoming or outgoing).
    /// Returns the previous node payload.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::NodeNotFound`] when `id` is not present.
    pub fn remove_node(&mut self, id: NodeId) -> Result<N, GraphError> {
        let node = self.nodes.remove(&id).ok_or(GraphError::NodeNotFound(id))?;

        // Tier-B max-fanout staleness check (ADR-115 phase-2). Capture
        // pre-mutation degrees of the removed node + every neighbour
        // whose degree will drop. If any of those values equalled the
        // cached max, the cache MAY be stale and is recomputed below.
        let removed_out = u32::try_from(
            self.outgoing
                .get(&id)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);
        let removed_in = u32::try_from(
            self.incoming
                .get(&id)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);

        // Collect all edge ids to remove (outgoing + incoming).
        let mut to_remove: Vec<EdgeId> = Vec::new();
        if let Some(outs) = self.outgoing.get(&id) {
            to_remove.extend(outs);
        }
        if let Some(ins) = self.incoming.get(&id) {
            to_remove.extend(ins);
        }

        // Track whether any neighbour's degree was equal to the cached
        // max BEFORE the cascade (any such neighbour will lose at least
        // one edge, so the cached value may become stale).
        let mut may_invalidate_out_cache = removed_out >= self.max_out_fanout;
        let mut may_invalidate_in_cache = removed_in >= self.max_in_fanout;
        for eid in &to_remove {
            if let Some(rec) = self.edges.get(eid) {
                // The cascading edge will decrement rec.dst's incoming
                // count (if rec.src == id) or rec.src's outgoing count
                // (if rec.dst == id). The opposite endpoint of `id` is
                // a neighbour whose degree will drop.
                if rec.src == id {
                    let dst_in = u32::try_from(
                        self.incoming
                            .get(&rec.dst)
                            .map_or(0, std::collections::BTreeSet::len),
                    )
                    .unwrap_or(u32::MAX);
                    if dst_in >= self.max_in_fanout {
                        may_invalidate_in_cache = true;
                    }
                }
                if rec.dst == id {
                    let src_out = u32::try_from(
                        self.outgoing
                            .get(&rec.src)
                            .map_or(0, std::collections::BTreeSet::len),
                    )
                    .unwrap_or(u32::MAX);
                    if src_out >= self.max_out_fanout {
                        may_invalidate_out_cache = true;
                    }
                }
            }
        }

        for eid in to_remove {
            self.remove_edge_unchecked(eid);
        }

        self.outgoing.remove(&id);
        self.incoming.remove(&id);

        // Tier-B partial-recomputation triggers (ADR-115 phase-2). Only
        // pay the O(N) scan cost when the staleness check above flagged
        // the cache; otherwise the cached max is provably still
        // attainable by some untouched node and is left unchanged.
        if may_invalidate_out_cache {
            self.recompute_max_out_fanout();
        }
        if may_invalidate_in_cache {
            self.recompute_max_in_fanout();
        }

        Ok(node)
    }

    /// Look up a node by id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&N> {
        self.nodes.get(&id)
    }

    /// Look up a node mutably by id.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut N> {
        self.nodes.get_mut(&id)
    }

    /// Iterate over all (id, node) pairs in deterministic order.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &N)> {
        self.nodes.iter().map(|(&id, n)| (id, n))
    }

    // -----------------------------------------------------------------------
    // Edge operations
    // -----------------------------------------------------------------------

    /// Insert a directed edge from `src` to `dst`.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::DuplicateEdge`] when `id` is already present,
    /// or [`GraphError::DanglingEndpoint`] when either endpoint is absent.
    pub fn insert_edge(
        &mut self,
        id: EdgeId,
        src: NodeId,
        dst: NodeId,
        edge: E,
    ) -> Result<(), GraphError> {
        if self.edges.contains_key(&id) {
            return Err(GraphError::DuplicateEdge(id));
        }
        let src_ok = self.nodes.contains_key(&src);
        let dst_ok = self.nodes.contains_key(&dst);
        if !src_ok || !dst_ok {
            return Err(GraphError::DanglingEndpoint { src, dst });
        }
        self.edges.insert(
            id,
            EdgeRecord {
                src,
                dst,
                data: edge,
            },
        );
        self.outgoing.entry(src).or_default().insert(id);
        self.incoming.entry(dst).or_default().insert(id);

        // Tier-B max-fanout cache update (O(1) on insert per ADR-115
        // phase-2). The new src out-degree and dst in-degree are the
        // only values that can become the new max — every other node's
        // degree is unchanged. The `as u32` cast cannot wrap because
        // `BTreeSet::len() <= node_count`, and a graph that holds more
        // than `u32::MAX` edges from a single node violates other
        // workspace invariants long before this point.
        let new_src_out = u32::try_from(
            self.outgoing
                .get(&src)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);
        let new_dst_in = u32::try_from(
            self.incoming
                .get(&dst)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);
        if new_src_out > self.max_out_fanout {
            self.max_out_fanout = new_src_out;
        }
        if new_dst_in > self.max_in_fanout {
            self.max_in_fanout = new_dst_in;
        }

        Ok(())
    }

    /// Replace the payload of an existing edge. Returns the old payload.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EdgeNotFound`] when `id` is not present.
    pub fn replace_edge(&mut self, id: EdgeId, edge: E) -> Result<E, GraphError> {
        let rec = self
            .edges
            .get_mut(&id)
            .ok_or(GraphError::EdgeNotFound(id))?;
        Ok(std::mem::replace(&mut rec.data, edge))
    }

    /// Remove an edge. Returns the full [`EdgeRecord`].
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EdgeNotFound`] when `id` is not present.
    ///
    /// # Panics
    ///
    /// Never panics in practice: existence is confirmed before calling the
    /// internal helper that asserts presence.
    pub fn remove_edge(&mut self, id: EdgeId) -> Result<EdgeRecord<E>, GraphError> {
        let rec = self.edges.get(&id).ok_or(GraphError::EdgeNotFound(id))?;

        // Tier-B max-fanout staleness check (ADR-115 phase-2). Capture
        // the affected nodes' pre-mutation degrees; if either equalled
        // the cached max the cache may be stale post-removal. The
        // typical case (cached max held by a different node) leaves the
        // cache untouched and avoids the O(N) scan.
        let src = rec.src;
        let dst = rec.dst;
        let pre_src_out = u32::try_from(
            self.outgoing
                .get(&src)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);
        let pre_dst_in = u32::try_from(
            self.incoming
                .get(&dst)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX);
        let may_invalidate_out_cache = pre_src_out >= self.max_out_fanout;
        let may_invalidate_in_cache = pre_dst_in >= self.max_in_fanout;

        let record = self
            .remove_edge_unchecked(id)
            .expect("just confirmed present");

        if may_invalidate_out_cache {
            self.recompute_max_out_fanout();
        }
        if may_invalidate_in_cache {
            self.recompute_max_in_fanout();
        }

        Ok(record)
    }

    /// Look up an edge by id.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<&EdgeRecord<E>> {
        self.edges.get(&id)
    }

    /// Look up an edge mutably by id.
    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut EdgeRecord<E>> {
        self.edges.get_mut(&id)
    }

    /// Iterate over all (id, record) pairs in deterministic order.
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &EdgeRecord<E>)> {
        self.edges.iter().map(|(&id, e)| (id, e))
    }

    /// Iterate over the [`EdgeId`]s of all outgoing edges from `src`.
    pub fn outgoing(&self, src: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        self.outgoing
            .get(&src)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Iterate over the [`EdgeId`]s of all incoming edges to `dst`.
    pub fn incoming(&self, dst: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        self.incoming
            .get(&dst)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    // -----------------------------------------------------------------------
    // Counts (Tier-A counters per ADR-115 phase-1)
    // -----------------------------------------------------------------------

    /// Returns the number of nodes currently in the graph.
    ///
    /// **Tier-A** (canonical structural counter; ADR-115 phase-2.5 amendment).
    ///
    /// O(1). Tier-A counter per ADR-115 phase-1 (graph-metrics substrate
    /// design, sub-decisions 1+2). Every mutation that adds or removes a
    /// node is transactional through [`Graph::insert_node`] /
    /// [`Graph::remove_node`]; the BTreeMap-backed `nodes` storage's
    /// `.len()` is the canonical count and is itself O(1) per the
    /// `std::collections::BTreeMap::len` contract.
    ///
    /// # Companion metrics
    ///
    /// - [`Graph::edge_count`] — edge-side counterpart (this same Tier).
    /// - `cad-core::OperatorGraph::operator_count` — domain-specific
    ///   thin wrapper exposing this count under the operator-graph
    ///   semantic name (every node in `OperatorGraph` is an operator).
    /// - `constraint_count` — deferred per ADR-115; depends on a future
    ///   constraint-system substrate that does not yet exist.
    /// - `invalidation_count` — deferred per ADR-115; cross-substrate
    ///   concern (cad-projection head-advance + cad-core checkpoint
    ///   commits) that lands in phase-3+ via the event-sourced
    ///   `GraphEvent` stream (ADR-115 sub-decision 4).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges currently in the graph.
    ///
    /// **Tier-A** (canonical structural counter; ADR-115 phase-2.5 amendment).
    ///
    /// O(1). Tier-A counter per ADR-115 phase-1 (graph-metrics substrate
    /// design, sub-decisions 1+2). Every mutation that adds or removes
    /// an edge is transactional through [`Graph::insert_edge`] /
    /// [`Graph::remove_edge`] (and the cascade path inside
    /// [`Graph::remove_node`]); the BTreeMap-backed `edges` storage's
    /// `.len()` is the canonical count and is itself O(1) per the
    /// `std::collections::BTreeMap::len` contract.
    ///
    /// # Companion metrics
    ///
    /// See [`Graph::node_count`] for the node-side counterpart and the
    /// list of deferred companion counters (`operator_count` exposed by
    /// `cad-core::OperatorGraph`; `constraint_count` /
    /// `invalidation_count` deferred per ADR-115).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    // -----------------------------------------------------------------------
    // Fanout (Tier-B incremental structural metrics per ADR-115 phase-2)
    // -----------------------------------------------------------------------
    //
    // Tier-B classification per ADR-115 sub-decision 2: maintained during
    // graph mutations via partial recomputation around the affected
    // region — NOT a full-graph scan on every read.
    //
    // Per-node degree accessors are derived O(1) from the existing
    // `outgoing` / `incoming` BTreeMap<NodeId, BTreeSet<EdgeId>> adjacency
    // caches (no new field required; `BTreeSet::len` is O(1)).
    //
    // Workspace-wide max accessors (`max_out_fanout` / `max_in_fanout`)
    // are cached in `self.max_out_fanout` / `self.max_in_fanout` fields.
    // Insert is O(1): the new src out-degree (resp. dst in-degree) is the
    // only candidate that can become the new max. Remove is O(1) when the
    // affected node was NOT holding the cached max (most cases); else
    // partial recomputation walks the (sparse) adjacency map keys via
    // [`Graph::recompute_max_out_fanout`] / [`Graph::recompute_max_in_fanout`].
    // The recomputation is bounded by the count of nodes that have at
    // least one outgoing (resp. incoming) edge, which is `<= node_count`.
    //
    // Average fanout is O(1) derivable as `edge_count / node_count` and
    // does NOT need a cached field. The same value applies to both
    // out-fanout and in-fanout (every directed edge contributes 1 to
    // exactly one src's outgoing count + 1 to exactly one dst's incoming
    // count); a single [`Graph::average_fanout`] accessor exposes the
    // shared value.
    //
    // Phase-2.5+ deferrals (per ADR-115 §"Followups"): `max_depth` (DFS +
    // cycle handling; not naturally incremental — adding an edge can
    // change the depth of many descendants), `scc_count` (Tarjan/
    // Kosaraju; not naturally incremental at all), `dependency_diameter`
    // (longest shortest path; O(V * (V+E)) full BFS-from-each-node; not
    // incremental). Each deserves its own dispatch with algorithmic
    // design discussion before implementation.

    /// Returns the in-degree (number of incoming edges) of `node`. Returns
    /// `0` when the node is not present in the graph.
    ///
    /// **Tier-B** (mutation-local incremental runtime metric; ADR-115 phase-2.5 amendment).
    ///
    /// O(1). Tier-B per-node fanout accessor per ADR-115 phase-2
    /// (graph-metrics substrate design, sub-decision 2). The value is
    /// derived from the `incoming` adjacency cache via `BTreeSet::len`,
    /// which is O(1) per the `std::collections::BTreeSet::len` contract;
    /// no new state is required because the adjacency cache is already
    /// maintained transactionally on every edge mutation.
    ///
    /// # Companion metrics
    ///
    /// - [`Graph::node_out_degree`] — outgoing-side counterpart.
    /// - [`Graph::max_in_fanout`] — workspace-wide maximum in-degree.
    /// - [`Graph::node_count`] / [`Graph::edge_count`] — Tier-A counters
    ///   (ADR-115 phase-1 companion).
    ///
    /// Phase-2.5+ deferred companions (per ADR-115 §"Followups"):
    /// `max_depth` / `scc_count` / `dependency_diameter` /
    /// `topology_lineage_breadth` — each requires algorithmic design
    /// before implementation; see this module's Tier-B section comment.
    #[must_use]
    pub fn node_in_degree(&self, node: NodeId) -> u32 {
        u32::try_from(
            self.incoming
                .get(&node)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX)
    }

    /// Returns the out-degree (number of outgoing edges) of `node`.
    /// Returns `0` when the node is not present in the graph.
    ///
    /// **Tier-B** (mutation-local incremental runtime metric; ADR-115 phase-2.5 amendment).
    ///
    /// O(1). Tier-B per-node fanout accessor per ADR-115 phase-2
    /// (graph-metrics substrate design, sub-decision 2). The value is
    /// derived from the `outgoing` adjacency cache via `BTreeSet::len`,
    /// which is O(1) per the `std::collections::BTreeSet::len` contract;
    /// no new state is required because the adjacency cache is already
    /// maintained transactionally on every edge mutation.
    ///
    /// # Companion metrics
    ///
    /// See [`Graph::node_in_degree`] for the incoming-side counterpart
    /// and the list of deferred phase-2.5+ companions.
    #[must_use]
    pub fn node_out_degree(&self, node: NodeId) -> u32 {
        u32::try_from(
            self.outgoing
                .get(&node)
                .map_or(0, std::collections::BTreeSet::len),
        )
        .unwrap_or(u32::MAX)
    }

    /// Returns the maximum out-degree across all nodes in the graph.
    /// Returns `0` when the graph has no edges.
    ///
    /// **Tier-B** (mutation-local incremental runtime metric; ADR-115 phase-2.5 amendment).
    ///
    /// Tier-B incremental structural metric per ADR-115 phase-2
    /// (graph-metrics substrate design, sub-decision 2). Cached in
    /// `self.max_out_fanout` and maintained via partial recomputation:
    ///
    /// - [`Graph::insert_edge`] — O(1): the new src out-degree is the
    ///   only candidate that can exceed the cached max; one comparison
    ///   suffices.
    /// - [`Graph::remove_edge`] / [`Graph::remove_node`] — O(1) when the
    ///   affected node was NOT holding the cached max (the typical case;
    ///   the cached value is provably still attainable by an untouched
    ///   node). Otherwise `O(N_with_outgoing)` via
    ///   [`Graph::recompute_max_out_fanout`], where `N_with_outgoing` is
    ///   the count of nodes that have at least one outgoing edge
    ///   (`<= node_count`).
    ///
    /// # Companion metrics
    ///
    /// - [`Graph::max_in_fanout`] — incoming-side counterpart.
    /// - [`Graph::node_out_degree`] — per-node accessor.
    /// - [`Graph::average_fanout`] — Tier-B-derived O(1) average.
    ///
    /// Phase-2.5+ deferred companions (per ADR-115 §"Followups"):
    /// `max_depth` / `scc_count` / `dependency_diameter` — none are
    /// naturally incremental and deserve their own dispatch with
    /// algorithmic design before implementation.
    #[must_use]
    pub fn max_out_fanout(&self) -> u32 {
        self.max_out_fanout
    }

    /// Returns the maximum in-degree across all nodes in the graph.
    /// Returns `0` when the graph has no edges.
    ///
    /// **Tier-B** (mutation-local incremental runtime metric; ADR-115 phase-2.5 amendment).
    ///
    /// Tier-B incremental structural metric per ADR-115 phase-2
    /// (graph-metrics substrate design, sub-decision 2). Symmetric
    /// companion to [`Graph::max_out_fanout`] for incoming edges; the
    /// per-mutation cost analysis is identical (O(1) on insert; O(1)
    /// when the affected node was NOT holding the cached max on remove;
    /// else `O(N_with_incoming)` via
    /// [`Graph::recompute_max_in_fanout`]).
    ///
    /// # Companion metrics
    ///
    /// See [`Graph::max_out_fanout`] for the outgoing-side counterpart
    /// and the list of deferred phase-2.5+ companions.
    #[must_use]
    pub fn max_in_fanout(&self) -> u32 {
        self.max_in_fanout
    }

    /// Returns the average fanout (edges per node) across the graph.
    /// Returns `0.0` for an empty graph (no division by zero).
    ///
    /// **Tier-B** (mutation-local incremental runtime metric; ADR-115 phase-2.5 amendment).
    ///
    /// O(1). Tier-B incremental structural metric per ADR-115 phase-2
    /// (graph-metrics substrate design, sub-decision 2). Derived from
    /// the Tier-A `edge_count` and `node_count` counters at read time —
    /// no cached field is required because the operation is a single
    /// floating-point divide on already-O(1) inputs.
    ///
    /// # Symmetry note
    ///
    /// The same value applies to both out-fanout and in-fanout averages:
    /// every directed edge contributes 1 to exactly one src's outgoing
    /// count and 1 to exactly one dst's incoming count, so the totals
    /// (and therefore the per-node averages) are identical. A single
    /// accessor is exposed instead of separate `average_in_fanout` /
    /// `average_out_fanout` methods.
    ///
    /// # Companion metrics
    ///
    /// - [`Graph::max_out_fanout`] / [`Graph::max_in_fanout`] — Tier-B
    ///   workspace-wide maxima.
    /// - [`Graph::node_in_degree`] / [`Graph::node_out_degree`] — Tier-B
    ///   per-node accessors.
    /// - [`Graph::node_count`] / [`Graph::edge_count`] — Tier-A counters
    ///   (ADR-115 phase-1) used as inputs.
    ///
    /// Phase-2.5+ deferred companions (per ADR-115 §"Followups"):
    /// `max_depth` / `scc_count` / `dependency_diameter` — see this
    /// module's Tier-B section comment for rationale.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "node_count / edge_count are usize; their f64 conversion is precise up to 2^53 entries, far above any plausible workspace graph size"
    )]
    pub fn average_fanout(&self) -> f64 {
        let nodes = self.node_count();
        if nodes == 0 {
            return 0.0;
        }
        self.edge_count() as f64 / nodes as f64
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Remove an edge without checking for existence. Returns the record if
    /// present, updating both adjacency caches.
    fn remove_edge_unchecked(&mut self, id: EdgeId) -> Option<EdgeRecord<E>> {
        let rec = self.edges.remove(&id)?;
        if let Some(set) = self.outgoing.get_mut(&rec.src) {
            set.remove(&id);
        }
        if let Some(set) = self.incoming.get_mut(&rec.dst) {
            set.remove(&id);
        }
        Some(rec)
    }

    /// Tier-B partial-recomputation path for [`Graph::max_out_fanout`].
    /// Walks the `outgoing` adjacency map keys and finds the maximum
    /// out-degree. Bounded by the count of nodes that have at least one
    /// outgoing edge (`<= node_count`). Invoked from
    /// [`Graph::remove_edge`] / [`Graph::remove_node`] only when the
    /// staleness check at the call site flagged the cache; the typical
    /// remove case is O(1) (cache provably still attainable).
    fn recompute_max_out_fanout(&mut self) {
        self.max_out_fanout = self
            .outgoing
            .values()
            .map(|set| u32::try_from(set.len()).unwrap_or(u32::MAX))
            .max()
            .unwrap_or(0);
    }

    /// Tier-B partial-recomputation path for [`Graph::max_in_fanout`].
    /// Symmetric companion to [`Graph::recompute_max_out_fanout`] over
    /// the `incoming` adjacency map keys.
    fn recompute_max_in_fanout(&mut self) {
        self.max_in_fanout = self
            .incoming
            .values()
            .map(|set| u32::try_from(set.len()).unwrap_or(u32::MAX))
            .max()
            .unwrap_or(0);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
//
// Extracted to `graph/tests.rs` (Phase 5 split — the original inline
// `#[cfg(test)] mod tests` block pushed `graph.rs` past the 1000-line
// hard cap once the ADR-115 phase-2 Tier-B fanout tests landed). The
// `mod tests;` declaration below is the only test-related code that
// remains in this file; see `graph/tests.rs` for the test definitions.

#[cfg(test)]
mod tests;
