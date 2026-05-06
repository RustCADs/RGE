//! Invalidation propagation through dependency DAGs.
//!
//! [`Invalidation`] routes dirty-bit signals to registered listeners and
//! recursively walks the dependency graph (supplied by the caller as a
//! closure) to propagate invalidation transitively.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::id::NodeId;

// ---------------------------------------------------------------------------
// Listener handle
// ---------------------------------------------------------------------------

/// Stable handle for a registered [`InvalidationListener`].
///
/// Used to unregister a listener via [`Invalidation::unregister`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ListenerHandle(u64);

// ---------------------------------------------------------------------------
// Listener trait
// ---------------------------------------------------------------------------

/// Receives `on_invalidated` callbacks for every node marked dirty.
///
/// Implementations must be `Send` so that the invalidation router may be used
/// across thread boundaries (e.g., when background jobs trigger invalidation).
pub trait InvalidationListener: Send + 'static {
    /// Called once per dirtied node during a [`Invalidation::mark_dirty`] call.
    ///
    /// Within a single `mark_dirty` propagation, each node is delivered at
    /// most once (deduplicated via a visited set).
    fn on_invalidated(&mut self, node: NodeId);
}

// ---------------------------------------------------------------------------
// Invalidation router
// ---------------------------------------------------------------------------

/// Invalidation router.
///
/// Subscribers register [`InvalidationListener`]s; [`Invalidation::mark_dirty`]
/// invokes every listener AND recursively walks `dependents(node)` (the
/// inverse-edge graph supplied by the caller) to propagate invalidation through
/// the dependency DAG.
///
/// # Example
///
/// ```rust
/// use rge_kernel_graph_foundation::{Invalidation, InvalidationListener, NodeId};
///
/// struct Recorder(Vec<NodeId>);
/// impl InvalidationListener for Recorder {
///     fn on_invalidated(&mut self, node: NodeId) {
///         self.0.push(node);
///     }
/// }
///
/// let mut inv = Invalidation::new();
/// let handle = inv.register(Box::new(Recorder(vec![])));
/// inv.mark_dirty(NodeId::from_raw(1), |_| vec![]);
/// ```
pub struct Invalidation {
    listeners: BTreeMap<ListenerHandle, Box<dyn InvalidationListener>>,
    next_handle: u64,
}

impl Invalidation {
    /// Construct a new, empty invalidation router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: BTreeMap::new(),
            next_handle: 0,
        }
    }

    /// Register a listener. Returns a [`ListenerHandle`] for later removal.
    pub fn register(&mut self, listener: Box<dyn InvalidationListener>) -> ListenerHandle {
        let handle = ListenerHandle(self.next_handle);
        self.next_handle += 1;
        self.listeners.insert(handle, listener);
        handle
    }

    /// Unregister a listener. Returns `true` if the handle was present.
    pub fn unregister(&mut self, handle: ListenerHandle) -> bool {
        self.listeners.remove(&handle).is_some()
    }

    /// Number of currently registered listeners.
    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Mark `root` dirty and propagate through the dependency DAG.
    ///
    /// The `dependents_of` closure maps a node to its downstream dependents
    /// (i.e., nodes that depend on the given node). This is the *inverse* of
    /// the graph's edges: if A → B means "B depends on A", then
    /// `dependents_of(A)` should return `[B]`.
    ///
    /// Each listener receives `on_invalidated` for every dirtied node exactly
    /// once per call (deduplicated via a visited set). Propagation order is
    /// BFS; listeners are invoked in registration order for each node.
    pub fn mark_dirty<F>(&mut self, root: NodeId, dependents_of: F)
    where
        F: Fn(NodeId) -> Vec<NodeId>,
    {
        let mut visited: BTreeSet<NodeId> = BTreeSet::new();
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        queue.push_back(root);

        while let Some(node) = queue.pop_front() {
            if !visited.insert(node) {
                continue; // already processed
            }
            // Notify all listeners for this node.
            for listener in self.listeners.values_mut() {
                listener.on_invalidated(node);
            }
            // Enqueue dependents.
            for dep in dependents_of(node) {
                if !visited.contains(&dep) {
                    queue.push_back(dep);
                }
            }
        }
    }
}

impl Default for Invalidation {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    struct Recorder(Vec<NodeId>);
    impl InvalidationListener for Recorder {
        fn on_invalidated(&mut self, node: NodeId) {
            self.0.push(node);
        }
    }

    #[derive(Clone)]
    struct SharedRecorder(Arc<Mutex<Vec<NodeId>>>);
    impl InvalidationListener for SharedRecorder {
        fn on_invalidated(&mut self, node: NodeId) {
            self.0.lock().unwrap().push(node);
        }
    }

    fn n(v: u128) -> NodeId {
        NodeId::from_raw(v)
    }

    #[test]
    fn register_and_unregister() {
        let mut inv = Invalidation::new();
        let h = inv.register(Box::new(Recorder(vec![])));
        assert_eq!(inv.listener_count(), 1);
        assert!(inv.unregister(h));
        assert_eq!(inv.listener_count(), 0);
        assert!(!inv.unregister(h), "double-unregister returns false");
    }

    #[test]
    fn mark_dirty_single_node_no_deps() {
        let mut inv = Invalidation::new();
        // We can't inspect the box after inserting, so just ensure no panic.
        let _h = inv.register(Box::new(Recorder(vec![])));
        inv.mark_dirty(n(1), |_| vec![]);
    }

    #[test]
    fn mark_dirty_propagates() {
        // A -> B, B -> C, B -> D; just confirms no panic.
        let mut inv = Invalidation::new();
        let _h = inv.register(Box::new(Recorder(vec![])));

        inv.mark_dirty(n(0), |node| {
            if node == n(0) {
                vec![n(1)]
            } else if node == n(1) {
                vec![n(2), n(3)]
            } else {
                vec![]
            }
        });
    }

    #[test]
    fn mark_dirty_deduplicates() {
        // Build a diamond: A -> B, A -> C, B -> D, C -> D
        // D should appear exactly once.
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut inv = Invalidation::new();
        inv.register(Box::new(SharedRecorder(log.clone())));

        let a = n(0);
        let b = n(1);
        let c = n(2);
        let d = n(3);

        inv.mark_dirty(a, |node| {
            if node == a {
                vec![b, c]
            } else if node == b || node == c {
                vec![d]
            } else {
                vec![]
            }
        });

        let calls = log.lock().unwrap();
        // All four nodes should appear exactly once.
        assert_eq!(calls.len(), 4, "each node dirtied exactly once: {calls:?}");
        // d appears at most once.
        let d_count = calls.iter().filter(|&&x| x == d).count();
        assert_eq!(d_count, 1, "diamond dedup: d must appear exactly once");
    }
}
