// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! glTF 2.0 import — top-level entry points.
//!
//! Two surfaces:
//! - [`import_glb`] — accept a path to a `.glb` file. Reads bytes, parses,
//!   calls [`crate::scene_builder::build_scene`], returns the populated
//!   [`Scene`]. Side-effects every asset insertion through the supplied
//!   [`Cache`].
//! - [`import_glb_bytes`] — same thing but takes the bytes directly. Useful
//!   for round-trip tests and for callers that already hold the buffer
//!   (HTTP downloads, embedded fixtures).
//!
//! ## Why we don't use `gltf::import_slice`
//!
//! The `gltf` crate's `import` feature transitively pulls `image` (texture
//! decode) which transitively pulls `cpufeatures 0.3.0`, which requires
//! `edition2024` (rustc 1.85+). The RGE workspace pins rustc 1.78. We do
//! our own buffer-data resolution against the GLB BIN chunk and base64
//! `data:` URIs — a small price to pay for staying inside the toolchain
//! pin and for keeping the dependency tree narrow (one-import-path-per-
//! format rule, PLAN §1.6.5).

use std::path::Path;

use crate::cache_stub::Cache;
use crate::scene_builder::build_scene;
use crate::scene_stub::Scene;
use crate::GltfError;

/// Buffer data backing a glTF document.
pub(crate) type BufferData = Vec<u8>;

/// Import a `.glb` file from disk. Returns the populated [`Scene`] and inserts
/// every referenced mesh / material / animation / skeleton asset into the
/// supplied [`Cache`].
pub fn import_glb(path: impl AsRef<Path>, cache: &mut dyn Cache) -> Result<Scene, GltfError> {
    let bytes = std::fs::read(path.as_ref())?;
    import_glb_bytes(&bytes, cache)
}

/// In-memory variant of [`import_glb`] — accepts the `.glb` bytes directly.
pub fn import_glb_bytes(bytes: &[u8], cache: &mut dyn Cache) -> Result<Scene, GltfError> {
    let gltf = gltf::Gltf::from_slice(bytes)?;
    let buffers = resolve_buffers(&gltf)?;
    build_scene(&gltf.document, &buffers, cache)
}

/// Resolve every buffer referenced by `gltf` to a `Vec<u8>`. Two sources
/// supported at v0:
///
/// 1. **GLB BIN chunk** — a buffer whose JSON has no `uri` field is the
///    binary chunk that follows the JSON chunk in a `.glb`. Per glTF 2.0
///    spec only the *first* buffer can be the BIN chunk.
/// 2. **`data:` URI** — base64-encoded inline buffer. We support the canonical
///    `data:application/octet-stream;base64,...` and `data:application/gltf-
///    buffer;base64,...` MIME types per glTF 2.0 §3.6.
///
/// External `.bin` sidecar URIs (a relative path to a separate file) are
/// rejected for v0 — the importer is wired for `.glb` only. M-wave can add
/// sidecar support behind a feature flag if needed.
fn resolve_buffers(gltf: &gltf::Gltf) -> Result<Vec<BufferData>, GltfError> {
    let mut out = Vec::with_capacity(gltf.document.buffers().count());
    for buffer in gltf.document.buffers() {
        let len = buffer.length();
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                let blob = gltf.blob.as_ref().ok_or_else(|| {
                    GltfError::Schema("buffer Source::Bin but GLB has no BIN chunk".into())
                })?;
                if blob.len() < len {
                    return Err(GltfError::Schema(format!(
                        "BIN chunk too short: declared {} but only {} bytes",
                        len,
                        blob.len()
                    )));
                }
                out.push(blob[..len].to_vec());
            }
            gltf::buffer::Source::Uri(uri) => {
                let data = decode_data_uri(uri)?;
                if data.len() < len {
                    return Err(GltfError::Schema(format!(
                        "data: URI shorter than declared byteLength ({} < {})",
                        data.len(),
                        len
                    )));
                }
                out.push(data);
            }
        }
    }
    Ok(out)
}

