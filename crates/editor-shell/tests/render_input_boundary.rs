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

// Gate C prerequisite dispatch 2 — boundary discipline regression
// =============================================================
// The two tests below pin the structural rule that render-side
// per-frame / per-resize functions must NOT read `self.editor_camera`
// directly; they must consume it via a `&RenderInput<'_>` parameter
// instead. They use source-text inspection (`include_str!`) rather
// than a new architecture lint to keep enforcement editor-shell local
// and dependency-free.
//
// **Scope clarification**: `init_render_state` is one-shot setup and
// is intentionally OUT of the per-frame / per-resize handoff
// boundary, so its `self.editor_camera` reads are not flagged.
//
// Brittleness budget: matches on stable signature prefixes
// (`fn render_frame(`, `fn resize_render_path(`). Robust to
// whitespace inside function bodies; would fail if someone renames
// the functions — that's a deliberate trip-wire, not brittleness.

/// Discipline — `render_frame` body must not read
/// `self.editor_camera` directly. Today's `render_frame` reads zero
/// sim-side state per frame (camera updates land via the GPU UBO
/// from `resize_render_path`); a future regression that reaches
/// into mutable sim state through `self` would defeat the Gate C
/// boundary. PLAN.md §13.6.
#[test]
fn render_frame_body_does_not_read_self_editor_camera() {
    let source = include_str!("../src/render_path.rs");
    let body = function_body(source, "fn render_frame(");
    assert!(
        !body.contains("self.editor_camera"),
        "render_frame body reads `self.editor_camera` directly — route through `RenderInput` instead.\n\nBody:\n{body}"
    );
}

/// Discipline — `resize_render_path` body must not read
/// `self.editor_camera` directly. Per Gate C dispatch 1, this
/// function takes `&RenderInput<'_>` and reads
/// `render_input.editor_camera` for the view*proj update. A
/// regression that bypasses the parameter and reaches into
/// `self.editor_camera` would re-couple the render path to
/// mutable sim state. PLAN.md §13.6 / §1.5.2.
#[test]
fn resize_render_path_body_does_not_read_self_editor_camera() {
    let source = include_str!("../src/render_path.rs");
    let body = function_body(source, "fn resize_render_path(");
    assert!(
        !body.contains("self.editor_camera"),
        "resize_render_path body reads `self.editor_camera` directly — route through `RenderInput` instead.\n\nBody:\n{body}"
    );
}

/// Extracts a function body (`{ ... }`) from `source` by locating
/// the first `{` after `signature_prefix` and walking matched
/// braces. Sufficient for `render_path.rs` (no string literals
/// containing unmatched braces; doc-comments live above the body).
fn function_body<'a>(source: &'a str, signature_prefix: &str) -> &'a str {
    let sig_idx = source
        .find(signature_prefix)
        .unwrap_or_else(|| panic!("signature `{signature_prefix}` not found in render_path.rs"));
    let body_start = source[sig_idx..]
        .find('{')
        .map(|i| sig_idx + i)
        .unwrap_or_else(|| panic!("no opening brace after `{signature_prefix}`"));
    let bytes = source.as_bytes();
    let mut depth: i32 = 0;
    for i in body_start..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[body_start..=i];
                }
            }
            _ => {}
        }
    }
    panic!("function body for `{signature_prefix}` not closed")
}
