// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Animation extraction.
//!
//! glTF animation = `clip { channels[] }`, where each channel is a triple
//! `(target node, target path ∈ {translation, rotation, scale, weights},
//! sampler)`, and each sampler is `(input keyframe-time accessor, output
//! value accessor, interpolation mode)`.
//!
//! v0 round-trips T/R/S channels with linear interpolation. Morph weights
//! are read but stored verbatim — playback support is downstream
//! (anim-clip / anim-graph waves).

use serde::{Deserialize, Serialize};

use crate::handles::AnimationHandle;
use crate::GltfError;

/// Interpolation mode for an animation sampler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    /// Step (no interpolation).
    Step,
    /// Linear interpolation.
    Linear,
    /// Cubic-spline interpolation (3 values per keyframe).
    CubicSpline,
}

impl Interpolation {
    fn from_gltf(m: gltf::animation::Interpolation) -> Self {
        match m {
            gltf::animation::Interpolation::Step => Self::Step,
            gltf::animation::Interpolation::Linear => Self::Linear,
            gltf::animation::Interpolation::CubicSpline => Self::CubicSpline,
        }
    }

    /// Spec string for export.
    pub(crate) fn as_gltf_str(self) -> &'static str {
        match self {
            Self::Step => "STEP",
            Self::Linear => "LINEAR",
            Self::CubicSpline => "CUBICSPLINE",
        }
    }
}

/// Property targeted by an animation channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BoneChannel {
    /// Translation channel — vec3 per keyframe.
    Translation(Vec<[f32; 3]>),
    /// Rotation channel — quaternion (xyzw) per keyframe.
    Rotation(Vec<[f32; 4]>),
    /// Scale channel — vec3 per keyframe.
    Scale(Vec<[f32; 3]>),
    /// Morph-target weights — `n_weights * keyframes` floats laid out one
    /// keyframe at a time.
    Weights(Vec<f32>),
}

impl BoneChannel {
    /// Property string for the glTF `target.path` field.
    pub(crate) fn as_path_str(&self) -> &'static str {
        match self {
            Self::Translation(_) => "translation",
            Self::Rotation(_) => "rotation",
            Self::Scale(_) => "scale",
            Self::Weights(_) => "weights",
        }
    }
}

/// One sampler / channel combo — keyframe times + per-keyframe values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationSampler {
    /// glTF node index this channel drives.
    pub target_node: usize,
    /// Keyframe times in seconds (sorted ascending per glTF spec).
    pub times: Vec<f32>,
    /// Per-keyframe values; the variant indicates which property.
    pub channel: BoneChannel,
    /// Interpolation mode.
    pub interpolation: Interpolation,
}

/// One animation clip — list of samplers driving one or more nodes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    /// Optional clip name.
    pub name: String,
    /// All channel samplers.
    pub samplers: Vec<AnimationSampler>,
}

impl AnimationClip {
    /// Maximum keyframe time across all samplers (= clip duration in
    /// seconds). Returns 0.0 for empty clips.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.samplers
            .iter()
            .filter_map(|s| s.times.last().copied())
            .fold(0.0_f32, f32::max)
    }

    /// Compute the content-hash handle.
    #[must_use]
    pub fn content_hash(&self) -> AnimationHandle {
        let mut h = blake3::Hasher::new();
        h.update(self.name.as_bytes());
        h.update(b"|");
        for s in &self.samplers {
            h.update(&(s.target_node as u64).to_le_bytes());
            h.update(s.interpolation.as_gltf_str().as_bytes());
            h.update(s.channel.as_path_str().as_bytes());
            for t in &s.times {
                h.update(&t.to_le_bytes());
            }
            match &s.channel {
                BoneChannel::Translation(v) | BoneChannel::Scale(v) => {
                    for c in v {
                        for x in c {
                            h.update(&x.to_le_bytes());
                        }
                    }
                }
                BoneChannel::Rotation(v) => {
                    for q in v {
                        for x in q {
                            h.update(&x.to_le_bytes());
                        }
                    }
                }
                BoneChannel::Weights(v) => {
                    for x in v {
                        h.update(&x.to_le_bytes());
                    }
                }
            }
            h.update(b"|");
        }
        AnimationHandle(*h.finalize().as_bytes())
    }
}

/// Walk every glTF animation, return them in document order.
pub fn extract_animations(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
) -> Result<Vec<AnimationClip>, GltfError> {
    let mut out = Vec::with_capacity(doc.animations().count());
    for anim in doc.animations() {
        let mut samplers = Vec::with_capacity(anim.channels().count());
        for ch in anim.channels() {
            let reader = ch.reader(|buf| buffers.get(buf.index()).map(Vec::as_slice));
            let times = reader
                .read_inputs()
                .ok_or_else(|| GltfError::Schema("animation channel missing inputs".into()))?
                .collect::<Vec<f32>>();
            let outputs = reader
                .read_outputs()
                .ok_or_else(|| GltfError::Schema("animation channel missing outputs".into()))?;
            let channel = match outputs {
                gltf::animation::util::ReadOutputs::Translations(it) => {
                    BoneChannel::Translation(it.collect())
                }
                gltf::animation::util::ReadOutputs::Rotations(rot) => {
                    BoneChannel::Rotation(rot.into_f32().collect())
                }
                gltf::animation::util::ReadOutputs::Scales(it) => BoneChannel::Scale(it.collect()),
                gltf::animation::util::ReadOutputs::MorphTargetWeights(w) => {
                    BoneChannel::Weights(w.into_f32().collect())
                }
            };
            samplers.push(AnimationSampler {
                target_node: ch.target().node().index(),
                times,
                channel,
                interpolation: Interpolation::from_gltf(ch.sampler().interpolation()),
            });
        }
        out.push(AnimationClip {
            name: anim.name().unwrap_or("").to_string(),
            samplers,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_clip_duration_zero() {
        let c = AnimationClip::default();
        assert_eq!(c.duration(), 0.0);
    }

    #[test]
    fn duration_picks_max_last_time() {
        let c = AnimationClip {
            name: "walk".into(),
            samplers: vec![
                AnimationSampler {
                    target_node: 0,
                    times: vec![0.0, 1.0, 2.0],
                    channel: BoneChannel::Translation(vec![[0.0; 3]; 3]),
                    interpolation: Interpolation::Linear,
                },
                AnimationSampler {
                    target_node: 1,
                    times: vec![0.0, 0.5, 1.5],
                    channel: BoneChannel::Rotation(vec![[0.0, 0.0, 0.0, 1.0]; 3]),
                    interpolation: Interpolation::Linear,
                },
            ],
        };
        assert_eq!(c.duration(), 2.0);
    }

    #[test]
    fn content_hash_distinguishes_clips() {
        let a = AnimationClip {
            name: "walk".into(),
            samplers: vec![],
        };
        let b = AnimationClip {
            name: "run".into(),
            samplers: vec![],
        };
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn channel_path_strings() {
        assert_eq!(
            BoneChannel::Translation(vec![]).as_path_str(),
            "translation"
        );
        assert_eq!(BoneChannel::Rotation(vec![]).as_path_str(), "rotation");
        assert_eq!(BoneChannel::Scale(vec![]).as_path_str(), "scale");
        assert_eq!(BoneChannel::Weights(vec![]).as_path_str(), "weights");
    }

    #[test]
    fn interpolation_strings() {
        assert_eq!(Interpolation::Linear.as_gltf_str(), "LINEAR");
        assert_eq!(Interpolation::Step.as_gltf_str(), "STEP");
        assert_eq!(Interpolation::CubicSpline.as_gltf_str(), "CUBICSPLINE");
    }
}