/// Strict subset of RFC-2397 — accept the two glTF-spec base64 prefixes only.
fn decode_data_uri(uri: &str) -> Result<Vec<u8>, GltfError> {
    const PREFIXES: &[&str] = &[
        "data:application/octet-stream;base64,",
        "data:application/gltf-buffer;base64,",
    ];
    for p in PREFIXES {
        if let Some(rest) = uri.strip_prefix(p) {
            return base64_decode(rest)
                .ok_or_else(|| GltfError::Schema("base64 decode failed in data URI".into()));
        }
    }
    Err(GltfError::Schema(format!(
        "unsupported buffer URI (only data:application/octet-stream;base64 and data:application/gltf-buffer;base64 supported): {uri}"
    )))
}

/// Minimal RFC-4648 base64 decoder. Used because adding the `base64` crate
/// to the workspace just for a single decode site would inflate the
/// import-path footprint.
///
/// Dispatch L exposes this through [`base64_decode_exposed`] so the
/// image-URI parser in [`crate::image`] can reuse the same routine
/// without duplicating it (image URIs and buffer URIs share the
/// base64 payload encoding).
pub(crate) fn base64_decode_exposed(s: &str) -> Option<Vec<u8>> {
    base64_decode(s)
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = s.as_bytes();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    let mut padding = 0;
    for &b in bytes {
        let v: u32 = match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a') + 26,
            b'0'..=b'9' => u32::from(b - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                bits += 6;
                if bits >= 8 {
                    bits -= 8;
                }
                continue;
            }
            // Whitespace inside data URIs is unusual but harmless to skip.
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        if padding > 0 {
            // Non-padding char after padding — corrupt.
            return None;
        }
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_stub::MemoryCache;
    use crate::export::export_glb;

    /// Tiny GLB built procedurally — single triangle, one material, one
    /// scene-node. Used to validate the import path without disk fixtures.
    fn synthetic_triangle_glb() -> Vec<u8> {
        let mut cache = MemoryCache::new();
        let mesh = crate::mesh::MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: Some(0),
        };
        let mat = crate::material::MaterialAsset {
            name: "tri-mat".into(),
            base_color: [0.8, 0.7, 0.6, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            ..Default::default()
        };
        let mh = cache.insert_mesh(mesh);
        let mat_h = cache.insert_material(mat);
        let mut scene = crate::scene_stub::Scene::new();
        scene.spawn(crate::scene_stub::EntityComponents {
            name: "root".into(),
            transform: crate::scene_stub::Transform::IDENTITY,
            parent: crate::scene_stub::Entity::ROOT,
            mesh: Some(mh),
            material: Some(mat_h),
            skeleton: None,
        });
        export_glb(&scene, &cache).expect("export")
    }

    #[test]
    fn import_synthetic_triangle_round_trips() {
        let glb = synthetic_triangle_glb();
        let mut cache = MemoryCache::new();
        let scene = import_glb_bytes(&glb, &mut cache).expect("import");
        assert_eq!(scene.entities.len(), 1);
        assert!(scene.entities[0].mesh.is_some());
        assert!(scene.entities[0].material.is_some());
        assert_eq!(cache.mesh_count(), 1);
        assert_eq!(cache.material_count(), 1);
    }

    #[test]
    fn base64_decode_basic() {
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(base64_decode("").unwrap(), b"");
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("***!").is_none());
    }

    #[test]
    fn data_uri_accepts_both_prefixes() {
        let octet = "data:application/octet-stream;base64,Zm9v";
        let gltf_buf = "data:application/gltf-buffer;base64,Zm9v";
        assert_eq!(decode_data_uri(octet).unwrap(), b"foo");
        assert_eq!(decode_data_uri(gltf_buf).unwrap(), b"foo");
    }

    #[test]
    fn data_uri_rejects_external() {
        assert!(decode_data_uri("file.bin").is_err());
        assert!(decode_data_uri("https://x.bin").is_err());
    }
}
