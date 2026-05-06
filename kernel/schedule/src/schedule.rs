//! Deterministic system scheduler.

use std::collections::{BTreeMap, BTreeSet};

use rge_kernel_diagnostics::DiagnosticSink;

use crate::{Stage, SystemDescriptor, SystemId};

/// Errors returned by [`Schedule`] operations.
#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    /// Two systems were registered with the same [`SystemId`].
    #[error("duplicate system id: {0:?}")]
    DuplicateSystem(SystemId),

    /// A dependency cycle was detected; the vec contains all system IDs
    /// involved (those remaining with non-zero in-degree after Kahn's
    /// algorithm terminates).
    #[error("dependency cycle detected involving systems: {0:?}")]
    Cycle(Vec<SystemId>),

    /// A system lists a dependency that is either not registered or lives in a
    /// later stage (cross-stage back-edge).
    #[error("system {dependent:?} depends on missing system {missing:?}")]
    MissingDependency {
        /// The system that declared the dependency.
        dependent: SystemId,
        /// The system that was referenced but is absent or in a later stage.
        missing: SystemId,
    },

    /// [`Schedule::run`] was called before [`Schedule::build`].
    #[error("schedule must be built before run; call .build() first")]
    NotBuilt,
}

/// A boxed system callback: `FnMut(&mut dyn DiagnosticSink) + Send`.
///
/// Aliased here to avoid the `clippy::type_complexity` lint on the field.
pub type SystemFn = Box<dyn FnMut(&mut dyn DiagnosticSink) + Send>;

/// Deterministic, single-threaded system scheduler.
///
/// # Usage
///
/// 1. Register systems via [`add_system`][Self::add_system].
/// 2. Call [`build`][Self::build] to validate deps and produce a deterministic
///    execution order (topo-sort within each stage, alphabetical tiebreak).
/// 3. Call [`run`][Self::run] each frame to execute every system once.
///
/// # Determinism guarantee
///
/// - Stages execute in [`Stage::ALL`] order (ascending discriminant).
/// - Within each stage, systems are ordered by Kahn's topological sort.
/// - When multiple nodes are eligible simultaneously, the one with the
///   lexicographically smallest [`SystemId`] runs first.
#[derive(Default)]
pub struct Schedule {
    /// Registered systems in insertion order (owned storage).
    systems: Vec<SystemDescriptor>,
    /// True after a successful [`build`][Self::build] call.
    built: bool,
    /// Per-stage deterministic execution order produced by `build`.
    ///
    /// Values are indices into `self.systems`.
    stage_order: BTreeMap<Stage, Vec<usize>>,
}

impl Schedule {
    /// Create an empty schedule.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a system.
    ///
    /// Adding a new system after a successful `build` resets the built flag;
    /// you must call `build` again before `run`.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::DuplicateSystem`] if a system with the same
    /// [`SystemId`] has already been added.
    pub fn add_system(&mut self, descriptor: SystemDescriptor) -> Result<(), ScheduleError> {
        if self.systems.iter().any(|s| s.id == descriptor.id) {
            return Err(ScheduleError::DuplicateSystem(descriptor.id));
        }
        self.systems.push(descriptor);
        self.built = false;
        self.stage_order.clear();
        Ok(())
    }

