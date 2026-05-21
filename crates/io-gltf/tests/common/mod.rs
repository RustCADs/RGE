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

/// Dispatch L — materialise the textured-cube fixture if not already
/// on disk; return its path.
pub fn textured_cube_fixture_path() -> PathBuf {
    let mut p = fixtures_dir();
    std::fs::create_dir_all(&p).expect("create fixtures dir");
    p.push("textured_cube.glb");
    if !p.exists() {
        let bytes = make_textured_cube_glb();
        std::fs::write(&p, bytes).expect("write textured_cube.glb");
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
                    #[allow(
                        clippy::unreadable_literal,
                        clippy::approx_constant,
                        reason = "fixture value is the canonical 7-digit single-precision encoding of cos(45deg) used in the glTF reference quaternion-rotation example; deliberately matches the value expected in fixture bytes byte-for-byte rather than computed via FRAC_1_SQRT_2"
                    )]
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

/// Dispatch L — build a textured-cube GLB with a REAL 4×4 PNG
/// checkerboard embedded in the BIN chunk via a buffer-view image
/// source. Distinct from [`make_pbr_material_glb`] (which round-trips
/// material parameters with placeholder image URIs that don't decode)
/// — this fixture's image bytes are produced by
/// [`rge_io_image::png::save_png`] and survive a real
/// [`rge_io_image::load_bytes`] decode round-trip.
///
/// The fixture intentionally carries NO mesh geometry (empty scene
/// nodes list). Dispatch L scope is image extraction only —
/// rendering wiring (Dispatch M) will pair the image with a real
/// drawable cube. Keeping the fixture small makes the bin chunk
/// easy to reason about: it holds ONLY the encoded PNG bytes.
pub fn make_textured_cube_glb() -> Vec<u8> {
    let png_bytes = make_checker_4x4_png();
    let png_len = png_bytes.len();

    // Hand-rolled JSON: minimal glTF document with one image sourced
    // from `bufferViews[0]` (offset 0, length = PNG byte count). We
    // bypass `export_glb` because the production exporter still emits
    // placeholder PNG-magic URIs (Dispatch M deferral, see export.rs
    // TODO) — for THIS fixture we need the importer's `View` path
    // exercised against real bytes.
    let json = serde_json::json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [] }],
        "materials": [{
            "name": "checker-mat",
            "pbrMetallicRoughness": {
                "baseColorTexture": { "index": 0 }
            }
        }],
        "textures": [{ "source": 0 }],
        "images": [{ "bufferView": 0, "mimeType": "image/png", "name": "checker" }],
        "buffers": [{ "byteLength": png_len }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": png_len }]
    });

    let mut json_padded = serde_json::to_vec(&json).expect("serialize json");
    while json_padded.len() % 4 != 0 {
        json_padded.push(b' ');
    }
    let mut bin_padded = png_bytes;
    while bin_padded.len() % 4 != 0 {
        bin_padded.push(0);
    }

    let json_chunk_len = u32::try_from(json_padded.len()).expect("json chunk fits u32");
    let bin_chunk_len = u32::try_from(bin_padded.len()).expect("bin chunk fits u32");
    let total_len_usize = 12 + 8 + json_padded.len() + 8 + bin_padded.len();
    let total_len = u32::try_from(total_len_usize).expect("total fits u32");

    let mut out = Vec::with_capacity(total_len_usize);
    // GLB header.
    out.extend_from_slice(&0x4654_6C67_u32.to_le_bytes()); // "glTF" magic
    out.extend_from_slice(&2_u32.to_le_bytes()); // version 2
    out.extend_from_slice(&total_len.to_le_bytes());
    // JSON chunk.
    out.extend_from_slice(&json_chunk_len.to_le_bytes());
    out.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&json_padded);
    // BIN chunk.
    out.extend_from_slice(&bin_chunk_len.to_le_bytes());
    out.extend_from_slice(&0x004E_4942_u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&bin_padded);

    out
}

