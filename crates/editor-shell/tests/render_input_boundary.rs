//! Gate C prerequisite dispatch 1 — `RenderInput<'a>` boundary test.
//!
//! Pins the structural shape of the snapshot-handoff boundary
//! between sim/editor state and the render path. Does NOT exercise
//! GPU init, threading, or wire format.
//!
//! See `crates/editor-shell/src/render_input.rs` for the boundary
//! rationale; PLAN.md §13.6 (Gate C measurability) and §1.5.2
//! (`(ECS_tick_N, CadCheckpointId_N)` immutability) for the upstream
//! contract this boundary will eventually enforce.

use rge_editor_shell::{EditorShell, RenderInput};

/// Structural — confirms `RenderInput::from_editor_shell`
/// constructs cleanly from a default-built [`EditorShell`] and that
/// the public type is reachable from outside the crate via the
/// `pub use` re-export in `lib.rs`.
#[test]
fn from_editor_shell_constructs_cleanly() {
    let shell = EditorShell::default();
    let _input = RenderInput::from_editor_shell(&shell);
}

/// Field-presence — confirms `editor_camera` is reachable through
/// [`RenderInput`] as a borrowed reference. The default
/// `EditorCameraState` places the eye at `(3, 3, 3)`; we check that
/// invariant through the view-type to prove the field traversal
/// works (and to guard against accidental rewiring of the field).
#[test]
fn editor_camera_field_reachable_via_render_input() {
    let shell = EditorShell::default();
    let input = RenderInput::from_editor_shell(&shell);
    // Default eye is (3, 3, 3) per `EditorCameraState::default()`.
    assert!((input.editor_camera.eye.x - 3.0).abs() < f32::EPSILON);
    assert!((input.editor_camera.eye.y - 3.0).abs() < f32::EPSILON);
    assert!((input.editor_camera.eye.z - 3.0).abs() < f32::EPSILON);
}
