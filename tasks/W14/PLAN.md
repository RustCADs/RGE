# Wave W14 — rge-data

> Self-contained agent dispatch. Phase 4 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.6 (file format discipline), §1.6.7 (versioning + migration).

## Goal

RON schemas for `.rge-project`, `.rge-scene`, `.rge-prefab`. Version field, migration registry, round-trip tests.

## Crate owned

`crates/rge-data`.

## Files this wave touches

```
crates/rge-data/src/{lib.rs, project.rs, scene.rs, prefab.rs, entity_ref.rs, asset_ref.rs, migration.rs, schema_version.rs}
crates/rge-data/tests/{round_trip.rs, migration_test.rs, schema_validation.rs}
crates/rge-data/tests/fixtures/{sample_project.rge-project, sample_scene.rge-scene, sample_prefab.rge-prefab, v0.0_to_v0.1_migration_input.rge-scene}
```

## Stubs needed

- `kernel/types::Reflect` (W02) for component serialization — local stub.
- `components-*` (W01) for component type definitions — local stubs.

## Implementation order

1. `entity_ref.rs` — `EntityId` ULID type; `Display` truncates to `e_<8 chars>` for debug.
2. `asset_ref.rs` — `AssetId(blake3:<hash>)` content-addressed.
3. `schema_version.rs` — `SchemaVersion(major: u8, minor: u8, patch: u8)`; serde + comparison.
4. `project.rs` — `Project { version, name, description, target_tiers, plugins, scenes, schema_version }`.
5. `scene.rs` — `Scene { version, name, entities: Vec<Entity>, root_entities: Vec<EntityId> }`. Entity = `{ id, name, components: Vec<ComponentValue>, relations: Vec<Relation> }`.
6. `prefab.rs` — `Prefab { version, name, parameters: Vec<ParamSpec>, entities, exposed_overrides }`.
7. `migration.rs` — registered migrations; `migrate(version_from, version_to, data)` walks the chain.
8. Test: parse vendored `sample_project.rge-project` + `sample_scene.rge-scene` + `sample_prefab.rge-prefab`.
9. Test: round-trip RON → struct → RON byte-identical.
10. Test: migration v0.0 → v0.1 lossless on fixture.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/apps/editor-app/src/ir_bridge.rs` | RON load/save (direct precursor) | adapt for project/scene/prefab |
| `rustforge/apps/editor-app/assets/*.ron` | RON IR format conventions | adapt formatting style |
| `rustforge/crates/persistence/` | persistence patterns | reference for migration registry |
| `rustforge/crates/release-manifest/` | versioning patterns | reference for SchemaVersion |

Header pattern: `// adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized for Project/Scene/Prefab`.

## Exit criteria

- Parse `sample_project.rge-project` + `sample_scene.rge-scene` + `sample_prefab.rge-prefab`.
- Round-trip RON byte-identically.
- Migration v0.0 → v0.1 lossless on fixture.
- EntityId ULID `Display` truncates to `e_abc12345` (8 hex chars) format.
- AssetId is content-stable (`blake3:<hash>` of source bytes).
- `cargo test -p rge-data` passes.

## Duration estimate

3 days.

## Anti-pattern check

PASS — RON only (single source-format family). Migration registry is the only versioning mechanism.

## Handoff

After merge: W15 pak-format references AssetId for asset blob lookup; W17 io-gltf populates Scene from imported glTF; editor app loads `.rge-project` at startup.
