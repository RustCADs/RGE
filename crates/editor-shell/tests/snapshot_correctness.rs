//! W03 exit-criterion test: editor-state (selection, active tool) persists
//! across Play/Stop. Per PLAN.md §1.15 + §6.13: editor-state lives on the
//! editor side of the snapshot boundary and does NOT participate in
//! `WorldSnapshot`.

use rge_editor_shell::audit::AuditEvent;
use rge_editor_shell::coord::ActiveTool;
use rge_editor_shell::world::{ComponentTypeId, EntityId};
use rge_editor_shell::{EditorShell, PlayState, ToolbarButtonId};

fn build_5_entity_scene(shell: &mut EditorShell) -> Vec<EntityId> {
    let mut ids = Vec::new();
    for i in 0..5u64 {
        let e = shell.world_mut().spawn();
        shell
            .world_mut()
            .insert_component(e, ComponentTypeId(1), i.to_le_bytes().to_vec());
        ids.push(e);
    }
    ids
}

#[test]
fn selection_persists_across_play_stop() {
    let mut shell = EditorShell::new();
    let ids = build_5_entity_scene(&mut shell);

    // Select two entities.
    shell.coord_mut().selection.add(ids[1]);
    shell.coord_mut().selection.add(ids[3]);
    assert_eq!(shell.coord().selection.len(), 2);

    // Round-trip.
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    // Selection must survive — it's editor-state, not world-state.
    assert_eq!(shell.coord().selection.len(), 2);
    assert!(shell.coord().selection.contains(ids[1]));
    assert!(shell.coord().selection.contains(ids[3]));
}

#[test]
fn active_tool_persists_across_play_stop() {
    let mut shell = EditorShell::new();
    build_5_entity_scene(&mut shell);

    shell.coord_mut().active_tool = ActiveTool::Translate;
    assert_eq!(shell.coord().active_tool, ActiveTool::Translate);

    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    assert_eq!(shell.coord().active_tool, ActiveTool::Translate);
}

#[test]
fn selection_changes_during_play_persist() {
    // User selects a NEW entity *during* Play. That selection change is
    // editor-state and must persist across Stop, even though the world
    // is restored.
    let mut shell = EditorShell::new();
    let ids = build_5_entity_scene(&mut shell);

    shell.coord_mut().selection.add(ids[0]);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(10);

    // Select another entity mid-Play.
    shell.coord_mut().selection.add(ids[2]);
    assert_eq!(shell.coord().selection.len(), 2);

    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    // Both selections survive Stop.
    assert_eq!(shell.coord().selection.len(), 2);
    assert!(shell.coord().selection.contains(ids[0]));
    assert!(shell.coord().selection.contains(ids[2]));
}

#[test]
fn snapshot_does_not_include_editor_coord() {
    // White-box assertion: the SnapshotCaptured audit event records the
    // *world* entity count, not editor-state. Confirms editor-state lives
    // outside the snapshot serialization.
    let mut shell = EditorShell::new();
    let ids = build_5_entity_scene(&mut shell);

    // Pile up editor-state.
    for id in &ids {
        shell.coord_mut().selection.add(*id);
    }
    shell.coord_mut().active_tool = ActiveTool::Brush;

    shell.handle_button(ToolbarButtonId::Play).unwrap();

    let captured = shell
        .audit()
        .iter()
        .find(|e| e.tag() == "SnapshotCaptured")
        .expect("Play should record SnapshotCaptured");

    // The captured event's entity_count is the WORLD's count — not
    // selection count, not coord size, just the 5 entities we spawned.
    if let AuditEvent::SnapshotCaptured { entity_count, .. } = captured {
        assert_eq!(*entity_count, 5);
    } else {
        panic!("expected SnapshotCaptured");
    }
}

#[test]
fn play_state_audit_sequence() {
    let mut shell = EditorShell::new();
    build_5_entity_scene(&mut shell);

    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(5);
    shell.handle_button(ToolbarButtonId::Pause).unwrap();
    shell.handle_button(ToolbarButtonId::Step).unwrap();
    shell.handle_button(ToolbarButtonId::Play).unwrap(); // resume
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    let tags: Vec<&'static str> = shell.audit().iter().map(AuditEvent::tag).collect();

    // Expected order:
    //   SnapshotCaptured, PlayPressed,
    //   PausePressed,
    //   StepPressed,
    //   PlayPressed (resume — no new snapshot),
    //   SnapshotRestored, StopPressed
    assert_eq!(tags[0], "SnapshotCaptured");
    assert_eq!(tags[1], "PlayPressed");
    assert!(tags.contains(&"PausePressed"));
    assert!(tags.contains(&"StepPressed"));
    assert!(tags.contains(&"SnapshotRestored"));
    assert_eq!(*tags.last().unwrap(), "StopPressed");
}

#[test]
fn play_state_starts_in_editing() {
    let shell = EditorShell::new();
    assert_eq!(shell.play_state(), PlayState::Editing);
    assert!(!shell.has_snapshot());
}