    /// Validate all dependencies and produce a deterministic execution order.
    ///
    /// Steps:
    /// 1. For every declared dependency, verify the dependency exists and lives
    ///    in the same or an *earlier* stage (cross-stage back-edges are errors).
    /// 2. Within each stage, run Kahn's algorithm with alphabetical tiebreaking
    ///    to produce a deterministic topo sort.
    /// 3. If Kahn's terminates with remaining nodes, a cycle exists — report
    ///    the involved IDs.
    ///
    /// # Errors
    ///
    /// - [`ScheduleError::MissingDependency`] — a `depends_on` target is not
    ///   registered, or is in a later stage (cross-stage back-edge).
    /// - [`ScheduleError::Cycle`] — circular dependency within a stage.
    ///
    /// # Panics
    ///
    /// Does not panic in correct usage. An internal `expect` guards a
    /// post-validation invariant (dep index must exist after the validation
    /// pass); it cannot fire unless the registration data is corrupted.
    pub fn build(&mut self) -> Result<(), ScheduleError> {
        // Build a fast lookup: SystemId → (index, stage).
        let id_to_idx: BTreeMap<SystemId, usize> = self
            .systems
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();

        // Validate every dependency edge.
        for sys in &self.systems {
            for &dep_id in &sys.depends_on {
                match id_to_idx.get(&dep_id) {
                    None => {
                        return Err(ScheduleError::MissingDependency {
                            dependent: sys.id,
                            missing: dep_id,
                        })
                    }
                    Some(&dep_idx) => {
                        let dep_stage = self.systems[dep_idx].stage;
                        // Cross-stage back-edge: dep is in a *later* stage.
                        if dep_stage > sys.stage {
                            return Err(ScheduleError::MissingDependency {
                                dependent: sys.id,
                                missing: dep_id,
                            });
                        }
                        // Cross-stage forward deps (dep_stage < sys.stage) are
                        // satisfied by stage order; no intra-stage edge needed.
                    }
                }
            }
        }

        // Topo-sort within each stage using Kahn's algorithm.
        let mut new_stage_order: BTreeMap<Stage, Vec<usize>> = BTreeMap::new();

        for &stage in Stage::ALL {
            // Collect systems that belong to this stage.
            let stage_indices: Vec<usize> = self
                .systems
                .iter()
                .enumerate()
                .filter(|(_, s)| s.stage == stage)
                .map(|(i, _)| i)
                .collect();

            if stage_indices.is_empty() {
                new_stage_order.insert(stage, Vec::new());
                continue;
            }

            // Build in-degree and adjacency within this stage only.
            // Index remapping: stage_indices[local] = global index.
            let local_count = stage_indices.len();
            // Map global index → local index.
            let global_to_local: BTreeMap<usize, usize> = stage_indices
                .iter()
                .enumerate()
                .map(|(local, &global)| (global, local))
                .collect();

            let mut in_degree = vec![0usize; local_count];
            // adj[from_local] = list of local successors.
            let mut adj: Vec<Vec<usize>> = vec![Vec::new(); local_count];

            for (local_idx, &global_idx) in stage_indices.iter().enumerate() {
                let sys = &self.systems[global_idx];
                for &dep_id in &sys.depends_on {
                    let dep_global = *id_to_idx.get(&dep_id).expect("dep validated in first pass");
                    // Only process intra-stage edges here.
                    if let Some(&dep_local) = global_to_local.get(&dep_global) {
                        // dep_local → local_idx (dep runs before sys).
                        adj[dep_local].push(local_idx);
                        in_degree[local_idx] += 1;
                    }
                    // Cross-stage forward deps are satisfied implicitly by stage
                    // order; they contribute no in-degree within this stage.
                }
            }

            // Kahn's: seed queue with zero-in-degree nodes, break ties by
            // SystemId (lexicographic) for determinism.
            let mut ready: BTreeSet<(SystemId, usize)> = stage_indices
                .iter()
                .enumerate()
                .filter(|(local, _)| in_degree[*local] == 0)
                .map(|(local, &global)| (self.systems[global].id, local))
                .collect();

            let mut sorted_globals: Vec<usize> = Vec::with_capacity(local_count);

            while let Some((_, local)) = ready.pop_first() {
                sorted_globals.push(stage_indices[local]);
                for successor_local in adj[local].clone() {
                    in_degree[successor_local] -= 1;
                    if in_degree[successor_local] == 0 {
                        let succ_global = stage_indices[successor_local];
                        ready.insert((self.systems[succ_global].id, successor_local));
                    }
                }
            }

            // If we did not emit all nodes, a cycle exists.
            if sorted_globals.len() != local_count {
                let cycle_ids: Vec<SystemId> = stage_indices
                    .iter()
                    .enumerate()
                    .filter(|(local, _)| in_degree[*local] > 0)
                    .map(|(_, &global)| self.systems[global].id)
                    .collect();
                return Err(ScheduleError::Cycle(cycle_ids));
            }

            new_stage_order.insert(stage, sorted_globals);
        }

        self.stage_order = new_stage_order;
        self.built = true;
        Ok(())
    }

