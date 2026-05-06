//! Bake the canonical W14 fixtures to `tests/fixtures/`.
//!
//! Run with `cargo run --example bake_fixtures` from the workspace root.
//! Idempotent: if the on-disk fixtures already match what this binary
//! would emit, nothing changes. Used to regenerate fixtures whenever the
//! pretty-print config or a struct shape changes.
//!
//! Mirrors the shape of `tests/round_trip.rs`'s canonical builders — the
//! two stay in lockstep deliberately.

use std::fs;
use std::path::Path;

use rge_data::{
    ComponentValue, Entity, EntityId, ExposedOverride, ParamSpec, PluginRef, Prefab, Project,
    Relation, Scene, ScenePath, SchemaVersion, TargetTier,
};

fn pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::new()
        .depth_limit(64)
        .new_line("\n".to_string())
        .indentor("    ".to_string())
        .struct_names(true)
        .separate_tuple_members(false)
        .enumerate_arrays(false)
}

fn ser<T: serde::Serialize>(value: &T) -> String {
    let mut text = ron::ser::to_string_pretty(value, pretty_config()).expect("serialize");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn write_fixture(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content.as_bytes()).expect("write fixture");
    println!("wrote {} ({} bytes)", path.display(), content.len());
}

fn canonical_project() -> Project {
    Project {
        version: SchemaVersion::V0_1_0,
        name: "demo".into(),
        description: "Sample W14 round-trip fixture.".into(),
        target_tiers: vec![TargetTier::Desktop, TargetTier::Headless],
        plugins: vec![PluginRef {
            id: "rge/standard-light".into(),
            version_req: "0.1".into(),
        }],
        scenes: vec![
            ScenePath("scenes/main-menu.rge-scene".into()),
            ScenePath("scenes/level-1.rge-scene".into()),
        ],
    }
}

fn canonical_scene() -> Scene {
    let parent = EntityId::from_u128(0x0000_0000_0000_0001_0000_0000_0000_0000_u128);
    let child = EntityId::from_u128(0x0000_0000_0000_0002_0000_0000_0000_0000_u128);

    Scene {
        version: SchemaVersion::V0_1_0,
        name: "main-menu".into(),
        entities: vec![
            Entity {
                id: parent,
                name: "Camera".into(),
                components: vec![ComponentValue {
                    type_id: "rge::components::Transform".into(),
                    data:
                        "(translation:(0.0,0.0,5.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))"
                            .into(),
                }],
                relations: vec![],
            },
            Entity {
                id: child,
                name: "ChildLight".into(),
                components: vec![ComponentValue {
                    type_id: "rge::components::Transform".into(),
                    data:
                        "(translation:(1.0,2.0,3.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))"
                            .into(),
                }],
                relations: vec![Relation::ChildOf { parent }],
            },
        ],
        root_entities: vec![parent],
    }
}

fn canonical_prefab() -> Prefab {
    let body = EntityId::from_u128(0x0000_0000_0000_0042_0000_0000_0000_0000_u128);
    Prefab {
        version: SchemaVersion::V0_1_0,
        name: "EnemyArcher".into(),
        parameters: vec![ParamSpec {
            name: "max_health".into(),
            ty: "f32".into(),
            default: "100.0".into(),
        }],
        entities: vec![Entity {
            id: body,
            name: "Body".into(),
            components: vec![ComponentValue {
                type_id: "rge::components::Transform".into(),
                data: "(translation:(0.0,0.0,0.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))"
                    .into(),
            }],
            relations: vec![],
        }],
        exposed_overrides: vec![ExposedOverride {
            entity: body,
            component_type: "rge::components::Transform".into(),
            field_path: "translation".into(),
        }],
    }
}

/// v0.0 fixture for `migration_test` — *no* `version:` field. Built by hand
/// because once we've migrated to v0.1 we lose the v0.0 shape.
fn canonical_v0_0_scene_text() -> &'static str {
    r#"Scene(
    name: "main-menu-v0",
    entities: [],
    root_entities: [],
)
"#
}

fn main() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures = crate_dir.join("tests").join("fixtures");

    write_fixture(
        &fixtures.join("sample_project.rge-project"),
        &ser(&canonical_project()),
    );
    write_fixture(
        &fixtures.join("sample_scene.rge-scene"),
        &ser(&canonical_scene()),
    );
    write_fixture(
        &fixtures.join("sample_prefab.rge-prefab"),
        &ser(&canonical_prefab()),
    );
    write_fixture(
        &fixtures.join("v0.0_to_v0.1_migration_input.rge-scene"),
        canonical_v0_0_scene_text(),
    );

    println!("done.");
}
