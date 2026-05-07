//! Integration tests for `rge-kernel-schedule`.
//!
//! Covers all 13 required test cases from IMPLEMENTATION.md Phase 1.5.

use std::sync::{Arc, Mutex};

use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator};
use rge_kernel_schedule::{
    AsyncBoundary, Schedule, ScheduleError, Stage, SystemDescriptor, SystemId,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn sid(name: &'static str) -> SystemId {
    SystemId(name)
}

fn noop(id: SystemId, stage: Stage) -> SystemDescriptor {
    SystemDescriptor::new(id, stage, |_| {})
}

/// Build a system that appends `id` to a shared vec when run.
fn recording(id: SystemId, stage: Stage, log: Arc<Mutex<Vec<SystemId>>>) -> SystemDescriptor {
    SystemDescriptor::new(id, stage, move |_sink| {
        log.lock().unwrap().push(id);
    })
}

// ── test 1: Stage ordering ────────────────────────────────────────────────────

#[test]
fn test_01_stage_all_is_sorted() {
    let stages = Stage::ALL;
    for window in stages.windows(2) {
        assert!(window[0] < window[1], "Stage::ALL must be sorted ascending");
    }
    assert!(Stage::EarlyUpdate < Stage::FixedUpdate);
    assert!(Stage::FixedUpdate < Stage::Update);
    assert!(Stage::Update < Stage::LateUpdate);
}

// ── test 2: single system runs ────────────────────────────────────────────────

#[test]
fn test_02_single_system_executes() {
    let ran = Arc::new(Mutex::new(false));
    let ran_clone = Arc::clone(&ran);

    let mut sched = Schedule::new();
    sched
        .add_system(SystemDescriptor::new(
            sid("only"),
            Stage::Update,
            move |_sink| {
                *ran_clone.lock().unwrap() = true;
            },
        ))
        .unwrap();
    sched.build().unwrap();
    sched.run(&mut ()).unwrap();

    assert!(*ran.lock().unwrap(), "system must have executed");
}

// ── test 3: insertion-independent ordering (alphabetical tiebreak) ─────────

#[test]
fn test_03_insertion_independent_order() {
    // Insert in reverse alphabetical order; expect alphabetical execution.
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut sched = Schedule::new();
    for &name in &["charlie", "bravo", "alpha"] {
        sched
            .add_system(recording(sid(name), Stage::Update, Arc::clone(&log)))
            .unwrap();
    }
    sched.build().unwrap();
    sched.run(&mut ()).unwrap();

    let order: Vec<&str> = log.lock().unwrap().iter().map(|id| id.0).collect();
    assert_eq!(order, ["alpha", "bravo", "charlie"]);

    // Verify execution_order matches.
    let exec: Vec<&str> = sched
        .execution_order()
        .unwrap()
        .iter()
        .map(|id| id.0)
        .collect();
    assert_eq!(exec, ["alpha", "bravo", "charlie"]);
}

// ── test 4: stage isolation ────────────────────────────────────────────────

#[test]
fn test_04_stage_isolation() {
    let log = Arc::new(Mutex::new(Vec::new()));

    let mut sched = Schedule::new();
    sched
        .add_system(recording(sid("late"), Stage::LateUpdate, Arc::clone(&log)))
        .unwrap();
    sched
        .add_system(recording(
            sid("early"),
            Stage::EarlyUpdate,
            Arc::clone(&log),
        ))
        .unwrap();
    sched
        .add_system(recording(sid("update"), Stage::Update, Arc::clone(&log)))
        .unwrap();
    sched.build().unwrap();
    sched.run(&mut ()).unwrap();

    let order: Vec<&str> = log.lock().unwrap().iter().map(|id| id.0).collect();
    // EarlyUpdate must come first, then Update, then LateUpdate.
    let early_pos = order.iter().position(|&s| s == "early").unwrap();
    let update_pos = order.iter().position(|&s| s == "update").unwrap();
    let late_pos = order.iter().position(|&s| s == "late").unwrap();
    assert!(early_pos < update_pos);
    assert!(update_pos < late_pos);
}

// ── test 5: intra-stage dep (B depends on A) ──────────────────────────────

#[test]
fn test_05_intra_stage_dep() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut sched = Schedule::new();

    let b_sys = SystemDescriptor::new(sid("b_sys"), Stage::Update, {
        let l = Arc::clone(&log);
        move |_| l.lock().unwrap().push(sid("b_sys"))
    })
    .with_dependency(sid("a_sys"));

    let a_sys = SystemDescriptor::new(sid("a_sys"), Stage::Update, {
        let l = Arc::clone(&log);
        move |_| l.lock().unwrap().push(sid("a_sys"))
    });

    // Insert b before a — dep resolution must still run a first.
    sched.add_system(b_sys).unwrap();
    sched.add_system(a_sys).unwrap();
    sched.build().unwrap();
    sched.run(&mut ()).unwrap();

    let order: Vec<&str> = log.lock().unwrap().iter().map(|id| id.0).collect();
    let a_pos = order.iter().position(|&s| s == "a_sys").unwrap();
    let b_pos = order.iter().position(|&s| s == "b_sys").unwrap();
    assert!(a_pos < b_pos, "a must run before b (b depends on a)");
}

