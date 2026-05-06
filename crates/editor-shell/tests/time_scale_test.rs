//! W03 exit-criterion test: time-scale slider affects game systems but
//! NOT editor systems. Per PLAN.md constitutional principle #8: the
//! editor never freezes — game-time dilation must not slow gizmos,
//! panel animations, or the hot-reload watcher.

use rge_editor_shell::world::ComponentTypeId;
use rge_editor_shell::{EditorShell, TimeScale, TimeScaleClass, ToolbarButtonId};

const POSITION: ComponentTypeId = ComponentTypeId(2);

fn position_x(shell: &EditorShell, e: rge_editor_shell::world::EntityId) -> f32 {
    let blob = shell.world().component(e, POSITION).expect("position");
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&blob[0..4]);
    f32::from_le_bytes(bytes)
}

#[test]
fn time_scale_within_range() {
    let t = TimeScale::with_value(0.5);
    assert!(t.value() >= TimeScale::MIN);
    assert!(t.value() <= TimeScale::MAX);

    let clamped_low = TimeScale::with_value(-10.0);
    assert!((clamped_low.value() - TimeScale::MIN).abs() < f32::EPSILON);

    let clamped_high = TimeScale::with_value(100.0);
    assert!((clamped_high.value() - TimeScale::MAX).abs() < f32::EPSILON);
}

#[test]
fn slow_motion_halves_game_progress() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    // 0.5x means 60 game ticks should advance position by ~half compared
    // to 1.0x.
    shell.set_time_scale(0.5);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(60);
    let x_half = position_x(&shell, e);

    shell.handle_button(ToolbarButtonId::Stop).unwrap();
    shell.set_time_scale(1.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(60);
    let x_full = position_x(&shell, e);

    // x_full should be ~2x x_half (within FP noise).
    assert!(x_half > 0.0);
    assert!(x_full > 0.0);
    let ratio = x_full / x_half;
    assert!(
        (ratio - 2.0).abs() < 0.01,
        "expected ~2.0x ratio, got {ratio}"
    );
}

#[test]
fn fast_forward_doubles_game_progress() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    shell.set_time_scale(2.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    let x_2x_30 = position_x(&shell, e);

    shell.handle_button(ToolbarButtonId::Stop).unwrap();
    shell.set_time_scale(1.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    let x_1x_30 = position_x(&shell, e);

    assert!(x_2x_30 > x_1x_30);
    let ratio = x_2x_30 / x_1x_30;
    assert!(
        (ratio - 2.0).abs() < 0.01,
        "expected ~2.0x ratio, got {ratio}"
    );
}

#[test]
fn editor_systems_ignore_time_scale() {
    let scale = TimeScale::with_value(0.01); // extreme slow-motion
    let dt = 0.016_f32;
    let editor_dt = scale.apply(dt, TimeScaleClass::Editor);
    assert!(
        (editor_dt - dt).abs() < 1e-6,
        "editor systems must see raw dt regardless of slider"
    );
}

#[test]
fn time_scale_audit_event_records_change() {
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.25);
    shell.set_time_scale(2.5);

    let mut tsc_count = 0;
    for e in shell.audit().iter() {
        if e.tag() == "TimeScaleChanged" {
            tsc_count += 1;
        }
    }
    assert_eq!(tsc_count, 2);
}

#[test]
fn extreme_min_scale_no_underflow() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    shell.set_time_scale(TimeScale::MIN);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(120);
    let x = position_x(&shell, e);

    // x = 120 ticks * (1/60 dt) * 0.01 scale = 0.02
    assert!(x > 0.0);
    assert!(x < 0.05);
}
