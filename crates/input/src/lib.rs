// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted
//
//! `rge-input` — winit + gilrs fan-in into a unified `InputEvent` stream.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: input failures (gilrs gamepad disconnect, dead-zone
//! reload error, key-translation table miss) are transient and recoverable
//! in-place — the event is dropped, the gamepad re-polled, or the user
//! surfaced a diagnostic. `Input<T>` resource state is per-frame and
//! reproducible from the next event stream; no PIE state is owned. Matches
//! audio + ui-theme (stateless / per-frame I/O subsystems).
//!
//! W13 deliverable: a single ordered event vocabulary across keyboard,
//! mouse, gamepad, touch, and stylus, plus `Input<T>` resources for
//! frame-stable "is currently pressed?" queries.
//!
//! ## Architecture
//!
//! - [`event`] — unified [`InputEvent`] enum (KeyDown/KeyUp/MouseMove/...)
//!   plus the reserved `Xr` variant.
//! - [`keyboard`] — winit `KeyEvent` → `InputEvent::KeyDown`/`KeyUp`.
//! - [`mouse`] — winit cursor / button / wheel translation.
//! - [`gamepad`] — gilrs poll + dead-zone normalization.
//! - [`touch`] — winit `Touch` events with stable contact IDs.
//! - [`stylus`] — winit `Force` → normalized pressure.
//! - [`state`] — [`Input<T>`] resource for ECS systems.
//!
//! ## Caller flow (per frame, sketch)
//!
//! ```ignore
//! // Set up once
//! let mut keys: Input<KeyCode> = Input::new();
//! let mut mouse_buttons: Input<MouseButton> = Input::new();
//! let mut touches = TouchState::new();
//! let mut pad = GamepadPoller::new();
//! let mut events: Vec<InputEvent> = Vec::new();
//!
//! // Per-frame: drain winit + gilrs into `events`, then update state.
//! events.clear();
//! keys.clear_just();
//! mouse_buttons.clear_just();
//! // (caller pumps winit WindowEvents into events via `keyboard::translate_keyboard`,
//! //  `mouse::translate_*`, `touch::translate_touch`, `stylus::translate_force`)
//! if let Some(p) = pad.as_mut() { p.poll(&mut events); }
//! for e in &events {
//!     keys.handle_event(e);
//!     mouse_buttons.handle_event(e);
//!     touches.handle_event(e);
//! }
//! ```
//!
//! ## Non-interference
//!
//! Per W13 §non-interference: this crate exposes pure-translation helpers
//! and standalone state structs. It does NOT register itself with a
//! kernel/events queue or kernel/ecs world — that wiring lives in
//! `runtime-desktop` / `editor-shell` (post-W13 integration). Keeps the
//! crate consumable from headless tooling and tests without dragging the
//! kernel-events transitive in.

pub mod event;
pub mod gamepad;
pub mod keyboard;
pub mod mouse;
pub mod state;
pub mod stylus;
pub mod touch;

// Re-exports: the most-used types floated to crate root for ergonomics.
pub use event::{GamepadAxis, GamepadButton, GamepadId, InputEvent, Pressed, TouchId, XrEvent};
pub use gamepad::{apply_dead_zone, GamepadPoller, DEFAULT_DEAD_ZONE};
pub use keyboard::{translate_keyboard, KeyCode};
pub use mouse::{
    translate_cursor_moved, translate_mouse_button, translate_mouse_wheel, MouseButton, ScrollDelta,
};
pub use state::{GamepadAxes, Input, TouchState};
pub use stylus::translate_force;
pub use touch::translate_touch;