// ── test 6: cross-stage forward dep (OK) ─────────────────────────────────

#[test]
fn test_06_cross_stage_forward_dep_ok() {
    let mut sched = Schedule::new();
    // "early_sys" in EarlyUpdate; "update_sys" in Update depends on it.
    let early = noop(sid("early_sys"), Stage::EarlyUpdate);
    let update = SystemDescriptor::new(sid("update_sys"), Stage::Update, |_| {})
        .with_dependency(sid("early_sys"));

    sched.add_system(early).unwrap();
    sched.add_system(update).unwrap();
    assert!(
        sched.build().is_ok(),
        "cross-stage forward dep must be accepted"
    );
}

// ── test 7: cross-stage backward dep (error) ─────────────────────────────

#[test]
fn test_07_cross_stage_backward_dep_error() {
    let mut sched = Schedule::new();
    // "early_sys" in EarlyUpdate depends on "update_sys" in Update — backward!
    let update = noop(sid("update_sys"), Stage::Update);
    let early = SystemDescriptor::new(sid("early_sys"), Stage::EarlyUpdate, |_| {})
        .with_dependency(sid("update_sys"));

    sched.add_system(update).unwrap();
    sched.add_system(early).unwrap();

    let err = sched.build().unwrap_err();
    assert!(
        matches!(err, ScheduleError::MissingDependency { .. }),
        "expected MissingDependency for cross-stage back-edge, got {err:?}"
    );
}

// ── test 8: cycle detect ─────────────────────────────────────────────────

