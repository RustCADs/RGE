# Wave W12 — audio

> Self-contained agent dispatch. Phase 5+ deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §6 (subsystem map).

## Goal

Kira wrap, ECS-integrated AudioSource component, schedule stage, basic mixer.

## Crate owned

`crates/audio`.

## Files this wave touches

```
crates/audio/src/{lib.rs, manager.rs, source.rs, listener.rs, schedule.rs, falloff.rs}
crates/audio/tests/{playback_test.rs, mix_test.rs, distance_falloff_test.rs}
```

## Stubs needed

- `components-audio` (W01) — `AudioSource`, `AudioListener`, `AudioFalloff`. Local stub if not merged.
- `components-spatial::Transform` — for distance attenuation.
- `kira` workspace dep.

## Implementation order

1. `manager.rs` — Kira `AudioManager` resource; one per ECS World.
2. `source.rs` — `AudioSource` component impl; play/pause/stop; loop; volume; pitch.
3. `listener.rs` — `AudioListener` (typically on Camera entity); position + orientation drives spatial mix.
4. `falloff.rs` — `AudioFalloff` curve types (Linear, Logarithmic, InverseSquare, Custom).
5. `schedule.rs` — per-frame mixer update; sync Transform → Kira spatial position.
6. Test: play 1-second sine wave; verify amplitude matches expected at known sample positions.
7. Test: AudioSource at distance N has reduced volume per falloff curve.
8. Test: multiple sources mix without clipping.

## Rustforge prior art (steal-and-adapt)

(none for audio specifically — rustforge has no audio crate). Greenfield with Kira.

## Exit criteria

- Play 1-second sine wave at 440Hz; first 100 samples match reference within 1% tolerance.
- AudioSource at 10m distance with InverseSquare falloff = 1/100 amplitude vs at 1m.
- 8 simultaneous sources mix without clipping.
- `cargo test -p rge-audio` passes.

## Duration estimate

2 days.

## Anti-pattern check

PASS — Kira only (no rodio sibling). Single audio engine.

## Handoff

After merge: W03 PIE snapshot saves/restores audio playhead positions via `SnapshotParticipate` (post-integration). Triggers from W11 can fire AudioSource start.
