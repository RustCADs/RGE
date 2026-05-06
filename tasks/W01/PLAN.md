# Wave W01 — components-* (11 sibling crates)

> Self-contained agent dispatch. Phase 2 deliverable per IMPLEMENTATION.md.
> Cross-refs: PLAN.md §1.3 Rule 1, §1.5.1 (entity roles); fileandfolderstructure.md §18.

## Goal

Seed the 11 cross-crate component crates with the canonical components used by 2+ subsystems. Components are state + handles + metadata + contracts only — no behavior, no orchestration.

## Crates owned by this wave

`crates/components-spatial`, `crates/components-identity`, `crates/components-visibility`, `crates/components-lifecycle`, `crates/components-interaction`, `crates/components-render`, `crates/components-physics`, `crates/components-animation`, `crates/components-audio`, `crates/components-networking`, `crates/components-editor`.

## Files this wave touches

```
crates/components-spatial/src/{lib.rs, transform.rs, parent.rs, child_of.rs, global_transform.rs}
crates/components-identity/src/{lib.rs, name.rs, asset_ref.rs, cad_ref.rs}
crates/components-visibility/src/{lib.rs, visibility.rs, hidden.rs, disabled.rs, highlight.rs}
crates/components-lifecycle/src/{lib.rs, spawn.rs, despawn.rs, age.rs}
crates/components-interaction/src/{lib.rs, trigger.rs, sensor.rs}
crates/components-render/src/{lib.rs, mesh_handle.rs, material_handle.rs, camera.rs, light.rs, brep_handle.rs, reflection_probe.rs, skinned_mesh.rs, lod.rs}
crates/components-physics/src/{lib.rs, rigid_body.rs, collider.rs, velocity.rs, angular_velocity.rs, joint.rs, mass.rs, character_controller.rs}
crates/components-animation/src/{lib.rs, skeleton.rs, bone_transforms.rs, animation_player.rs, animation_graph_instance.rs, ik_chain.rs, animation_event_listener.rs}
crates/components-audio/src/{lib.rs, audio_source.rs, audio_listener.rs, audio_falloff.rs}
crates/components-networking/src/{lib.rs, replicated.rs, network_owner.rs, authoritative.rs, remote_peer.rs, replication_policy.rs}
crates/components-editor/src/{lib.rs, editor_only_root.rs}
```

## Stubs needed

- `kernel/types::reflect` for `#[derive(Reflect)]` — assume W02 has shipped a stub trait.
- `kernel/asset::AssetId` — local stub if W14 not merged.

## Implementation order

1. `components-spatial`: Transform (Vec3 pos, Quat rot, Vec3 scale), Parent(Entity), ChildOf marker, GlobalTransform.
2. `components-identity`: Name(String), AssetRef(AssetId), CadRef(CadNodeId stub).
3. `components-visibility`: Visibility enum (Visible/Hidden/Inherited), Hidden marker, Disabled, Highlight.
4. `components-lifecycle`: Spawn/Despawn markers, Age tracking.
5. `components-interaction`: Trigger { on_enter, on_exit } marker, Sensor.
6. `components-render`: MeshHandle, MaterialHandle, Camera (Projection enum, viewport, priority), Light (Directional/Point/Spot variants), BRepHandle, ReflectionProbe, SkinnedMesh, LOD.
7. `components-physics`: RigidBody (Dynamic/Kinematic/Static), Collider (shape, friction, restitution), Velocity/AngularVelocity, Joint (kind, anchors), Mass/Inertia, CharacterController.
8. `components-animation`: Skeleton, BoneTransforms, AnimationPlayer, AnimationGraphInstance, IKChain, AnimationEventListener.
9. `components-audio`: AudioSource, AudioListener, AudioFalloff.
10. `components-networking`: marker components only — `Replicated`, `NetworkOwner(PeerId)`, `Authoritative`, `RemotePeer(PeerId)`, `ReplicationPolicy`. Zero-cost at v1.0.
11. `components-editor`: `EditorOnlyRoot` marker, gizmo markers.

Round-trip serde RON test on each component.

## Rustforge prior art (steal-and-adapt)

| Source | Relevance | Adaptation |
|---|---|---|
| `rustforge/crates/runtime-bvh/` | spatial accel structs | extract Transform-adjacent types |
| `rustforge/crates/runtime-color/` | color types | adapt for Light color, Material tint |
| `rustforge/crates/runtime-curves/` | animation curves | adapt for AnimationCurve |
| `rustforge/crates/runtime-pbr/` | PBR material types | adapt for MaterialHandle param shapes |
| `rustforge/crates/runtime-smartobject/` | object metadata | adapt for Name patterns |
| `rustforge/crates/runtime-statetree/` | state tree | adapt for hierarchy markers |
| `rustforge/crates/runtime-tags/` | tag types | adapt for Visibility/Highlight/marker patterns |
| `rustforge/crates/core/` | reflect IR types | check for Transform/Camera/Light reflections to steal |

Header on each adapted file: `// adapted from rustforge::<path> on 2026-05-05 — <what changed>`.

## Exit criteria

- 11 crates compile (`cargo check --workspace`).
- Each component has a round-trip serde RON test.
- One-major-type-per-file (Rule 3).
- No file exceeds 300 lines.
- Lib.rs in each crate re-exports public types only.

## Duration estimate

2 days at full focus.

## Anti-pattern check

PASS — components are state-only, no behavior. No new runtime / no sibling system.

## Handoff

After merge, components are consumed by W11 (physics needs RigidBody/Collider), W12 (audio needs AudioSource), W13 (input has its own state but uses Transform), W17 (io-gltf populates components), and W03 (editor-shell uses Transform/Camera/Light for PIE smoke test).
