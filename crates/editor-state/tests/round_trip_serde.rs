//! Serde round-trip integration tests for `rge-editor-state` types.
//!
//! Verifies that `Selection`, `Hover`, and `ActiveTool` survive a
//! serialize → deserialize cycle via RON.

use rge_editor_state::{ActiveTool, Hover, PanelId, Selection};
use rge_kernel_ecs::EntityId;

#[test]
fn selection_round_trips_via_ron() {
    let mut sel = Selection::new();
    let ids: Vec<EntityId> = (0..4).map(|_| EntityId::new()).collect();
    for &e in &ids {
        sel.add(e);
    }

    let serialized = ron::to_string(&sel).expect("serialize Selection");
    let deserialized: Selection = ron::from_str(&serialized).expect("deserialize Selection");

    assert_eq!(
        sel, deserialized,
        "round-trip must produce identical Selection"
    );
    // Verify every original ID survives.
    for e in &ids {
        assert!(deserialized.contains(*e));
    }
}

#[test]
fn hover_round_trips_via_ron() {
    let mut hover = Hover::new();
    let panels = ["viewport", "inspector", "scene-tree"];
    let entities: Vec<EntityId> = panels.iter().map(|_| EntityId::new()).collect();
    for (p, e) in panels.iter().zip(entities.iter()) {
        hover.set(PanelId::new(*p), *e);
    }

    let serialized = ron::to_string(&hover).expect("serialize Hover");
    let deserialized: Hover = ron::from_str(&serialized).expect("deserialize Hover");

    assert_eq!(
        hover, deserialized,
        "round-trip must produce identical Hover"
    );
    for (p, e) in panels.iter().zip(entities.iter()) {
        assert_eq!(deserialized.get(&PanelId::new(*p)), Some(*e));
    }
}

#[test]
fn active_tool_round_trips_via_ron() {
    for &tool in ActiveTool::all() {
        let serialized = ron::to_string(&tool).expect("serialize ActiveTool");
        let deserialized: ActiveTool = ron::from_str(&serialized).expect("deserialize ActiveTool");
        assert_eq!(tool, deserialized, "round-trip failed for {tool:?}");
    }
}
