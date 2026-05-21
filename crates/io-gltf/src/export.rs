// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! glTF 2.0 export — `Scene` (resolved against a [`Cache`]) → `.glb` bytes.
//!
//! Output is a single-buffer GLB (binary glTF). The binary chunk packs every
//! mesh primitive's vertex / index data, then every skin's inverse-bind
//! matrices, then every animation sampler's keyframe times + values, in
//! that order. Buffer views and accessors point into the chunk by offset.
//!
//! ## GLB layout (glTF 2.0 §4.4)
//!
//! ```text
//! +--------+--------+--------+--------+
//! | magic  |version |length  |        |
//! | "glTF" |   2    | total  |        |
//! +--------+--------+--------+--------+
//! | json_len | "JSON"  |  json bytes  |
//! +----------+---------+--------------+
//! |  bin_len | "BIN\0" |  bin  bytes  |
//! +----------+---------+--------------+
//! ```
//!
//! Every chunk's payload is padded to a 4-byte multiple (JSON pads with `'
//! '`, BIN pads with `0x00`).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::animation::{AnimationClip, BoneChannel};
use crate::cache_stub::Cache;
use crate::handles::{MaterialHandle, MeshHandle, SkeletonHandle};
use crate::material::{AlphaMode, MaterialAsset};
use crate::mesh::MeshAsset;
use crate::scene_stub::{Entity, Scene};
use crate::skeleton::Skeleton;
use crate::GltfError;

// glTF 2.0 component-type spec codes.
const COMPONENT_FLOAT: u32 = 5126;
const COMPONENT_UNSIGNED_INT: u32 = 5125;

// glTF 2.0 buffer-view target hints.
const TARGET_ARRAY_BUFFER: u32 = 34962;
const TARGET_ELEMENT_ARRAY_BUFFER: u32 = 34963;

// GLB chunk type magic.
const CHUNK_TYPE_JSON: u32 = 0x4E4F_534A; // "JSON" little-endian
const CHUNK_TYPE_BIN: u32 = 0x004E_4942; // "BIN\0" little-endian
const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF" little-endian
const GLB_VERSION: u32 = 2;

/// Serialise a [`Scene`] (resolving handles against `cache`) to a `.glb` file.
pub fn export_glb_to_file(
    scene: &Scene,
    cache: &dyn Cache,
    path: impl AsRef<Path>,
) -> Result<(), GltfError> {
    let bytes = export_glb(scene, cache)?;
    let mut f = File::create(path.as_ref())?;
    f.write_all(&bytes)?;
    Ok(())
}

