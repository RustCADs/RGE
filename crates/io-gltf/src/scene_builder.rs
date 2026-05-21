// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Scene-builder: glue between extracted assets and the [`crate::Scene`] tree.
//!
//! Walks the glTF scene-node graph depth-first, spawning one [`Entity`] per
//! glTF node. For nodes that carry a mesh, expands multi-primitive meshes
//! into one entity per primitive (parented under the original node entity)
//! so each draw call gets its own (mesh, material) handle pair.
//!
//! Skin attachment: if a glTF node has a skin, the matching [`Skeleton`]
//! handle is attached to *every* primitive entity emitted from that node's
//! mesh.

use crate::animation::{extract_animations, AnimationClip};
use crate::cache_stub::Cache;
use crate::handles::{AnimationHandle, ImageHandle, MaterialHandle, MeshHandle, SkeletonHandle};
use crate::image::{extract_images, ImageAsset};
use crate::import::BufferData;
use crate::material::{extract_materials, MaterialAsset};
use crate::mesh::{extract_meshes, MeshAsset};
use crate::scene_stub::{Entity, EntityComponents, Scene, Transform};
use crate::skeleton::{extract_skeletons, Skeleton};
use crate::GltfError;

/// Drive the full import pipeline: extract assets, populate the cache, walk
/// the scene tree, return a populated [`Scene`].
///
/// `doc` is the parsed glTF document; `buffers` is the binary buffer set
/// (one per glTF buffer, typically a single buffer for `.glb`). Indexed by
/// glTF buffer index.
pub fn build_scene(
    doc: &gltf::Document,
    buffers: &[BufferData],
    cache: &mut dyn Cache,
) -> Result<Scene, GltfError> {
    // Step 1 — extract & cache all assets.
    let mesh_prims: Vec<Vec<MeshAsset>> = extract_meshes(doc, buffers)?;
    let materials: Vec<MaterialAsset> = extract_materials(doc);
    let skeletons: Vec<Skeleton> = extract_skeletons(doc, buffers)?;
    let animations: Vec<AnimationClip> = extract_animations(doc, buffers)?;

    // Dispatch L — decode every embedded glTF image and insert into
    // the cache. Build a `glTF image index -> ImageHandle` lookup so
    // material extraction can resolve `base_color_texture` through
    // `textures[i].source().index()` to an `ImageHandle`. Pure-glTF-
    // index entities (cube.glb / animated_character.glb today) have
    // no images and produce an empty handle vec, leaving every
    // material's `base_color_image_handle` as `None` (verified by
    // unit + integration tests).
    let images: Vec<ImageAsset> = extract_images(doc, buffers)?;
    let image_handles: Vec<ImageHandle> = images
        .into_iter()
        .map(|img| cache.insert_image(img))
        .collect();

    // glTF `textures[i]` maps to `images[image_handles[texture.source().
    // index()]]`. Pre-resolve the indirection so material extraction
    // is a single Option lookup per `base_color_texture` slot.
    let texture_index_to_image_handle: Vec<Option<ImageHandle>> = doc
        .textures()
        .map(|t| image_handles.get(t.source().index()).copied())
        .collect();

    // Populate `base_color_image_handle` on each material BEFORE
    // inserting into the cache (handle is part of the material's
    // identity for downstream consumers; but excluded from
    // `MaterialAsset::content_hash` since the image identity is
    // already content-addressed via the cache itself).
    let materials_with_image_handles: Vec<MaterialAsset> = materials
        .into_iter()
        .map(|mut m| {
            if let Some(tex_idx) = m.base_color_texture {
                if let Some(handle) = texture_index_to_image_handle
                    .get(tex_idx)
                    .copied()
                    .flatten()
                {
                    m.base_color_image_handle = Some(handle);
                }
            }
            m
        })
        .collect();

    let material_handles: Vec<MaterialHandle> = materials_with_image_handles
        .into_iter()
        .map(|m| cache.insert_material(m))
        .collect();

    let mesh_prim_handles: Vec<Vec<(MeshHandle, Option<MaterialHandle>)>> = mesh_prims
        .into_iter()
        .map(|prims| {
            prims
                .into_iter()
                .map(|p| {
                    let mat_idx = p.material_index;
                    let mh = cache.insert_mesh(p);
                    let mat = mat_idx.and_then(|i| material_handles.get(i).copied());
                    (mh, mat)
                })
                .collect()
        })
        .collect();

    let skeleton_handles: Vec<SkeletonHandle> = skeletons
        .into_iter()
        .map(|s| cache.insert_skeleton(s))
        .collect();

    let animation_handles: Vec<AnimationHandle> = animations
        .into_iter()
        .map(|a| cache.insert_animation(a))
        .collect();

    // Step 2 — walk the scene tree (use scene 0 if multiple, like most
    // viewers do). glTF allows zero scenes (asset-only file); we surface
    // those as an empty Scene.
    let mut scene = Scene {
        animations: animation_handles,
        ..Scene::default()
    };

    // Pre-allocate one Entity slot per glTF node so children can resolve
    // their parent's Entity index before we visit them. We then fill the
    // EntityComponents in-place during the walk.
    for _ in 0..doc.nodes().count() {
        scene.spawn(EntityComponents::default());
    }

    let active_scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .ok_or_else(|| GltfError::Schema("glTF has no scenes".into()))?;

    for root in active_scene.nodes() {
        visit_node(
            &root,
            Entity::ROOT,
            &mesh_prim_handles,
            &skeleton_handles,
            &mut scene,
        );
    }

    Ok(scene)
}

