# Wave W13 — input

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §3 (multi-device input).

## Goal

winit + gilrs fan-in, unified ECS event stream across mouse/keyboard/gamepad/touch/stylus. XR slot reserved (Phase 5+).

## Crate owned

`crates/input`. Sibling `crates/input-gestures` (touch/stylus gesture recognizer) stub only — out of scope for W13.

## Files this wave touches

```
crates/input/src/{lib.rs, keyboard.rs, mouse.rs, gamepad.rs, touch.rs, stylus.rs, event.rs, state.rs}
crates/input/tests/{state_transitions.rs, gamepad_dead_zone.rs, touch_multi_finger.rs}
```

## Stubs needed

- `kernel/events` for unified event stream.
- `winit::Event` translation in caller scope.
- `gilrs` workspace dep.

## Implementation order

1. `event.rs` — unified `InputEvent` enum: `KeyDown(KeyCode)`, `KeyUp(KeyCode)`, `MouseMove(Vec2)`, `MouseButton(MouseButton, Pressed)`, `Scroll(Vec2)`, `GamepadButton(GamepadId, GamepadButton, Pressed)`, `GamepadAxis(GamepadId, GamepadAxis, f32)`, `TouchStart(TouchId, Vec2)`, `TouchMove(TouchId, Vec2)`, `TouchEnd(TouchId)`, `StylusPressure(f32)`, etc.
2. `keyboard.rs` — winit `KeyboardInput` → `InputEvent::KeyDown/Up`.
3. `mouse.rs` — winit cursor + button + wheel events.
4. `gamepad.rs` — gilrs poll → events; dead-zone normalization.
5. `touch.rs` — winit Touch events; multi-finger ID tracking.
6. `stylus.rs` — winit Pointer pressure events; stylus-only path.
7. `state.rs` — `Input<KeyCode>`, `Input<MouseButton>`, etc. ECS resources for "is currently pressed" queries.
8. Test: keyboard press → release transitions; state matches expectation.
9. Test: gamepad axis dead-zone correctly suppresses |x| < 0.1.
10. Test: touch multi-finger: 3 fingers tracked independently with stable IDs.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/app_lifecycle.rs` | winit `WindowEvent` handling (precursor) | extract input-event translation pattern |

Header pattern: `// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — input fan-in extracted`.

## Exit criteria

- Keyboard press/release transitions tracked correctly.
- Gamepad dead-zone (default 0.1) normalizes correctly.
- Touch multi-finger (≥3 fingers) tracked with stable IDs.
- XR slot reserved (`InputEvent::Xr` variant exists; payloads stub).
- `cargo test -p rge-input` passes.

## Duration estimate

2 days.

## Anti-pattern check

PASS — single unified input event stream. Per-platform translation hidden behind `cfg(target_os)` in `runtime-platform-*` (separate crates, post-W13).

## Handoff

After merge: W03 editor-shell consumes Input resource for viewport navigation. Game-runtime scripts subscribe via WIT `rge:ecs/observer` (post-W04+W19).
