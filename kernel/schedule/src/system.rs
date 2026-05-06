//! System identity, descriptor, and async-boundary declarations.

use rge_kernel_diagnostics::DiagnosticSink;
use serde::{Deserialize, Serialize};

use crate::schedule::SystemFn;
use crate::Stage;

/// Stable system identifier — interned on registration via a `&'static str` name.
///
/// Ordering is lexicographic on the name string, which is the tiebreaking rule
/// used by the topological sort to guarantee determinism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SystemId(pub &'static str);

impl std::fmt::Display for SystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Declares whether a system performs async work.
///
/// Currently metadata only — execution is always synchronous in Phase 1.
/// A future scheduler may inspect this field to insert sync points or
/// schedule work on an async executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AsyncBoundary {
    /// Pure CPU work; no async I/O or yield points.
    Sync,
    /// Declared async boundary (asset load, network, GPU readback). Sync today.
    Async,
}

/// Registration record for one system.
///
/// Build descriptors via [`SystemDescriptor::new`] and refine with the
/// `with_*` builder methods before passing to [`Schedule::add_system`].
pub struct SystemDescriptor {
    /// Stable identity for this system.
    pub id: SystemId,
    /// Which stage this system belongs to.
    pub stage: Stage,
    /// Direct dependency edges — resolved and cycle-checked during
    /// [`Schedule::build`].
    pub depends_on: Vec<SystemId>,
    /// Whether this system declares an async boundary (metadata only).
    pub async_boundary: AsyncBoundary,
    /// The callable to invoke during [`Schedule::run`].
    pub run: SystemFn,
}

impl SystemDescriptor {
    /// Create a new descriptor with default values (no deps, `Sync` boundary).
    pub fn new<F>(id: SystemId, stage: Stage, run: F) -> Self
    where
        F: FnMut(&mut dyn DiagnosticSink) + Send + 'static,
    {
        Self {
            id,
            stage,
            depends_on: Vec::new(),
            async_boundary: AsyncBoundary::Sync,
            run: Box::new(run),
        }
    }

    /// Declare a direct dependency on another system (within the same stage or
    /// an earlier stage). Evaluated during [`Schedule::build`].
    #[must_use]
    pub fn with_dependency(mut self, dep: SystemId) -> Self {
        self.depends_on.push(dep);
        self
    }

    /// Override the async-boundary metadata for this system.
    #[must_use]
    pub fn with_async_boundary(mut self, boundary: AsyncBoundary) -> Self {
        self.async_boundary = boundary;
        self
    }
}

impl std::fmt::Debug for SystemDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemDescriptor")
            .field("id", &self.id)
            .field("stage", &self.stage)
            .field("depends_on", &self.depends_on)
            .field("async_boundary", &self.async_boundary)
            .finish_non_exhaustive()
    }
}
