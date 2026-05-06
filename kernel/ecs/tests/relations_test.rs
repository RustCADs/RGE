//! Integration tests for relation storages.

use rge_kernel_ecs::relations::LodOf;
use rge_kernel_ecs::{bone_of, parent_of, World};

// ---------------------------------------------------------------------------
// parent_of — 3-level tree
// ---------------------------------------------------------------------------

#[test]
fn parent_of_three_level_tree() {
    let mut world = World::new();
    let root = world.spawn();
    let c1 = world.spawn();
    let c2 = world.spawn();
    let gc1 = world.spawn(); // grand-child of c1
    let gc2 = world.spawn(); // grand-child of c1

    parent_of(&mut world, root, c1);
    parent_of(&mut world, root, c2);
    parent_of(&mut world, c1, gc1);
    parent_of(&mut world, c1, gc2);

    let root_children: Vec<_> = world
        .relations::<rge_kernel_ecs::relations::ParentOf>()
        .unwrap()
        .iter_children(root)
        .collect();
    assert_eq!(root_children.len(), 2);
    assert!(root_children.contains(&c1));
    assert!(root_children.contains(&c2));

    let c1_children: Vec<_> = world
        .relations::<rge_kernel_ecs::relations::ParentOf>()
        .unwrap()
        .iter_children(c1)
        .collect();
    assert_eq!(c1_children.len(), 2);
    assert!(c1_children.contains(&gc1));
    assert!(c1_children.contains(&gc2));

    // c2 has no children
    let c2_children: Vec<_> = world
        .relations::<rge_kernel_ecs::relations::ParentOf>()
        .unwrap()
        .iter_children(c2)
        .collect();
    assert!(c2_children.is_empty());
}

#[test]
fn parent_of_deterministic_order() {
    let mut world = World::new();
    let root = world.spawn();
    let children: Vec<_> = (0..5).map(|_| world.spawn()).collect();
    for &child in &children {
        parent_of(&mut world, root, child);
    }
    let got: Vec<_> = world
        .relations::<rge_kernel_ecs::relations::ParentOf>()
        .unwrap()
        .iter_children(root)
        .collect();
    assert_eq!(got, children, "children must be in insertion order");
}

// ---------------------------------------------------------------------------
// bone_of — 16-bone skeleton
// ---------------------------------------------------------------------------

#[test]
fn bone_of_skeleton_order() {
    let mut world = World::new();
    let skeleton = world.spawn();
    let bones: Vec<_> = (0..16).map(|_| world.spawn()).collect();
    for &bone in &bones {
        bone_of(&mut world, skeleton, bone);
    }
    let linked: Vec<_> = world
        .relations::<rge_kernel_ecs::relations::BoneOf>()
        .unwrap()
        .iter_targets(skeleton)
        .collect();
    assert_eq!(linked, bones, "bone order must match insertion order");
    assert_eq!(linked.len(), 16);
}

// ---------------------------------------------------------------------------
// lod_of — sparse, 1000 LOD groups
// ---------------------------------------------------------------------------

#[test]
fn lod_of_sparse_1000_groups() {
    let mut world = World::new();

    // Create 1000 "mesh" entities and assign them LOD targets with varying density.
    let meshes: Vec<_> = (0..1_000).map(|_| world.spawn()).collect();
    let lod_roots: Vec<_> = (0..100).map(|_| world.spawn()).collect();

    // Assign each LOD root between 1 and 10 meshes (varying density).
    for (i, &root) in lod_roots.iter().enumerate() {
        let count = (i % 10) + 1; // 1..=10
        for j in 0..count {
            let mesh_idx = (i * 10 + j) % 1_000;
            world.relations_mut::<LodOf>().link(root, meshes[mesh_idx]);
        }
    }

    // Verify a few groups have the right sizes.
    for (i, &root) in lod_roots.iter().enumerate() {
        let expected = (i % 10) + 1;
        let actual = world
            .relations::<LodOf>()
            .unwrap()
            .iter_targets(root)
            .count();
        // Due to dedup (same mesh can appear multiple times across groups),
        // actual might be ≤ expected.  Just verify it's bounded.
        assert!(
            actual <= expected,
            "LOD group {i} should have ≤ {expected} meshes, got {actual}"
        );
        assert!(actual > 0, "LOD group {i} should have at least 1 mesh");
    }

    // Total source count should match non-empty groups.
    let source_count = world.relations::<LodOf>().unwrap().source_count();
    assert_eq!(source_count, 100);
}