    /// Execute every registered system once in deterministic order.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::NotBuilt`] if [`build`][Self::build] has not
    /// been called successfully since the last [`add_system`][Self::add_system].
    pub fn run(&mut self, sink: &mut dyn DiagnosticSink) -> Result<(), ScheduleError> {
        if !self.built {
            return Err(ScheduleError::NotBuilt);
        }

        // Collect the execution order as a flat list of global indices first,
        // so we can then mutably borrow `self.systems` for the run callbacks.
        let order: Vec<usize> = Stage::ALL
            .iter()
            .flat_map(|stage| {
                self.stage_order
                    .get(stage)
                    .map_or(&[] as &[usize], Vec::as_slice)
                    .iter()
                    .copied()
            })
            .collect();

        for idx in order {
            (self.systems[idx].run)(sink);
        }

        Ok(())
    }

    /// Number of registered systems.
    #[must_use]
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    /// Return the deterministic execution order across all stages.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::NotBuilt`] if [`build`][Self::build] has not
    /// been called successfully.
    pub fn execution_order(&self) -> Result<Vec<SystemId>, ScheduleError> {
        if !self.built {
            return Err(ScheduleError::NotBuilt);
        }

        let ids = Stage::ALL
            .iter()
            .flat_map(|stage| {
                self.stage_order
                    .get(stage)
                    .map_or(&[] as &[usize], Vec::as_slice)
                    .iter()
                    .map(|&idx| self.systems[idx].id)
            })
            .collect();

        Ok(ids)
    }
}

impl std::fmt::Debug for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schedule")
            .field("system_count", &self.systems.len())
            .field("built", &self.built)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Stage, SystemDescriptor, SystemId};

    fn sid(name: &'static str) -> SystemId {
        SystemId(name)
    }

    fn noop_sys(id: SystemId, stage: Stage) -> SystemDescriptor {
        SystemDescriptor::new(id, stage, |_sink| {})
    }

    #[test]
    fn new_schedule_is_empty() {
        let s = Schedule::new();
        assert_eq!(s.system_count(), 0);
        assert!(!s.built);
    }

    #[test]
    fn default_equals_new() {
        let s = Schedule::default();
        assert_eq!(s.system_count(), 0);
    }

    #[test]
    fn add_system_increments_count() {
        let mut s = Schedule::new();
        s.add_system(noop_sys(sid("a"), Stage::Update)).unwrap();
        assert_eq!(s.system_count(), 1);
    }

    #[test]
    fn duplicate_system_errors() {
        let mut s = Schedule::new();
        s.add_system(noop_sys(sid("dup"), Stage::Update)).unwrap();
        let err = s
            .add_system(noop_sys(sid("dup"), Stage::Update))
            .unwrap_err();
        assert!(matches!(err, ScheduleError::DuplicateSystem(_)));
    }

    #[test]
    fn run_before_build_errors() {
        let mut s = Schedule::new();
        s.add_system(noop_sys(sid("x"), Stage::Update)).unwrap();
        let err = s.run(&mut ()).unwrap_err();
        assert!(matches!(err, ScheduleError::NotBuilt));
    }

    #[test]
    fn execution_order_before_build_errors() {
        let s = Schedule::new();
        assert!(matches!(s.execution_order(), Err(ScheduleError::NotBuilt)));
    }
}
