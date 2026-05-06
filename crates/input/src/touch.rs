// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! Touch fan-in: winit `WindowEvent::Touch` → `InputEvent::TouchStart` /
//! `TouchMove` / `TouchEnd`.
//!
//! Each contact carries a stable `TouchId` (from winit's `Touch::id`) so a
//! 3+ finger gesture can be tracked across the whole start→move→end span
//! without the fan-in layer doing any matching itself. Per W13 exit:
//! `TouchPhase::Cancelled` collapses to `TouchEnd` so consumers don't need
//! a fourth variant.

use winit::event::{Touch, TouchPhase};

use crate::event::{InputEvent, TouchId};

/// Translate a winit `Touch` event. Always returns `Some` — every winit
/// touch phase has a fan-in counterpart (cancelled folds to `TouchEnd`).
///
/// winit reports `f64` positions; we narrow to `f32` because consumers
/// (gesture recognizer, hit-testing) operate in viewport `f32` space.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn translate_touch(touch: Touch) -> InputEvent {
    let id = TouchId(touch.id);
    let pos = [touch.location.x as f32, touch.location.y as f32];
    match touch.phase {
        TouchPhase::Started => InputEvent::TouchStart(id, pos),
        TouchPhase::Moved => InputEvent::TouchMove(id, pos),
        // Cancelled (gesture-recognised system-takeover) is reported as End
        // — the contact is gone from the input substrate's perspective. The
        // gesture-recognizer crate (input-gestures, post-W13) can layer
        // distinction on top by inspecting the prior frame's contact set.
        TouchPhase::Ended | TouchPhase::Cancelled => InputEvent::TouchEnd(id),
    }
}