/// Build a 4×4 red/blue checkerboard PNG via the production io-image
/// codec. Sixteen pixels = 64 bytes RGBA8; output is a real PNG that
/// `rge_io_image::load_bytes` decodes back to the same RGBA layout.
///
/// Pixel layout (row-major from top):
///   row 0: R B R B
///   row 1: B R B R
///   row 2: R B R B
///   row 3: B R B R
fn make_checker_4x4_png() -> Vec<u8> {
    let red: [u8; 4] = [255, 0, 0, 255];
    let blue: [u8; 4] = [0, 0, 255, 255];
    let mut rgba = Vec::with_capacity(64);
    for y in 0..4 {
        for x in 0..4 {
            let on = ((x + y) % 2) == 0;
            rgba.extend_from_slice(if on { &red } else { &blue });
        }
    }
    let img = rge_io_image::Image::from_rgba8(4, 4, rgba);
    rge_io_image::png::save_png(&img).expect("save_png")
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
        base_color_image_handle: None,
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

/// Materialise the UV-cube fixture if not already on disk.
pub fn uv_cube_fixture_path() -> PathBuf {
    let mut p = fixtures_dir();
    std::fs::create_dir_all(&p).expect("create fixtures dir");
    p.push("uv_cube.glb");
    if !p.exists() {
        let bytes = make_uv_cube_glb();
        std::fs::write(&p, bytes).expect("write uv_cube.glb");
    }
    p
}

/// Dispatch M1 — build a UV-mapped cube GLB. Identical geometry to
/// [`make_cube_glb`] (24 verts, 12 tris, 6 faces) but each face's
/// 4 vertices carry the canonical `(0,0) → (1,0) → (1,1) → (0,1)`
/// UV unwrap. No texture image — M1 is the UV-substrate dispatch;
/// M2 will pair this geometry with an embedded image.
///
/// Material has `base_color_texture: None` and a distinguishable
/// magenta `base_color` so the dispatch-K base_color path stays
/// observable when launching the editor against this fixture.
pub fn make_uv_cube_glb() -> Vec<u8> {
    let mut cache = MemoryCache::new();
    let mesh = uv_cube_mesh();
    let mat = MaterialAsset {
        name: "uv-cube-mat".into(),
        base_color: [0.9, 0.4, 0.9, 1.0],
        ..Default::default()
    };
    let mh = cache.insert_mesh(mesh);
    let mat_h = cache.insert_material(mat);

    let mut scene = Scene::new();
    scene.spawn(EntityComponents {
        name: "uv_cube".into(),
        transform: Transform::IDENTITY,
        parent: Entity::ROOT,
        mesh: Some(mh),
        material: Some(mat_h),
        skeleton: None,
    });

    rge_io_gltf::export_glb(&scene, &cache).expect("export uv cube")
}

/// Cube mesh with per-face UV unwrap. Mirrors [`cube_mesh`]'s
/// per-face split shape (24 verts, 12 tris) but adds a
/// `texcoords: Vec<[f32; 2]>` populated with the canonical
/// `(0,0) → (1,0) → (1,1) → (0,1)` quad mapping for every face.
fn uv_cube_mesh() -> MeshAsset {
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut texcoords = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // Same face layout as `cube_mesh`. Indices reference the just-
    // pushed 4 vertices via `base + 0..3`.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
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

    // Canonical per-face UV unwrap: the 4 verts of each face span the
    // unit square in order (0,0)(1,0)(1,1)(0,1) — top-left to
    // bottom-left going clockwise viewed from outside the cube.
    let face_uvs: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    for (n, verts) in &faces {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "fixture cube has 6 quads × 4 verts = 24 positions max — well under u32::MAX"
        )]
        let base = positions.len() as u32;
        for (v, uv) in verts.iter().zip(face_uvs.iter()) {
            positions.push(*v);
            normals.push(*n);
            texcoords.push(*uv);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshAsset {
        positions,
        normals,
        texcoords,
        indices,
        material_index: Some(0),
    }
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
        #[allow(
            clippy::cast_possible_truncation,
            reason = "fixture cube has 6 quads × 4 verts = 24 positions max — well under u32::MAX, no truncation possible"
        )]
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
