//! Phase 5.3 PIE constitutional gate: Play → ticks → Stop → world byte-identical.
//!
//! Uses the **real `rge_kernel_ecs::World`** with typed [`SnapshotComponent`]s
//! (Position + `TickCounter`) registered on the shell's world. Replaces the W03
//! stub which used `ComponentTypeId(u32)` + raw byte blobs.
//!
//! Per PLAN.md §6.13 + IMPLEMENTATION.md Phase 5.3: if this test fails, the
//! ECS storage or snapshot path needs redesign.

use rge_editor_shell::{EditorShell, PlayState, PlayStateTransition, ToolbarButtonId};
use rge_kernel_ecs::{Component, SnapshotComponent};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Typed fixture components
// ---------------------------------------------------------------------------

/// 3-axis position component used in the round-trip scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}
impl Component for Position {}
impl SnapshotComponent for Position {}

/// Monotonic tick counter used in the round-trip scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TickCounter(u64);
impl Component for TickCounter {}
impl SnapshotComponent for TickCounter {}

// ---------------------------------------------------------------------------
// Scene builder
// ---------------------------------------------------------------------------

/// Build the canonical 100-entity scene used by all PIE round-trip tests.
///
/// Registers `Position` and `TickCounter` as snapshot components, then spawns
/// 100 entities each carrying both. The kernel world is the authority for
/// snapshot bytes; the blob-API entity set is also populated so `entity_count`
/// is correct.
fn build_100_entity_scene(shell: &mut EditorShell) {
    shell.world_mut().register_snapshot_component::<Position>();
    shell
        .world_mut()
        .register_snapshot_component::<TickCounter>();
    #[allow(clippy::cast_precision_loss)]
    for i in 0..100u64 {
        let e = shell.world_mut().spawn();
        shell.world_mut().kernel_mut().insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        shell.world_mut().kernel_mut().insert(e, TickCounter(i));
    }
}

/// Advance game state by mutating typed components in the kernel world.
///
/// Increments each `TickCounter` by 1 and shifts each `Position.x` by 0.1
/// per tick. Called instead of `tick_game_systems` so the typed snapshot
/// components are actually modified during play.
fn advance_kernel_tick(shell: &mut EditorShell) {
    let entities: Vec<_> = shell
        .world()
        .kernel()
        .query::<TickCounter>()
        .map(|(e, _)| e)
        .collect();
    for e in entities {
        let current_tick = shell
            .world()
            .kernel()
            .entity(e)
            .and_then(|er| er.get::<TickCounter>().cloned());
        if let Some(TickCounter(t)) = current_tick {
            shell.world_mut().kernel_mut().insert(e, TickCounter(t + 1));
        }
        let current_pos = shell
            .world()
            .kernel()
            .entity(e)
            .and_then(|er| er.get::<Position>().cloned());
        if let Some(Position { x, y, z }) = current_pos {
            shell
                .world_mut()
                .kernel_mut()
                .insert(e, Position { x: x + 0.1, y, z });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Constitutional gate: kernel snapshot is byte-identical after Play → Stop.
#[test]
fn pie_round_trip_byte_identical() {
    let mut shell = EditorShell::new();
    build_100_entity_scene(&mut shell);
    assert_eq!(shell.world().entity_count(), 100);

    // Capture kernel snapshot before Play.
    let pre_play = shell
        .world()
        .serialize_snapshot()
        .expect("pre-play serialize");
    assert_eq!(shell.play_state(), PlayState::Editing);

    // Press Play.
    let t = shell.handle_button(ToolbarButtonId::Play).unwrap();
    assert_eq!(t, PlayStateTransition::StartedPlay);
    assert!(shell.has_snapshot());
    assert_eq!(shell.play_state(), PlayState::Playing);

    // Advance 60 ticks of kernel state.
    for _ in 0..60 {
        advance_kernel_tick(&mut shell);
    }

    let mid_play = shell
        .world()
        .serialize_snapshot()
        .expect("mid-play serialize");
    assert_ne!(
        pre_play, mid_play,
        "60 ticks of kernel mutation must change the snapshot bytes"
    );

    // Press Stop.
    let t = shell.handle_button(ToolbarButtonId::Stop).unwrap();
    assert_eq!(t, PlayStateTransition::StoppedAndRestored);
    assert_eq!(shell.play_state(), PlayState::Editing);
    assert!(!shell.has_snapshot(), "snapshot consumed by Stop");

    // Constitutional check: post-Stop kernel snapshot bytes byte-identical to pre-Play.
    let post_stop = shell
        .world()
        .serialize_snapshot()
        .expect("post-stop serialize");
    assert_eq!(
        pre_play, post_stop,
        "PIE round-trip MUST produce byte-identical kernel snapshot (Phase 5.3 constitutional gate)"
    );
}

/// Entity count is preserved across Play → Stop.
#[test]
fn pie_round_trip_preserves_entity_count() {
    let mut shell = EditorShell::new();
    build_100_entity_scene(&mut shell);
    let pre_count = shell.world().entity_count();

    shell.handle_button(ToolbarButtonId::Play).unwrap();
    for _ in 0..60 {
        advance_kernel_tick(&mut shell);
    }
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    assert_eq!(shell.world().entity_count(), pre_count);
}

/// Stop restores byte-identically even after Pause/Step/Resume.
#[test]
fn pie_round_trip_with_pause_step_resume() {
    let mut shell = EditorShell::new();
    build_100_entity_scene(&mut shell);
    let pre_play = shell
        .world()
        .serialize_snapshot()
        .expect("pre-play serialize");

    shell.handle_button(ToolbarButtonId::Play).unwrap();
    for _ in 0..20 {
        advance_kernel_tick(&mut shell);
    }

    shell.handle_button(ToolbarButtonId::Pause).unwrap();
    shell.handle_button(ToolbarButtonId::Step).unwrap();
    shell.handle_button(ToolbarButtonId::Step).unwrap();
    shell.handle_button(ToolbarButtonId::Play).unwrap(); // resume

    for _ in 0..40 {
        advance_kernel_tick(&mut shell);
    }
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    let post_stop = shell
        .world()
        .serialize_snapshot()
        .expect("post-stop serialize");
    assert_eq!(
        pre_play, post_stop,
        "Pause/Step/Resume cycle must not affect Stop's restore byte-identity"
    );
}

/// Two consecutive PIE sessions both restore cleanly.
#[test]
fn play_stop_play_stop_double_round_trip() {
    let mut shell = EditorShell::new();
    build_100_entity_scene(&mut shell);
    let pre = shell
        .world()
        .serialize_snapshot()
        .expect("initial serialize");

    for cycle in 0..2 {
        shell.handle_button(ToolbarButtonId::Play).unwrap();
        for _ in 0..30 {
            advance_kernel_tick(&mut shell);
        }
        shell.handle_button(ToolbarButtonId::Stop).unwrap();

        let restored = shell
            .world()
            .serialize_snapshot()
            .expect("post-stop serialize");
        assert_eq!(
            pre, restored,
            "round-trip {cycle} must restore byte-identical kernel snapshot"
        );
    }
}
