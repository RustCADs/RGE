//! Shared fixture generation for the io-gltf integration tests.
//!
//! Fixtures are built procedurally and persisted to `tests/fixtures/` on
//! first run (the directory is checked into the repo, but the bytes are
//! generated rather than hand-edited so a regen is one `cargo test` away).
//! Each fixture lives in a deterministic byte form because
//! [`rge_io_gltf::export_glb`] serialises in entity / handle order.

// `tests/common/mod.rs` compiles once per integration-test binary; each
// binary uses only a subset of these helpers, so unused-fn warnings would
// fire spuriously. Silence them at the module level — they're real "this
// helper isn't used by *this* test bin" and not actual dead code.
#![allow(dead_code, unreachable_pub)]

use std::path::PathBuf;

use rge_io_gltf::{
    AnimationClip, AnimationSampler, BoneChannel, Cache, Entity, EntityComponents, MaterialAsset,
    MemoryCache, MeshAsset, Scene, Skeleton, Transform,
};

/// Project-relative path to the fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut p = PathBuf::from(manifest);
    p.push("tests");
    p.push("fixtures");
    p
}

/// Materialise the cube fixture if not already on disk; return its path.
pub fn cube_fixture_path() -> PathBuf {
    let mut p = fixtures_dir();
    std::fs::create_dir_all(&p).expect("create fixtures dir");
    p.push("cube.glb");
    if !p.exists() {
        let bytes = make_cube_glb();
        std::fs::write(&p, bytes).expect("write cube.glb");
    }
    p
}

/// Materialise the animated-character fixture if not already on disk.
pub fn animated_character_fixture_path() -> PathBuf {
    let mut p = fixtures_dir();
    std::fs::create_dir_all(&p).expect("create fixtures dir");
    p.push("animated_character.glb");
    if !p.exists() {
        let bytes = make_animated_character_glb();
        std::fs::write(&p, bytes).expect("write animated_character.glb");
    }
    p
}

/// Materialise the PBR-material fixture if not already on disk.
pub fn pbr_material_fixture_path() -> PathBuf {
    let mut p = fixtures_dir();
    std::fs::create_dir_all(&p).expect("create fixtures dir");
    p.push("pbr_material.glb");
    if !p.exists() {
        let bytes = make_pbr_material_glb();
        std::fs::write(&p, bytes).expect("write pbr_material.glb");
    }
    p
}

/// Build a unit-cube GLB: 8 vertices, 12 triangles, 1 material.
pub fn make_cube_glb() -> Vec<u8> {
    let mut cache = MemoryCache::new();
    let mesh = cube_mesh();
    let mat = MaterialAsset {
        name: "cube-mat".into(),
        base_color: [0.4, 0.6, 0.8, 1.0],
        metallic: 0.1,
        roughness: 0.7,
        ..Default::default()
    };
    let mh = cache.insert_mesh(mesh);
    let mat_h = cache.insert_material(mat);

    let mut scene = Scene::new();
    scene.spawn(EntityComponents {
        name: "cube".into(),
        transform: Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        parent: Entity::ROOT,
        mesh: Some(mh),
        material: Some(mat_h),
        skeleton: None,
    });

    rge_io_gltf::export_glb(&scene, &cache).expect("export cube")
}