fn visit_node(
    node: &gltf::Node,
    parent: Entity,
    mesh_prim_handles: &[Vec<(MeshHandle, Option<MaterialHandle>)>],
    skeleton_handles: &[SkeletonHandle],
    scene: &mut Scene,
) {
    let node_entity = Entity(node.index() as u32);

    // Decompose the node's transform.
    let (translation, rotation, scale) = node.transform().decomposed();
    let transform = Transform {
        translation,
        rotation,
        scale,
    };

    // Pull the skin handle (if any).
    let skeleton = node.skin().map(|s| skeleton_handles[s.index()]);

    // If the node has a mesh, emit one entity per primitive. The parent of
    // every primitive entity is `node_entity` itself; the primary node
    // entity carries no mesh (just transform + name) so multi-primitive
    // meshes don't lose their geometric grouping.
    if let Some(mesh) = node.mesh() {
        let prims = &mesh_prim_handles[mesh.index()];
        if prims.len() == 1 {
            // Single-primitive — attach directly to the node entity.
            let (mh, mat) = prims[0];
            *scene
                .get_mut(node_entity)
                .expect("pre-allocated entity slot") = EntityComponents {
                name: node.name().unwrap_or("").to_string(),
                transform,
                parent,
                mesh: Some(mh),
                material: mat,
                skeleton,
            };
        } else {
            // Multi-primitive — node entity carries the transform; child
            // entities (one per primitive) carry mesh + material handles.
            *scene
                .get_mut(node_entity)
                .expect("pre-allocated entity slot") = EntityComponents {
                name: node.name().unwrap_or("").to_string(),
                transform,
                parent,
                mesh: None,
                material: None,
                skeleton: None,
            };
            for (i, (mh, mat)) in prims.iter().enumerate() {
                scene.spawn(EntityComponents {
                    name: format!("{}#prim{}", node.name().unwrap_or(""), i),
                    transform: Transform::IDENTITY,
                    parent: node_entity,
                    mesh: Some(*mh),
                    material: *mat,
                    skeleton,
                });
            }
        }
    } else {
        *scene
            .get_mut(node_entity)
            .expect("pre-allocated entity slot") = EntityComponents {
            name: node.name().unwrap_or("").to_string(),
            transform,
            parent,
            mesh: None,
            material: None,
            skeleton,
        };
    }

    for child in node.children() {
        visit_node(
            &child,
            node_entity,
            mesh_prim_handles,
            skeleton_handles,
            scene,
        );
    }
}

#[cfg(test)]
mod tests {
    // Scene-builder is exercised through the full import path in
    // `import.rs::tests` and the integration tests under `tests/`.
}
