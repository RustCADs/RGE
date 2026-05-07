// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Mesh extraction & cache insertion.
//!
//! glTF organises geometry as `mesh -> primitives[]` where each primitive is
//! one draw call (one POSITION accessor, optional NORMAL, `TEXCOORD_0`,
//! indices, material). Our [`MeshAsset`] is the simplest unit: a single
//! primitive's vertex / index data plus the originating glTF material index
//! (resolved to a [`crate::MaterialHandle`] in [`crate::scene_builder`]).
//!
//! Multi-primitive glTF meshes expand into N adjacent entities at scene-build
//! time — keeps the asset cache content-hash-friendly (each draw call is
//! independently de-duped).

use serde::{Deserialize, Serialize};

use crate::handles::MeshHandle;
use crate::GltfError;

/// One drawable primitive (vertex + index data + material slot).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshAsset {
    /// Vertex positions (model space, glTF Y-up convention).
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals; empty when the glTF primitive omitted NORMAL.
    pub normals: Vec<[f32; 3]>,
    /// Vertex UV0 coordinates; empty when the glTF primitive omitted
    /// `TEXCOORD_0`.
    pub texcoords: Vec<[f32; 2]>,
    /// Triangle indices (flat — every three values is one triangle).
    pub indices: Vec<u32>,
    /// glTF document-relative material index, or `None` when the primitive
    /// has no material assigned.
    pub material_index: Option<usize>,
}

impl MeshAsset {
    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles (= `indices.len() / 3`).
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Compute the content-hash handle for this mesh asset.
    ///
    /// Hashes the canonical byte form: positions / normals / texcoords /
    /// indices written in sequence as little-endian f32 / u32.
    /// Material index is *not* hashed — the same vertex buffer with two
    /// different materials should de-dupe to one mesh asset.
    #[must_use]
    pub fn content_hash(&self) -> MeshHandle {
        let mut hasher = blake3::Hasher::new();
        for v in &self.positions {
            for c in v {
                hasher.update(&c.to_le_bytes());
            }
        }
        hasher.update(b"|");
        for v in &self.normals {
            for c in v {
                hasher.update(&c.to_le_bytes());
            }
        }
        hasher.update(b"|");
        for v in &self.texcoords {
            for c in v {
                hasher.update(&c.to_le_bytes());
            }
        }
        hasher.update(b"|");
        for i in &self.indices {
            hasher.update(&i.to_le_bytes());
        }
        MeshHandle(*hasher.finalize().as_bytes())
    }
}

/// Helper alias used in scene-builder code to keep call-site clarity.
pub type Primitive = MeshAsset;

/// Walk every glTF mesh × primitive, allocate a [`MeshAsset`] per primitive,
/// and return them indexed by `(mesh_index, primitive_index)` so the
/// scene-builder can attach the right one to each node.
///
/// `buffers` is the glTF binary buffer set (typically one per `.glb`),
/// indexed by glTF buffer index.
pub fn extract_meshes(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
) -> Result<Vec<Vec<MeshAsset>>, GltfError> {
    let mut out = Vec::with_capacity(doc.meshes().count());

    for mesh in doc.meshes() {
        let mut prims = Vec::with_capacity(mesh.primitives().count());
        for primitive in mesh.primitives() {
            // Reject non-triangle topologies. v0 only handles indexed /
            // unindexed triangle lists; strips/fans/lines/points are
            // surfaced as a Schema error so the caller knows what bounced.
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(GltfError::Schema(format!(
                    "primitive mode {:?} not supported (only Triangles)",
                    primitive.mode()
                )));
            }

            let reader =
                primitive.reader(|buf| buffers.get(buf.index()).map(std::vec::Vec::as_slice));

            let positions = reader
                .read_positions()
                .ok_or_else(|| GltfError::Schema("primitive missing POSITION".into()))?
                .collect::<Vec<[f32; 3]>>();

            let normals = reader
                .read_normals()
                .map(std::iter::Iterator::collect::<Vec<[f32; 3]>>)
                .unwrap_or_default();

            let texcoords = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect::<Vec<[f32; 2]>>())
                .unwrap_or_default();

            // Indices: glTF allows missing indices, in which case vertices
            // are drawn sequentially (every 3 = 1 tri). We materialise the
            // implicit indices so downstream code can treat all meshes
            // uniformly.
            let indices = match reader.read_indices() {
                Some(it) => it.into_u32().collect::<Vec<u32>>(),
                None => (0u32..positions.len() as u32).collect(),
            };

            if indices.len() % 3 != 0 {
                return Err(GltfError::Schema(format!(
                    "triangle index count {} not a multiple of 3",
                    indices.len()
                )));
            }

            prims.push(MeshAsset {
                positions,
                normals,
                texcoords,
                indices,
                material_index: primitive.material().index(),
            });
        }
        out.push(prims);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_and_triangle_counts() {
        let m = MeshAsset {
            positions: vec![[0.0; 3]; 6],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_index: None,
        };
        assert_eq!(m.vertex_count(), 6);
        assert_eq!(m.triangle_count(), 2);
    }

    #[test]
    fn content_hash_is_deterministic() {
        let m = MeshAsset {
            positions: vec![[1.0, 2.0, 3.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            texcoords: vec![],
            indices: vec![0],
            material_index: Some(0),
        };
        assert_eq!(m.content_hash(), m.content_hash());
    }

    #[test]
    fn material_index_does_not_affect_hash() {
        let mut a = MeshAsset {
            positions: vec![[1.0, 2.0, 3.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0],
            material_index: Some(0),
        };
        let h1 = a.content_hash();
        a.material_index = Some(99);
        let h2 = a.content_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn distinct_geometry_distinct_hash() {
        let a = MeshAsset {
            positions: vec![[1.0, 0.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0],
            material_index: None,
        };
        let b = MeshAsset {
            positions: vec![[2.0, 0.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0],
            material_index: None,
        };
        assert_ne!(a.content_hash(), b.content_hash());
    }
}