/// Serialise a [`Scene`] to a `.glb` byte vector.
pub fn export_glb(scene: &Scene, cache: &dyn Cache) -> Result<Vec<u8>, GltfError> {
    let mut bin = Vec::<u8>::new();
    let mut buffer_views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();

    // ---- collect unique meshes / materials / skeletons / animations ----
    // We walk the scene in deterministic entity order and remember which
    // handle we've already emitted, so the JSON arrays come out stable.

    let mut mesh_handle_to_idx: BTreeMap<MeshHandle, usize> = BTreeMap::new();
    let mut mat_handle_to_idx: BTreeMap<MaterialHandle, usize> = BTreeMap::new();
    let mut skel_handle_to_idx: BTreeMap<SkeletonHandle, usize> = BTreeMap::new();

    let mut meshes_json: Vec<Value> = Vec::new();
    let mut materials_json: Vec<Value> = Vec::new();
    let mut skins_json: Vec<Value> = Vec::new();

    for ec in &scene.entities {
        // Materials first so primitives can reference them.
        if let Some(mh) = ec.material {
            if let std::collections::btree_map::Entry::Vacant(slot) = mat_handle_to_idx.entry(mh) {
                let mat = cache.get_material(&mh).ok_or_else(|| {
                    GltfError::Schema(format!("export: material {} not in cache", mh.to_hex()))
                })?;
                let idx = materials_json.len();
                materials_json.push(emit_material_json(mat));
                slot.insert(idx);
            }
        }
    }

    for ec in &scene.entities {
        if let Some(mh) = ec.mesh {
            if let std::collections::btree_map::Entry::Vacant(slot) = mesh_handle_to_idx.entry(mh) {
                let asset = cache.get_mesh(&mh).ok_or_else(|| {
                    GltfError::Schema(format!("export: mesh {} not in cache", mh.to_hex()))
                })?;
                let mat_idx = ec.material.and_then(|m| mat_handle_to_idx.get(&m).copied());
                let mesh_json =
                    emit_mesh_json(asset, mat_idx, &mut bin, &mut buffer_views, &mut accessors);
                let idx = meshes_json.len();
                meshes_json.push(mesh_json);
                slot.insert(idx);
            }
        }
    }

    for ec in &scene.entities {
        if let Some(sh) = ec.skeleton {
            if let std::collections::btree_map::Entry::Vacant(slot) = skel_handle_to_idx.entry(sh) {
                let skel = cache.get_skeleton(&sh).ok_or_else(|| {
                    GltfError::Schema(format!("export: skeleton {} not in cache", sh.to_hex()))
                })?;
                let idx = skins_json.len();
                skins_json.push(emit_skin_json(
                    skel,
                    &mut bin,
                    &mut buffer_views,
                    &mut accessors,
                ));
                slot.insert(idx);
            }
        }
    }

    // ---- nodes ----
    let nodes_json: Vec<Value> = scene
        .entities
        .iter()
        .enumerate()
        .map(|(i, ec)| {
            let mut m = Map::new();
            if !ec.name.is_empty() {
                m.insert("name".into(), Value::String(ec.name.clone()));
            }
            // TRS — emit only non-default fields to keep output compact and
            // byte-stable across importer versions that default-fill.
            if ec.transform.translation != [0.0, 0.0, 0.0] {
                m.insert(
                    "translation".into(),
                    json!(ec.transform.translation.to_vec()),
                );
            }
            if ec.transform.rotation != [0.0, 0.0, 0.0, 1.0] {
                m.insert("rotation".into(), json!(ec.transform.rotation.to_vec()));
            }
            if ec.transform.scale != [1.0, 1.0, 1.0] {
                m.insert("scale".into(), json!(ec.transform.scale.to_vec()));
            }
            if let Some(mh) = ec.mesh {
                m.insert("mesh".into(), json!(mesh_handle_to_idx[&mh]));
            }
            if let Some(sh) = ec.skeleton {
                m.insert("skin".into(), json!(skel_handle_to_idx[&sh]));
            }
            // Children: any entity whose `parent` field == this entity.
            let children: Vec<usize> = scene
                .entities
                .iter()
                .enumerate()
                .filter_map(|(j, e)| {
                    if e.parent == Entity(i as u32) {
                        Some(j)
                    } else {
                        None
                    }
                })
                .collect();
            if !children.is_empty() {
                m.insert("children".into(), json!(children));
            }
            Value::Object(m)
        })
        .collect();

    // ---- scenes (single scene 0 with all root entities) ----
    let root_indices: Vec<usize> = scene
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, ec)| {
            if ec.parent == Entity::ROOT {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    // ---- animations ----
    let animations_json: Vec<Value> = scene
        .animations
        .iter()
        .map(|h| {
            let clip = cache.get_animation(h).ok_or_else(|| {
                GltfError::Schema(format!("export: animation {} not in cache", h.to_hex()))
            })?;
            Ok::<Value, GltfError>(emit_animation_json(
                clip,
                &mut bin,
                &mut buffer_views,
                &mut accessors,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // ---- top-level glTF JSON ----
    let mut top = Map::new();
    top.insert(
        "asset".into(),
        json!({"version": "2.0", "generator": "rge-io-gltf v0"}),
    );
    top.insert("scene".into(), json!(0));
    top.insert("scenes".into(), json!([{"nodes": root_indices}]));
    if !nodes_json.is_empty() {
        top.insert("nodes".into(), Value::Array(nodes_json));
    }
    if !meshes_json.is_empty() {
        top.insert("meshes".into(), Value::Array(meshes_json));
    }
    if !materials_json.is_empty() {
        top.insert("materials".into(), Value::Array(materials_json));
    }

    // Synthetic texture / sampler / image padding.
    //
    // TODO (Dispatch M+) — real texture export. Dispatch L added
    // `MaterialAsset::base_color_image_handle` and a cache surface for
    // decoded images, but this exporter still emits placeholder PNG
    // magic-byte URIs (`iVBORw0KGgo=`) to satisfy the glTF validator
    // on round-trip. Hooking the placeholder path to
    // `cache.get_image(handle)` + `rge_io_image::png::save_png`
    // requires (a) deciding whether to re-encode every export or
    // cache the encoded bytes alongside the decoded ones, and (b)
    // wiring sampler + texture JSON properly. Deferred to its own
    // dispatch so Dispatch L stays substrate-only and the import
    // path lands cleanly.
    //
    // v0 round-trips texture **indices** but doesn't serialise the texture
    // bitmaps themselves (those live in `io-image`, W18). The `gltf` crate's
    // validator enforces that every texture index resolves; if we leave the
    // arrays empty, materials with a `baseColorTexture.index = 5` fail
    // import. We pad with synthetic placeholder textures up to the maximum
    // referenced index so the document stays spec-valid through the round
    // trip. Texture bytes themselves are not preserved at v0 — that's
    // explicitly W18's job.
    if let Some(max_tex) = max_referenced_texture_index(cache, scene) {
        // Dispatch L — synthesise a real 1×1 white PNG via io-image so
        // the round-trip survives the new `extract_images` decode step
        // (the prior 8-byte PNG-magic-only placeholder was rejected by
        // `rge_io_image::load_bytes` as truncated). Single placeholder
        // shared across every texture slot — they're identical byte
        // patterns so importers dedupe via content hash anyway.
        let placeholder_uri = synthetic_placeholder_png_data_uri();
        let images: Vec<Value> = (0..=max_tex)
            .map(|i| json!({"uri": placeholder_uri.clone(), "name": format!("placeholder_{i}")}))
            .collect();
        let samplers: Vec<Value> = (0..=max_tex).map(|_| json!({})).collect();
        let textures: Vec<Value> = (0..=max_tex)
            .map(|i| json!({"source": i, "sampler": i}))
            .collect();
        top.insert("images".into(), Value::Array(images));
        top.insert("samplers".into(), Value::Array(samplers));
        top.insert("textures".into(), Value::Array(textures));
    }

    if !skins_json.is_empty() {
        top.insert("skins".into(), Value::Array(skins_json));
    }
    if !animations_json.is_empty() {
        top.insert("animations".into(), Value::Array(animations_json));
    }
    if !accessors.is_empty() {
        top.insert("accessors".into(), Value::Array(accessors));
    }
    if !buffer_views.is_empty() {
        top.insert("bufferViews".into(), Value::Array(buffer_views));
    }
    if !bin.is_empty() {
        top.insert("buffers".into(), json!([{"byteLength": bin.len()}]));
    }

    let json_str = serde_json::to_string(&Value::Object(top))?;
    Ok(pack_glb(json_str.as_bytes(), &bin))
}

/// Dispatch L — encode a 1×1 white PNG once per export and return it
/// as a `data:image/png;base64,...` URI. Replaces the prior 8-byte
/// PNG-magic-only placeholder so the round-trip survives the new
/// `extract_images` decode step (`rge_io_image::load_bytes` rejected
/// the truncated form). Cheap enough to recompute per export call —
/// the PNG is tiny (~70 bytes) and base64 doubles that.
///
/// Round-trip semantics unchanged: importers see a valid 1×1 image,
/// the cache stores it, the texture index resolves; the actual pixel
/// content is opaque white and is NOT a faithful re-encoding of any
/// originally-imported image. Real texture export is deferred to
/// Dispatch M+ (see TODO at the synthetic-padding block above).
fn synthetic_placeholder_png_data_uri() -> String {
    let img = rge_io_image::Image::from_rgba8(1, 1, vec![0xFF, 0xFF, 0xFF, 0xFF]);
    let png = rge_io_image::png::save_png(&img).expect("save_png 1×1 white");
    let mut b64 = String::with_capacity((png.len() + 2) / 3 * 4);
    base64_encode_into(&png, &mut b64);
    format!("data:image/png;base64,{b64}")
}

/// Minimal RFC-4648 base64 encoder. Inverse of
/// [`crate::import::base64_decode_exposed`]; same charset, same
/// padding rules. Local to export so the synthetic-placeholder path
/// doesn't pull a new dependency in.
fn base64_encode_into(bytes: &[u8], out: &mut String) {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut chunks = bytes.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
        out.push(CHARSET[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = u32::from(rem[0]) << 16;
            out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            out.push(CHARSET[((n >> 18) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 12) & 0x3F) as usize] as char);
            out.push(CHARSET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
}

/// Highest texture index referenced by any material in the cache that is
/// reachable from `scene`. Returns `None` when no material in the scene
/// uses any texture slot.
fn max_referenced_texture_index(cache: &dyn Cache, scene: &Scene) -> Option<usize> {
    let mut max: Option<usize> = None;
    for ec in &scene.entities {
        if let Some(mh) = ec.material {
            if let Some(mat) = cache.get_material(&mh) {
                for idx in [
                    mat.base_color_texture,
                    mat.normal_texture,
                    mat.metallic_roughness_texture,
                ]
                .into_iter()
                .flatten()
                {
                    max = Some(max.map_or(idx, |m| m.max(idx)));
                }
            }
        }
    }
    max
}

/// Append `data` to `bin`, zero-pad to 4-byte alignment, and emit a
/// bufferView pointing at the freshly-written slice. Returns the bufferView
/// index.
fn push_buffer_view(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    data: &[u8],
    target: Option<u32>,
) -> usize {
    let offset = bin.len();
    bin.extend_from_slice(data);
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let mut bv = Map::new();
    bv.insert("buffer".into(), json!(0));
    bv.insert("byteOffset".into(), json!(offset));
    bv.insert("byteLength".into(), json!(data.len()));
    if let Some(t) = target {
        bv.insert("target".into(), json!(t));
    }
    let idx = buffer_views.len();
    buffer_views.push(Value::Object(bv));
    idx
}

/// Append a vec3 array to the BIN chunk and emit the matching accessor.
/// `kind` is the glTF spec string ("VEC3", "VEC4", "SCALAR", ...).
fn push_accessor_vec3(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[[f32; 3]],
    target: Option<u32>,
    bounds: bool,
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 12);
    for v in data {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, target);

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_FLOAT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("VEC3"));
    if bounds {
        let (min, max) = component_min_max_3(data);
        acc.insert("min".into(), json!(min));
        acc.insert("max".into(), json!(max));
    }
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn push_accessor_vec2(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[[f32; 2]],
    target: Option<u32>,
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for v in data {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, target);

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_FLOAT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("VEC2"));
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn push_accessor_vec4(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[[f32; 4]],
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 16);
    for v in data {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, None);

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_FLOAT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("VEC4"));
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn push_accessor_mat4(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[[f32; 16]],
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 64);
    for v in data {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, None);

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_FLOAT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("MAT4"));
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn push_accessor_scalar_f32(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[f32],
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, None);

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_FLOAT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("SCALAR"));
    if !data.is_empty() {
        let mut mn = f32::INFINITY;
        let mut mx = f32::NEG_INFINITY;
        for v in data {
            if *v < mn {
                mn = *v;
            }
            if *v > mx {
                mx = *v;
            }
        }
        acc.insert("min".into(), json!([mn]));
        acc.insert("max".into(), json!([mx]));
    }
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn push_accessor_indices_u32(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    data: &[u32],
) -> usize {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let bv = push_buffer_view(bin, buffer_views, &bytes, Some(TARGET_ELEMENT_ARRAY_BUFFER));

    let mut acc = Map::new();
    acc.insert("bufferView".into(), json!(bv));
    acc.insert("componentType".into(), json!(COMPONENT_UNSIGNED_INT));
    acc.insert("count".into(), json!(data.len()));
    acc.insert("type".into(), json!("SCALAR"));
    let idx = accessors.len();
    accessors.push(Value::Object(acc));
    idx
}

fn component_min_max_3(data: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for v in data {
        for i in 0..3 {
            if v[i] < mn[i] {
                mn[i] = v[i];
            }
            if v[i] > mx[i] {
                mx[i] = v[i];
            }
        }
    }
    (mn, mx)
}

fn emit_mesh_json(
    asset: &MeshAsset,
    material_index: Option<usize>,
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> Value {
    let pos_acc = push_accessor_vec3(
        bin,
        buffer_views,
        accessors,
        &asset.positions,
        Some(TARGET_ARRAY_BUFFER),
        true,
    );
    let mut attrs = Map::new();
    attrs.insert("POSITION".into(), json!(pos_acc));
    if !asset.normals.is_empty() {
        let n_acc = push_accessor_vec3(
            bin,
            buffer_views,
            accessors,
            &asset.normals,
            Some(TARGET_ARRAY_BUFFER),
            false,
        );
        attrs.insert("NORMAL".into(), json!(n_acc));
    }
    if !asset.texcoords.is_empty() {
        let t_acc = push_accessor_vec2(
            bin,
            buffer_views,
            accessors,
            &asset.texcoords,
            Some(TARGET_ARRAY_BUFFER),
        );
        attrs.insert("TEXCOORD_0".into(), json!(t_acc));
    }
    let idx_acc = push_accessor_indices_u32(bin, buffer_views, accessors, &asset.indices);

    let mut prim = Map::new();
    prim.insert("attributes".into(), Value::Object(attrs));
    prim.insert("indices".into(), json!(idx_acc));
    if let Some(mi) = material_index {
        prim.insert("material".into(), json!(mi));
    }

    json!({
        "primitives": [Value::Object(prim)],
    })
}

fn emit_material_json(mat: &MaterialAsset) -> Value {
    let mut pbr = Map::new();
    pbr.insert("baseColorFactor".into(), json!(mat.base_color.to_vec()));
    pbr.insert("metallicFactor".into(), json!(mat.metallic));
    pbr.insert("roughnessFactor".into(), json!(mat.roughness));
    if let Some(t) = mat.base_color_texture {
        pbr.insert("baseColorTexture".into(), json!({"index": t}));
    }
    if let Some(t) = mat.metallic_roughness_texture {
        pbr.insert("metallicRoughnessTexture".into(), json!({"index": t}));
    }

    let mut m = Map::new();
    if !mat.name.is_empty() {
        m.insert("name".into(), Value::String(mat.name.clone()));
    }
    m.insert("pbrMetallicRoughness".into(), Value::Object(pbr));
    if let Some(t) = mat.normal_texture {
        m.insert("normalTexture".into(), json!({"index": t}));
    }
    if mat.emissive != [0.0, 0.0, 0.0] {
        m.insert("emissiveFactor".into(), json!(mat.emissive.to_vec()));
    }
    if mat.double_sided {
        m.insert("doubleSided".into(), json!(true));
    }
    if mat.alpha_mode != AlphaMode::Opaque {
        m.insert("alphaMode".into(), json!(mat.alpha_mode.as_gltf_str()));
        if mat.alpha_mode == AlphaMode::Mask {
            m.insert("alphaCutoff".into(), json!(mat.alpha_cutoff));
        }
    }
    Value::Object(m)
}

fn emit_skin_json(
    skel: &Skeleton,
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> Value {
    let mut m = Map::new();
    if !skel.name.is_empty() {
        m.insert("name".into(), Value::String(skel.name.clone()));
    }
    m.insert("joints".into(), json!(skel.joints));
    if let Some(r) = skel.root {
        m.insert("skeleton".into(), json!(r));
    }
    if !skel.inverse_bind_matrices.is_empty() {
        let acc = push_accessor_mat4(bin, buffer_views, accessors, &skel.inverse_bind_matrices);
        m.insert("inverseBindMatrices".into(), json!(acc));
    }
    Value::Object(m)
}

fn emit_animation_json(
    clip: &AnimationClip,
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> Value {
    let mut samplers_json = Vec::with_capacity(clip.samplers.len());
    let mut channels_json = Vec::with_capacity(clip.samplers.len());

    for (i, s) in clip.samplers.iter().enumerate() {
        let times_acc = push_accessor_scalar_f32(bin, buffer_views, accessors, &s.times);
        let out_acc = match &s.channel {
            BoneChannel::Translation(v) => {
                push_accessor_vec3(bin, buffer_views, accessors, v, None, false)
            }
            BoneChannel::Scale(v) => {
                push_accessor_vec3(bin, buffer_views, accessors, v, None, false)
            }
            BoneChannel::Rotation(v) => push_accessor_vec4(bin, buffer_views, accessors, v),
            BoneChannel::Weights(v) => push_accessor_scalar_f32(bin, buffer_views, accessors, v),
        };

        samplers_json.push(json!({
            "input": times_acc,
            "output": out_acc,
            "interpolation": s.interpolation.as_gltf_str(),
        }));
        channels_json.push(json!({
            "sampler": i,
            "target": {
                "node": s.target_node,
                "path": s.channel.as_path_str(),
            },
        }));
    }

    let mut m = Map::new();
    if !clip.name.is_empty() {
        m.insert("name".into(), Value::String(clip.name.clone()));
    }
    m.insert("samplers".into(), Value::Array(samplers_json));
    m.insert("channels".into(), Value::Array(channels_json));
    Value::Object(m)
}

/// Pack JSON + BIN chunks into a GLB byte vector.
fn pack_glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    // Pad the JSON chunk with spaces (0x20) so the BIN chunk header lands
    // 4-byte aligned.
    let mut json_padded = json.to_vec();
    while json_padded.len() % 4 != 0 {
        json_padded.push(b' ');
    }
    let mut bin_padded = bin.to_vec();
    while bin_padded.len() % 4 != 0 {
        bin_padded.push(0);
    }

    let mut out = Vec::with_capacity(12 + 8 + json_padded.len() + 8 + bin_padded.len());

    // GLB header.
    let total_len: u32 = (12
        + 8
        + json_padded.len()
        + if bin_padded.is_empty() {
            0
        } else {
            8 + bin_padded.len()
        }) as u32;
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&GLB_VERSION.to_le_bytes());
    out.extend_from_slice(&total_len.to_le_bytes());

    // JSON chunk.
    out.extend_from_slice(&(json_padded.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_TYPE_JSON.to_le_bytes());
    out.extend_from_slice(&json_padded);

    // BIN chunk (omitted when empty per glTF spec — JSON-only docs are
    // valid GLB).
    if !bin_padded.is_empty() {
        out.extend_from_slice(&(bin_padded.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_TYPE_BIN.to_le_bytes());
        out.extend_from_slice(&bin_padded);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_stub::MemoryCache;
    use crate::scene_stub::{Entity, EntityComponents, Transform};

    #[test]
    fn empty_scene_packs_into_valid_glb() {
        let scene = Scene::new();
        let cache = MemoryCache::new();
        let bytes = export_glb(&scene, &cache).expect("export");
        // Must be at least: header(12) + json chunk header(8) + min json
        assert!(bytes.len() >= 20);
        // Magic.
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, GLB_MAGIC);
    }

    #[test]
    fn glb_total_length_matches_actual_size() {
        let mut cache = MemoryCache::new();
        let mh = cache.insert_mesh(MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        });
        let mut scene = Scene::new();
        scene.spawn(EntityComponents {
            name: "tri".into(),
            transform: Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh),
            material: None,
            skeleton: None,
        });
        let bytes = export_glb(&scene, &cache).expect("export");
        let claimed = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(claimed as usize, bytes.len());
    }
}
