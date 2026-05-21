// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Material extraction & cache insertion.
//!
//! v0 supports glTF 2.0 `pbrMetallicRoughness` (the most-used profile) plus a
//! pointer to the optional normal-map texture. Extension materials
//! (`KHR_materials_clearcoat` etc.) are NOT round-tripped at v0; they come back
//! as their `pbrMetallicRoughness` fallback. The glTF crate's
//! `material.pbr_metallic_roughness()` is always present (default-spec'd) so
//! v0 doesn't error on missing-pbrMR materials — it picks up the spec
//! defaults of `[1,1,1,1]` / `1.0` / `1.0`.

use serde::{Deserialize, Serialize};

use crate::handles::{ImageHandle, MaterialHandle};

/// PBR material parameters round-tripped at v0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialAsset {
    /// Optional human-readable material name.
    pub name: String,
    /// Linear-space base colour `[r, g, b, a]`.
    pub base_color: [f32; 4],
    /// Metallic factor (0..1).
    pub metallic: f32,
    /// Roughness factor (0..1).
    pub roughness: f32,
    /// Optional document-relative texture index for the base-colour map.
    pub base_color_texture: Option<usize>,
    /// Optional document-relative texture index for the normal map.
    pub normal_texture: Option<usize>,
    /// Optional document-relative texture index for the metallic-roughness
    /// map (RGB channel layout per glTF spec).
    pub metallic_roughness_texture: Option<usize>,
    /// Emissive factor.
    pub emissive: [f32; 3],
    /// Whether the material is double-sided.
    pub double_sided: bool,
    /// Alpha mode (Opaque / Mask / Blend).
    pub alpha_mode: AlphaMode,
    /// Alpha cutoff (used only with [`AlphaMode::Mask`]).
    pub alpha_cutoff: f32,
    /// Dispatch L — content-hash handle to the decoded base-colour
    /// image, populated by [`crate::scene_builder::build_scene`] when
    /// `base_color_texture` is `Some(i)` and the resolved
    /// `textures[i] -> images[j]` image successfully decodes through
    /// [`crate::extract_images`]. `None` when no `base_color_texture`
    /// was set or the resolution chain failed at scene-build time.
    ///
    /// Additive surface: the existing `base_color_texture` index field
    /// is preserved verbatim for round-trip and downstream callers
    /// that haven't migrated to handle-based lookup. Not hashed in
    /// [`Self::content_hash`] — handle identity is derivable from the
    /// image bytes via the cache.
    #[serde(default)]
    pub base_color_image_handle: Option<ImageHandle>,
}

impl Default for MaterialAsset {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 1.0,
            roughness: 1.0,
            base_color_texture: None,
            normal_texture: None,
            metallic_roughness_texture: None,
            emissive: [0.0, 0.0, 0.0],
            double_sided: false,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            base_color_image_handle: None,
        }
    }
}

/// glTF alpha-blending mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode {
    /// Fully opaque.
    Opaque,
    /// 1-bit mask via alpha cutoff.
    Mask,
    /// Standard alpha-blend.
    Blend,
}

impl AlphaMode {
    /// Convert from the `gltf` crate's enum.
    fn from_gltf(m: gltf::material::AlphaMode) -> Self {
        match m {
            gltf::material::AlphaMode::Opaque => Self::Opaque,
            gltf::material::AlphaMode::Mask => Self::Mask,
            gltf::material::AlphaMode::Blend => Self::Blend,
        }
    }

    /// glTF spec string for export.
    pub(crate) fn as_gltf_str(self) -> &'static str {
        match self {
            Self::Opaque => "OPAQUE",
            Self::Mask => "MASK",
            Self::Blend => "BLEND",
        }
    }
}

impl MaterialAsset {
    /// Compute the content-hash handle. Hashes every PBR parameter and
    /// texture-index slot in document order; the name is included so two
    /// materials with identical numeric params but different names dedupe
    /// only when they truly match (caller can strip names if intentional).
    #[must_use]
    pub fn content_hash(&self) -> MaterialHandle {
        let mut h = blake3::Hasher::new();
        h.update(self.name.as_bytes());
        h.update(b"|");
        for c in self.base_color {
            h.update(&c.to_le_bytes());
        }
        h.update(&self.metallic.to_le_bytes());
        h.update(&self.roughness.to_le_bytes());
        for c in self.emissive {
            h.update(&c.to_le_bytes());
        }
        h.update(&[u8::from(self.double_sided)]);
        h.update(self.alpha_mode.as_gltf_str().as_bytes());
        h.update(&self.alpha_cutoff.to_le_bytes());
        h.update(&(self.base_color_texture.unwrap_or(usize::MAX) as u64).to_le_bytes());
        h.update(&(self.normal_texture.unwrap_or(usize::MAX) as u64).to_le_bytes());
        h.update(&(self.metallic_roughness_texture.unwrap_or(usize::MAX) as u64).to_le_bytes());
        MaterialHandle(*h.finalize().as_bytes())
    }
}

/// Walk every glTF material, return them in document order.
pub fn extract_materials(doc: &gltf::Document) -> Vec<MaterialAsset> {
    doc.materials().map(extract_one).collect()
}

fn extract_one(m: gltf::Material) -> MaterialAsset {
    let pbr = m.pbr_metallic_roughness();
    let base_color = pbr.base_color_factor();
    let bc_tex = pbr.base_color_texture().map(|t| t.texture().index());
    let mr_tex = pbr
        .metallic_roughness_texture()
        .map(|t| t.texture().index());
    let normal_tex = m.normal_texture().map(|t| t.texture().index());

    MaterialAsset {
        name: m.name().unwrap_or("").to_string(),
        base_color,
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        base_color_texture: bc_tex,
        normal_texture: normal_tex,
        metallic_roughness_texture: mr_tex,
        emissive: m.emissive_factor(),
        double_sided: m.double_sided(),
        alpha_mode: AlphaMode::from_gltf(m.alpha_mode()),
        alpha_cutoff: m.alpha_cutoff().unwrap_or(0.5),
        // Dispatch L — populated post-extraction by
        // `scene_builder::build_scene` after images are decoded and
        // texture indices resolved. Default `None` is correct here.
        base_color_image_handle: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_white_unit_metal() {
        let m = MaterialAsset::default();
        assert_eq!(m.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(m.metallic, 1.0);
        assert_eq!(m.roughness, 1.0);
        assert_eq!(m.alpha_mode, AlphaMode::Opaque);
    }

    #[test]
    fn content_hash_is_stable() {
        let m = MaterialAsset {
            name: "brass".into(),
            base_color: [0.8, 0.7, 0.2, 1.0],
            metallic: 1.0,
            roughness: 0.3,
            ..Default::default()
        };
        assert_eq!(m.content_hash(), m.content_hash());
    }

    #[test]
    fn distinct_metallic_distinct_hash() {
        let mut a = MaterialAsset::default();
        let h1 = a.content_hash();
        a.metallic = 0.0;
        let h2 = a.content_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn alpha_mode_strings_round_trip_via_str() {
        assert_eq!(AlphaMode::Opaque.as_gltf_str(), "OPAQUE");
        assert_eq!(AlphaMode::Mask.as_gltf_str(), "MASK");
        assert_eq!(AlphaMode::Blend.as_gltf_str(), "BLEND");
    }
}
