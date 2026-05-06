# Wave W17 — io-gltf

> Self-contained agent dispatch. Phase 4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.6.4 (import authority); ADR-046.

## Goal

Adapt rustforge's `io-gltf` for the new asset-store interface. glTF 2.0 import + export. CI lint guarantees this is the only path for glTF.

## Crate owned

`crates/io-gltf`.

## Files this wave touches

```
crates/io-gltf/src/{lib.rs, import.rs, export.rs, scene_builder.rs, mesh.rs, material.rs, animation.rs, skeleton.rs}
crates/io-gltf/tests/{cube_round_trip.rs, animated_character_test.rs, pbr_material_test.rs}
crates/io-gltf/tests/fixtures/{cube.glb, animated_character.glb, pbr_material.glb}
```

## Stubs needed

- `asset-store::Cache` trait (W16) — local trait stub.
- `components-render::{MeshHandle, MaterialHandle}` (W01) — local stub.
- `components-spatial::{Transform, Parent, ChildOf}` (W01) — local stub.
- `components-animation::{Skeleton, BoneTransforms}` — local stub.
- `gltf` crate workspace dep.

## Implementation order

1. `import.rs` — `import_glb(path) -> Scene` (rge-data Scene): parses glTF, builds entity tree.
2. `mesh.rs` — extract meshes; produce MeshHandle (asset-store-cached).
3. `material.rs` — extract materials → MaterialHandle.
4. `animation.rs` — extract animation clips; produce AnimationClip assets.
5. `skeleton.rs` — extract skin / joints → Skeleton.
6. `scene_builder.rs` — orchestrate: traverse glTF nodes, spawn entities, set Transform, attach handles, build hierarchy via `parent_of` relations.
7. `export.rs` — reverse: ECS Scene → glTF. Used by editor "Export glTF" feature.
8. Test: import `cube.glb` → produces 1 mesh entity with Transform + MeshHandle + MaterialHandle.
9. Test: import + export + import → equivalent scene (mesh vertex count + material params + transforms match).
10. Test: animated_character.glb → produces Skeleton + BoneTransforms + AnimationClip.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/io-gltf/` | **direct precursor** | adapt to new asset-store::Cache interface; preserve test fixtures |
| `rustforge/crates/io-3mf/` | similar I/O crate structure | reference for module organization |
| `rustforge/crates/mesh-export/` | export-side mesh patterns | reference |

Header pattern: `// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait`.

## Exit criteria

- `cube.glb` round-trip: import → export → import produces equivalent scene (vertex count + materials + transforms within tolerance).
- `animated_character.glb` produces Skeleton + 1+ AnimationClip + 1+ skinned mesh.
- `pbr_material.glb` populates MaterialHandle with PBR params (baseColor, metallic, roughness, normal map ref).
- CI lint: no other crate imports `gltf` crate directly (one-import-path-per-format rule).
- `cargo test -p rge-io-gltf` passes.

## Duration estimate

2 days (much of the work is adapting existing rustforge code).

## Anti-pattern check

PASS — `io-gltf` is the only import path for glTF. CI lint enforces.

## Handoff

After merge: editor "File → Import glTF" uses this. Cook pipeline (post-W17) produces glTF re-exports for cooked builds (where applicable).
