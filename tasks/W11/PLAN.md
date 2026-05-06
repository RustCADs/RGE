# Wave W11 — physics

> Self-contained agent dispatch. Phase 5+ deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6.10 (physics); §1.6.8 (determinism modes).

## Goal

Rapier3D wrap, ECS components, schedule stages, falling-cube smoke test, deterministic replay validation.

## Crate owned

`crates/physics`. Sibling `crates/physics-debug` is opt-in editor-only (gizmos, wireframes — out of scope for this wave; stub only).

## Files this wave touches

```
crates/physics/src/{lib.rs, world.rs, sync.rs, step.rs, events.rs, character.rs, joint.rs}
crates/physics/tests/{falling_cube.rs, deterministic_replay.rs, character_controller.rs}
```

## Stubs needed

- `components-physics` (W01) — `RigidBody`, `Collider`, `Velocity`, `Joint`, `CharacterController`. Local stubs if W01 not merged.
- `kernel/events` for collision events — local stub.
- `kernel/audit-ledger` for replay recording — local stub.
- `rapier3d` workspace dep — pinned for determinism.

## Implementation order

1. `world.rs` — Rapier `World` resource; one per ECS World; deterministic broadphase + parallel solver.
2. `sync.rs` — bidirectional sync: ECS Transform ↔ Rapier RigidBody position; component change detection drives sync direction.
3. `step.rs` — `physics_step` system: fixed timestep (60Hz); records inputs (forces, impulses) to audit-ledger.
4. `events.rs` — Rapier contact events → `kernel/events` typed channels (`CollisionStarted`, `CollisionEnded`, `TriggerEntered`, `TriggerExited`).
5. `character.rs` — `CharacterController` kinematic capsule; slope_limit, step_offset.
6. `joint.rs` — `Joint` kinds (Revolute, Prismatic, Spherical, Fixed); ECS components map to Rapier joints.
7. Schedule stages: `pre_physics` → `physics_step` → `post_physics` → `contact_events`.
8. Test: falling cube — `RigidBody::Dynamic` + Collider falls, lands on plane, comes to rest.
9. Test: deterministic replay — record 1000 ticks, replay, byte-identical world state.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/runtime-bvh/` | spatial accel data types | reference only — Rapier owns its own broadphase |
| (none for physics specifically) | rustforge has no physics crate | greenfield with `rapier3d` |

Mostly greenfield. Pin `rapier3d` version for determinism.

## Exit criteria

- Cube with `RigidBody::Dynamic` falls, lands on plane, comes to rest within 60 ticks.
- Deterministic over 1000-tick replay (byte-identical world state on re-run, same machine).
- Trigger event fires on collision; reaches script handler within <16ms (1 frame).
- `cargo test -p rge-physics` passes.

## Duration estimate

3 days.

## Anti-pattern check

PASS — Rapier only (no Bullet, no PhysX). Determinism via fixed timestep + pinned version. Same-platform Replay-Stable; cross-platform Lockstep-Stable deferred to Phase 5-Scale.

## Handoff

After merge: W03 PIE snapshot includes Rapier internal state via `SnapshotParticipate` impl (post-integration). W12 audio doesn't depend on this. Triggers fan to W04+W19+W20 script-host integration.
