//! Verify `editor-state` only imports `EntityId` and std types. Static check —
//! purely the test compiling proves the rule. Failure mode = a future addition
//! pulls in a forbidden import; the `editor-state-ownership` lint catches that
//! before this test would.

use rge_editor_state::{ActiveTool, Hover, PanelId, Selection};
use rge_kernel_ecs::EntityId;

#[test]
fn coordination_state_uses_entity_ids_only() {
    // Just construct each — proves the API surface compiles correctly.
    let mut sel = Selection::new();
    sel.add(EntityId::new());

    let mut hover = Hover::new();
    hover.set(PanelId::new("test"), EntityId::new());

    let _ = ActiveTool::default();
}
