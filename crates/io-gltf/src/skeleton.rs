// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Skeleton / skin extraction.
//!
//! glTF skin = `(joints[], inverseBindMatrices accessor, skeleton root)`. Our
//! [`Skeleton`] flattens that to a list of joints (each glTF node index) plus
//! the per-joint 4×4 inverse-bind matrix in column-major order — the same
//! layout the eventual `components-animation::Skeleton` will accept.

use serde::{Deserialize, Serialize};

use crate::handles::SkeletonHandle;
use crate::GltfError;

/// Skinning data for one mesh.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Skeleton {
    /// Optional skeleton name (= glTF skin name).
    pub name: String,
    /// glTF node indices of the joints, in skin order. Index `i` here is
    /// the bone-id used by `JOINTS_0` vertex attribute on the skinned mesh.
    pub joints: Vec<usize>,
    /// 4×4 inverse-bind matrices, column-major, one per joint. Empty when
    /// the glTF skin omitted them — caller treats absent as identity per
    /// glTF spec.
    pub inverse_bind_matrices: Vec<[f32; 16]>,
    /// Optional explicit skeleton-root node index.
    pub root: Option<usize>,
}

impl Skeleton {
    /// Number of joints.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// Compute the content-hash handle.
    #[must_use]
    pub fn content_hash(&self) -> SkeletonHandle {
        let mut h = blake3::Hasher::new();
        h.update(self.name.as_bytes());
        h.update(b"|");
        for j in &self.joints {
            h.update(&(*j as u64).to_le_bytes());
        }
        h.update(b"|");
        for m in &self.inverse_bind_matrices {
            for c in m {
                h.update(&c.to_le_bytes());
            }
        }
        h.update(b"|");
        h.update(&(self.root.unwrap_or(usize::MAX) as u64).to_le_bytes());
        SkeletonHandle(*h.finalize().as_bytes())
    }
}

/// Walk every glTF skin, return them in document order.
pub fn extract_skeletons(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
) -> Result<Vec<Skeleton>, GltfError> {
    let mut out = Vec::with_capacity(doc.skins().count());
    for skin in doc.skins() {
        let reader = skin.reader(|buf| buffers.get(buf.index()).map(Vec::as_slice));
        let joints = skin.joints().map(|n| n.index()).collect::<Vec<usize>>();
        // glTF spec stores inverse-bind matrices as 4×4 column-major; the
        // `gltf` crate yields them as `[[f32; 4]; 4]`. We flatten to a
        // 16-float row-array (still column-major in memory) so downstream
        // ECS code can treat the data as a contiguous slice.
        let inverse_bind_matrices = reader
            .read_inverse_bind_matrices()
            .map(|it| {
                it.map(|m| {
                    let mut flat = [0.0_f32; 16];
                    for col in 0..4 {
                        for row in 0..4 {
                            flat[col * 4 + row] = m[col][row];
                        }
                    }
                    flat
                })
                .collect::<Vec<[f32; 16]>>()
            })
            .unwrap_or_default();
        if !inverse_bind_matrices.is_empty() && inverse_bind_matrices.len() != joints.len() {
            return Err(GltfError::Schema(format!(
                "skin: {} joints but {} inverse-bind matrices",
                joints.len(),
                inverse_bind_matrices.len()
            )));
        }
        out.push(Skeleton {
            name: skin.name().unwrap_or("").to_string(),
            joints,
            inverse_bind_matrices,
            root: skin.skeleton().map(|n| n.index()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_skeleton() {
        let s = Skeleton::default();
        assert_eq!(s.joint_count(), 0);
    }

    #[test]
    fn content_hash_stable() {
        let s = Skeleton {
            name: "char1".into(),
            joints: vec![1, 2, 3],
            inverse_bind_matrices: vec![[0.0; 16]; 3],
            root: Some(0),
        };
        assert_eq!(s.content_hash(), s.content_hash());
    }

    #[test]
    fn distinct_joint_lists_distinct_hash() {
        let a = Skeleton {
            joints: vec![1, 2],
            ..Default::default()
        };
        let b = Skeleton {
            joints: vec![1, 3],
            ..Default::default()
        };
        assert_ne!(a.content_hash(), b.content_hash());
    }
}
