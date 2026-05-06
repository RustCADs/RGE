# Wave W03 — editor-shell PIE skeleton

> Self-contained agent dispatch. Phase 5 deliverable per IMPLEMENTATION.md. PIE validates editor/runtime unification.
> Cross-refs: PLAN.md §6.13 (PIE), §1.15 (editor-state); IMPLEMENTATION.md Phase 5.

## Goal

PIE state machine + ECS world snapshot/restore + Play/Stop/Pause/Step/FrameStep wiring. Validates that ECS snapshot/restore is correctness-preserving and that editor-state persists across the boundary.

## Crate owned

`crates/editor-shell`.

## Files this wave touches

```
crates/editor-shell/src/{lib.rs, play_state.rs, snapshot.rs, lifecycle.rs, play_toolbar.rs, time_scale.rs, viewport.rs}
crates/editor-shell/tests/{pie_round_trip.rs, snapshot_correctness.rs, time_scale_test.rs}
crates/editor-shell/tests/fixtures/scene_100_entities.ron
```

## Stubs needed

- `kernel/ecs::World` + `Snapshot` — assume W21-equivalent stub or use existing rustforge `kernel/ecs/`.
- `editor-state::Selection`, `ActiveTool` — local stub if W08+ not merged.
- `kernel/audit-ledger` — local stub if not yet implemented.

## Implementation order

1. `play_state.rs`: `enum PlayState { Editing, Playing, Paused }` + transition rules.
2. `snapshot.rs`: `WorldSnapshot` clone via ECS storage clone; `restore()` byte-identical.
3. `lifecycle.rs`: winit `ApplicationHandler` impl — adapt from `rustforge/apps/editor-app/src/app_lifecycle.rs`.
4. `play_toolbar.rs`: register Play/Pause/Stop/Step/FrameStep buttons in `editor.play_mode.toolbar` extension point (stub registration if W08 not merged).
5. `time_scale.rs`: 0.01–4.0× scale slider; `Time::delta_seconds()` scaled per game system; editor systems ignore scale.
6. `viewport.rs`: viewport widget skeleton (no rendering yet — display "Editing" or "Playing" text overlay).
7. Tests:
   - `pie_round_trip`: spawn 100 entities → Play → 60 ticks → Stop → byte-identical world.
   - `snapshot_correctness`: verify selection persists across Play/Stop (selection lives in editor-state, not in snapshot).

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/app_lifecycle.rs` | winit ApplicationHandler impl (direct precursor) | copy + adapt for PlayState transitions |
| `rustforge/apps/editor-app/src/app.rs` | EditorApp ECS lifecycle | adapt for editor-shell crate boundary |
| `rustforge/apps/editor-app/src/main.rs` | entry, panic hook, App startup | adapt for PIE-aware bootstrap (don't move main here — editor app owns main) |
| `rustforge/apps/editor-app/src/app_render.rs` | wgpu render pass setup | reference only; gfx wave (W21+) owns this |

Header pattern: `// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — PlayState transitions added`.

## Exit criteria

- Spawn entity in editing → press Play → run 60 ticks → press Stop → world byte-identical to pre-play state (CI gate).
- Selection persists across Play/Stop cycle.
- Time-scale slider affects game systems but not editor systems.
- `cargo test -p rge-editor-shell` passes.

## Duration estimate

2 days. **CRITICAL** — Phase 5 PIE gate. If PIE round-trip fails byte-identical, ECS storage layout needs redesign (per IMPLEMENTATION.md Phase 5 abort condition).

## Anti-pattern check

PASS — single PlayState state machine. Snapshot is ECS clone (one mechanism). Editor-state persists separately (per §1.15 coordination-not-authority rule).

## Handoff

After merge: W11 physics, W12 audio, etc. add `SnapshotParticipate` impls. Phase 5 integration wires `editor-state` selection persistence and verifies cross-subsystem snapshot correctness.
