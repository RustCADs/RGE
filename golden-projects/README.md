# RGE Golden Projects

Canonical regression-validation projects. Run through CI on every major change.

| Project | Tests |
|---|---|
| `simple-scene/` | basic load, transform, camera + light render |
| `material-zoo/` | 10+ materials covering PBR / unlit / skinned / blend-shape / B-Rep tessellated |
| `skinned-character/` | glTF import, skeleton, anim, skinning |
| `physics-puzzle/` | rigid bodies, joints, triggers, character controller, deterministic replay |
| `cad-parametric/` | B-Rep edits, lineage, projection invalidation (Phase 7 validation) |
| `stress-world/` | 50k+ entities, scene-streaming, perf regression detection (Phase 9) |

Exit criteria (per [PLAN.md §13](../PLAN.md)): all 6 load, run 60 ticks, screenshot match within tolerance, byte-identical cook output.

Per [WAVES.md W21](../WAVES.md). Each project is excluded from default workspace builds; opted in via dedicated CI workflows in `tools/ci/`.
