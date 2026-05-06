//! Animated-character import — exit criterion #2.
//!
//! `animated_character.glb` (procedural; see `common/mod.rs`) imports to a
//! [`Scene`] containing **at least**:
//!
//! - one [`rge_io_gltf::Skeleton`] cached (joint count > 0, IBM count
//!   matches),
//! - one [`rge_io_gltf::AnimationClip`] cached (samplers > 0,
//!   duration > 0),
//! - one entity with both a mesh handle and a skeleton handle (the skinned
//!   mesh).

mod common;

use rge_io_gltf::{import_glb, Cache, MemoryCache};

#[test]
fn animated_character_produces_skeleton_and_clip() {
    let path = common::animated_character_fixture_path();
    let mut cache = MemoryCache::new();
    let scene = import_glb(&path, &mut cache).expect("import animated character");

    // At least one skeleton in the cache.
    assert!(
        cache.skeleton_count() >= 1,
        "expected ≥1 skeleton, got {}",
        cache.skeleton_count()
    );

    // At least one animation clip (cache + scene-attached).
    assert!(
        cache.animation_count() >= 1,
        "expected ≥1 animation clip, got {}",
        cache.animation_count()
    );
    assert!(
        !scene.animations.is_empty(),
        "scene should reference the animation clip"
    );

    // Skinned mesh: at least one entity carries both mesh and skeleton.
    let skinned: Vec<_> = scene
        .entities
        .iter()
        .filter(|e| e.mesh.is_some() && e.skeleton.is_some())
        .collect();
    assert!(
        !skinned.is_empty(),
        "expected at least one skinned mesh entity"
    );

    // Skeleton joint count > 0 and IBM count matches.
    let skel_handle = skinned[0].skeleton.expect("skeleton handle");
    let skel = cache.get_skeleton(&skel_handle).expect("skeleton in cache");
    assert!(skel.joint_count() > 0);
    assert_eq!(
        skel.inverse_bind_matrices.len(),
        skel.joint_count(),
        "IBM count must match joint count"
    );

    // Animation duration > 0 and at least one TRS sampler.
    let anim_h = scene.animations[0];
    let clip = cache.get_animation(&anim_h).expect("anim in cache");
    assert!(
        clip.duration() > 0.0,
        "animation duration should be > 0, got {}",
        clip.duration()
    );
    assert!(!clip.samplers.is_empty());

    // At least one channel must be a Translation, Rotation, or Scale (per
    // the W17 spec — TRS round-trip is the v0 baseline).
    let has_trs = clip.samplers.iter().any(|s| {
        matches!(
            s.channel,
            rge_io_gltf::BoneChannel::Translation(_)
                | rge_io_gltf::BoneChannel::Rotation(_)
                | rge_io_gltf::BoneChannel::Scale(_)
        )
    });
    assert!(has_trs, "expected at least one TRS sampler");
}

#[test]
fn animated_character_round_trips_animation() {
    // Round-trip the animation clip: import, re-export, import — duration
    // and channel kinds must be preserved.
    let path = common::animated_character_fixture_path();

    let mut cache_a = MemoryCache::new();
    let scene_a = import_glb(&path, &mut cache_a).expect("first import");
    let bytes = rge_io_gltf::export_glb(&scene_a, &cache_a).expect("re-export");

    let mut cache_b = MemoryCache::new();
    let scene_b = rge_io_gltf::import_glb_bytes(&bytes, &mut cache_b).expect("second import");

    let clip_a = cache_a
        .get_animation(&scene_a.animations[0])
        .expect("clip a");
    let clip_b = cache_b
        .get_animation(&scene_b.animations[0])
        .expect("clip b");

    assert!((clip_a.duration() - clip_b.duration()).abs() < 1e-5);
    assert_eq!(clip_a.samplers.len(), clip_b.samplers.len());
    for (sa, sb) in clip_a.samplers.iter().zip(&clip_b.samplers) {
        assert_eq!(sa.times.len(), sb.times.len());
        // Channel discriminant must match.
        assert_eq!(
            std::mem::discriminant(&sa.channel),
            std::mem::discriminant(&sb.channel),
        );
    }
}