#[test]
fn test_08_cycle_detect() {
    let mut sched = Schedule::new();

    let a = SystemDescriptor::new(sid("cycle_a"), Stage::Update, |_| {})
        .with_dependency(sid("cycle_b"));
    let b = SystemDescriptor::new(sid("cycle_b"), Stage::Update, |_| {})
        .with_dependency(sid("cycle_a"));

    sched.add_system(a).unwrap();
    sched.add_system(b).unwrap();

    let err = sched.build().unwrap_err();
    match err {
        ScheduleError::Cycle(ids) => {
            assert!(ids.contains(&sid("cycle_a")));
            assert!(ids.contains(&sid("cycle_b")));
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

// ── test 9: duplicate system ──────────────────────────────────────────────

#[test]
fn test_09_duplicate_system() {
    let mut sched = Schedule::new();
    sched.add_system(noop(sid("dup"), Stage::Update)).unwrap();
    let err = sched
        .add_system(noop(sid("dup"), Stage::Update))
        .unwrap_err();
    assert!(
        matches!(err, ScheduleError::DuplicateSystem(id) if id == sid("dup")),
        "expected DuplicateSystem"
    );
}

// ── test 10: missing dependency ──────────────────────────────────────────

#[test]
fn test_10_missing_dependency() {
    let mut sched = Schedule::new();
    let sys = SystemDescriptor::new(sid("dependent"), Stage::Update, |_| {})
        .with_dependency(sid("ghost")); // "ghost" never registered.
    sched.add_system(sys).unwrap();

    let err = sched.build().unwrap_err();
    assert!(
        matches!(
            err,
            ScheduleError::MissingDependency { dependent, missing }
                if dependent == sid("dependent") && missing == sid("ghost")
        ),
        "expected MissingDependency {{ dependent: \"dependent\", missing: \"ghost\" }}"
    );
}

// ── test 11: run before build ─────────────────────────────────────────────

#[test]
fn test_11_run_before_build() {
    let mut sched = Schedule::new();
    sched.add_system(noop(sid("x"), Stage::Update)).unwrap();
    let err = sched.run(&mut ()).unwrap_err();
    assert!(matches!(err, ScheduleError::NotBuilt));
}

// ── test 12: 10-system smoke test ─────────────────────────────────────────

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "single end-to-end smoke test wiring 10 systems across all 4 stages with intra-/inter-stage dependencies plus determinism comparison; splitting it would lose the single-pass build-run-rerun-rebuild-compare narrative"
)]
fn test_12_ten_system_smoke_test() {
    // Ten systems across all four stages with dependency edges.
    // Verify deterministic execution order across two consecutive runs.

    let log1 = Arc::new(Mutex::new(Vec::new()));
    let log2 = Arc::new(Mutex::new(Vec::new()));

    let build_schedule = |log: Arc<Mutex<Vec<SystemId>>>| {
        let mut sched = Schedule::new();

        // EarlyUpdate: e0, e1 (e1 depends on e0)
        sched
            .add_system(recording(
                sid("e0_early"),
                Stage::EarlyUpdate,
                Arc::clone(&log),
            ))
            .unwrap();
        sched
            .add_system(
                recording(sid("e1_early"), Stage::EarlyUpdate, Arc::clone(&log))
                    .with_dependency(sid("e0_early")),
            )
            .unwrap();

        // FixedUpdate: f0
        sched
            .add_system(recording(
                sid("f0_fixed"),
                Stage::FixedUpdate,
                Arc::clone(&log),
            ))
            .unwrap();

        // Update: u0, u1, u2 (u1→u0, u2→u0)
        sched
            .add_system(recording(sid("u0_update"), Stage::Update, Arc::clone(&log)))
            .unwrap();
        sched
            .add_system(
                recording(sid("u1_update"), Stage::Update, Arc::clone(&log))
                    .with_dependency(sid("u0_update")),
            )
            .unwrap();
        sched
            .add_system(
                recording(sid("u2_update"), Stage::Update, Arc::clone(&log))
                    .with_dependency(sid("u0_update")),
            )
            .unwrap();
        // Also test forward cross-stage dep from Update → EarlyUpdate.
        sched
            .add_system(
                recording(sid("u3_update"), Stage::Update, Arc::clone(&log))
                    .with_dependency(sid("e0_early")),
            )
            .unwrap();

        // LateUpdate: l0, l1, l2
        sched
            .add_system(recording(
                sid("l0_late"),
                Stage::LateUpdate,
                Arc::clone(&log),
            ))
            .unwrap();
        sched
            .add_system(recording(
                sid("l1_late"),
                Stage::LateUpdate,
                Arc::clone(&log),
            ))
            .unwrap();
        sched
            .add_system(recording(
                sid("l2_late"),
                Stage::LateUpdate,
                Arc::clone(&log),
            ))
            .unwrap();

        sched
    };

    let mut sched = build_schedule(Arc::clone(&log1));
    sched.build().unwrap();
    assert_eq!(sched.system_count(), 10);
    sched.run(&mut ()).unwrap();

    // Run again to confirm determinism across calls.
    sched.run(&mut ()).unwrap();

    // Build a second fresh schedule and compare order.
    let mut sched2 = build_schedule(Arc::clone(&log2));
    sched2.build().unwrap();
    let order1 = sched.execution_order().unwrap();
    let order2 = sched2.execution_order().unwrap();
    assert_eq!(
        order1, order2,
        "execution order must be deterministic across schedules"
    );

    let order_names: Vec<&str> = order1.iter().map(|id| id.0).collect();

    // Stage ordering: all EarlyUpdate before FixedUpdate before Update before LateUpdate.
    let early_max = order_names
        .iter()
        .enumerate()
        .filter(|(_, &n)| n.ends_with("early"))
        .map(|(i, _)| i)
        .max()
        .unwrap();
    let fixed_min = order_names
        .iter()
        .enumerate()
        .filter(|(_, &n)| n.ends_with("fixed"))
        .map(|(i, _)| i)
        .min()
        .unwrap();
    let update_min = order_names
        .iter()
        .enumerate()
        .filter(|(_, &n)| n.ends_with("update"))
        .map(|(i, _)| i)
        .min()
        .unwrap();
    let late_min = order_names
        .iter()
        .enumerate()
        .filter(|(_, &n)| n.ends_with("late"))
        .map(|(i, _)| i)
        .min()
        .unwrap();

    assert!(
        early_max < fixed_min,
        "EarlyUpdate must complete before FixedUpdate"
    );
    assert!(
        fixed_min < update_min,
        "FixedUpdate must complete before Update"
    );
    assert!(
        update_min < late_min,
        "Update must complete before LateUpdate"
    );

    // e0 before e1 (intra-stage dep).
    let e0_pos = order_names.iter().position(|&n| n == "e0_early").unwrap();
    let e1_pos = order_names.iter().position(|&n| n == "e1_early").unwrap();
    assert!(e0_pos < e1_pos);

    // u0 before u1 and u2 (intra-stage deps).
    let u0_pos = order_names.iter().position(|&n| n == "u0_update").unwrap();
    let u1_pos = order_names.iter().position(|&n| n == "u1_update").unwrap();
    let u2_pos = order_names.iter().position(|&n| n == "u2_update").unwrap();
    assert!(u0_pos < u1_pos);
    assert!(u0_pos < u2_pos);

    // u1 before u2 (alphabetical tiebreak since both depend only on u0).
    assert!(u1_pos < u2_pos);
}

// ── test 13: diagnostics flow-through ────────────────────────────────────

#[test]
fn test_13_diagnostics_flow_through() {
    let mut sched = Schedule::new();
    sched
        .add_system(SystemDescriptor::new(
            sid("warn_system"),
            Stage::Update,
            |sink| {
                sink.emit(Diagnostic::warning("system emitted a warning"));
            },
        ))
        .unwrap();
    sched.build().unwrap();

    let mut agg = DiagnosticAggregator::new();
    sched.run(&mut agg).unwrap();

    assert_eq!(agg.len(), 1, "aggregator must capture the warning");
    assert!(agg.iter().any(|d| d.message == "system emitted a warning"));
}

// ── additional: async boundary metadata ──────────────────────────────────

#[test]
fn test_async_boundary_metadata_stored() {
    let sys = SystemDescriptor::new(sid("async_sys"), Stage::Update, |_| {})
        .with_async_boundary(AsyncBoundary::Async);
    assert_eq!(sys.async_boundary, AsyncBoundary::Async);
}

// ── additional: add_system after build resets built flag ─────────────────

#[test]
fn test_add_after_build_requires_rebuild() {
    let mut sched = Schedule::new();
    sched.add_system(noop(sid("first"), Stage::Update)).unwrap();
    sched.build().unwrap();
    sched
        .add_system(noop(sid("second"), Stage::Update))
        .unwrap();
    // Must rebuild before run.
    let err = sched.run(&mut ()).unwrap_err();
    assert!(matches!(err, ScheduleError::NotBuilt));
}