/// Build the animated-character GLB: a 2-bone skeleton + 1 skinned mesh +
/// 1 animation clip with a translation + rotation channel on the root bone.
pub fn make_animated_character_glb() -> Vec<u8> {
    let mut cache = MemoryCache::new();

    // Skinned mesh (just a tetrahedron).
    let mesh = MeshAsset {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        normals: vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        texcoords: vec![],
        indices: vec![0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2],
        material_index: Some(0),
    };
    let material = MaterialAsset {
        name: "skin-mat".into(),
        base_color: [1.0, 0.85, 0.7, 1.0],
        metallic: 0.0,
        roughness: 0.5,
        ..Default::default()
    };

    // 2-joint skeleton — joints reference scene-node indices 1 (root bone)
    // and 2 (child bone). Identity inverse-bind so re-import sees the
    // matrix-count matching joint-count rule pass.
    let skeleton = Skeleton {
        name: "char-skel".into(),
        joints: vec![1, 2],
        inverse_bind_matrices: vec![
            [
                1.0, 0.0, 0.0, 0.0, // col 0
                0.0, 1.0, 0.0, 0.0, // col 1
                0.0, 0.0, 1.0, 0.0, // col 2
                0.0, 0.0, 0.0, 1.0, // col 3
            ];
            2
        ],
        root: Some(1),
    };

    let animation = AnimationClip {
        name: "walk".into(),
        samplers: vec![
            AnimationSampler {
                target_node: 1,
                times: vec![0.0, 0.5, 1.0],
                channel: BoneChannel::Translation(vec![
                    [0.0, 0.0, 0.0],
                    [0.0, 0.5, 0.0],
                    [0.0, 0.0, 0.0],
                ]),
                interpolation: rge_io_gltf::animation::Interpolation::Linear,
            },
            AnimationSampler {
                target_node: 1,
                times: vec![0.0, 1.0],
                channel: BoneChannel::Rotation(vec![
                    [0.0, 0.0, 0.0, 1.0],
                    [0.0, 0.7071068, 0.0, 0.7071068],
                ]),
                interpolation: rge_io_gltf::animation::Interpolation::Linear,
            },
        ],
    };

    let mh = cache.insert_mesh(mesh);
    let mat_h = cache.insert_material(material);
    let sk_h = cache.insert_skeleton(skeleton);
    let an_h = cache.insert_animation(animation);

    let mut scene = Scene::new();
    // Entity 0 — armature root with skinned mesh.
    scene.spawn(EntityComponents {
        name: "armature".into(),
        transform: Transform::IDENTITY,
        parent: Entity::ROOT,
        mesh: Some(mh),
        material: Some(mat_h),
        skeleton: Some(sk_h),
    });
    // Entity 1 — root bone.
    scene.spawn(EntityComponents {
        name: "root_bone".into(),
        transform: Transform::IDENTITY,
        parent: Entity(0),
        mesh: None,
        material: None,
        skeleton: None,
    });
    // Entity 2 — child bone.
    scene.spawn(EntityComponents {
        name: "child_bone".into(),
        transform: Transform::from_xyz(0.0, 1.0, 0.0),
        parent: Entity(1),
        mesh: None,
        material: None,
        skeleton: None,
    });
    scene.animations.push(an_h);

    rge_io_gltf::export_glb(&scene, &cache).expect("export animated character")
}

/// Build the PBR-material GLB: a cube + a material exercising all the PBR
/// parameter slots and the two non-default texture-index slots so the
/// importer round-trips them.
pub fn make_pbr_material_glb() -> Vec<u8> {
    let mut cache = MemoryCache::new();
    let mesh = cube_mesh();
    let mat = MaterialAsset {
        name: "pbr-spec".into(),
        base_color: [0.97, 0.86, 0.32, 1.0],
        metallic: 1.0,
        roughness: 0.18,
        // Texture indices here are dangling references into a hypothetical
        // textures[] table that we don't actually serialise (v0 doesn't ship
        // textures yet) — the importer still round-trips the indices.
        base_color_texture: Some(0),
        normal_texture: Some(1),
        metallic_roughness_texture: Some(2),
        emissive: [0.05, 0.0, 0.0],
        double_sided: true,
        alpha_mode: rge_io_gltf::material::AlphaMode::Mask,
        alpha_cutoff: 0.4,
    };
    let mh = cache.insert_mesh(mesh);
    let mat_h = cache.insert_material(mat);

    let mut scene = Scene::new();
    scene.spawn(EntityComponents {
        name: "pbr-cube".into(),
        transform: Transform::IDENTITY,
        parent: Entity::ROOT,
        mesh: Some(mh),
        material: Some(mat_h),
        skeleton: None,
    });

    rge_io_gltf::export_glb(&scene, &cache).expect("export pbr material")
}

/// Procedural unit cube. Per-face split so each triangle gets a flat normal
/// (normals match the face orientation and don't share verts across faces).
fn cube_mesh() -> MeshAsset {
    // Six faces × 4 vertices each = 24 verts. Two triangles per face = 12 tris.
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X face
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        // -X
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        // +Y
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        // -Y
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        // +Z
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        // -Z
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];

    for (n, verts) in &faces {
        let base = positions.len() as u32;
        for v in verts {
            positions.push(*v);
            normals.push(*n);
        }
        // Two triangles per quad.
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshAsset {
        positions,
        normals,
        texcoords: vec![],
        indices,
        material_index: Some(0),
    }
}

// Local helper extension — `Transform::from_xyz` doesn't exist on the W17
// stub, so we re-implement here.
trait TransformXYZ {
    fn from_xyz(x: f32, y: f32, z: f32) -> Self;
}
impl TransformXYZ for Transform {
    fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: [x, y, z],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}
