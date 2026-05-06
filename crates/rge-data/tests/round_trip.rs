//! Round-trip RON → struct → RON byte-identical, on every vendored fixture.
//!
//! Per `tasks/W14/PLAN.md` exit criterion: load each of
//! `tests/fixtures/sample_{project,scene,prefab}.rge-{project,scene,prefab}`,
//! deserialize into the canonical struct, re-serialize with the wave's
//! pretty-print config, and assert the bytes are identical to what the
//! fixture says on disk.
//!
//! The pretty-print config is a single named function so production loaders,
//! migrations, and tests all use the same formatting. If a future wave
//! changes formatting, every fixture has to be regenerated *and* the change
//! is visible in this single function — no hidden divergence.

use rge_data::migration::{builtin, FileKind};
use rge_data::{
    ComponentValue, Entity, EntityId, ExposedOverride, Migration, ParamSpec, PluginRef, Prefab,
    Project, Relation, Scene, ScenePath, SchemaVersion, TargetTier,
};

const PROJECT_FIXTURE: &str = include_str!("fixtures/sample_project.rge-project");
const SCENE_FIXTURE: &str = include_str!("fixtures/sample_scene.rge-scene");
const PREFAB_FIXTURE: &str = include_str!("fixtures/sample_prefab.rge-prefab");

/// Canonical pretty-print configuration. Every loader, fixture, and test in
/// the W14 wave must serialize through *this* function so the round-trip
/// tests can compare bytes.
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

// -- canonical builders -------------------------------------------------

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

// -- byte-identical round-trip ------------------------------------------

#[test]
fn project_fixture_round_trips_byte_identical() {
    let parsed: Project = ron::from_str(PROJECT_FIXTURE).expect("parse project fixture");
    let re_emitted = ser(&parsed);
    assert_eq!(
        re_emitted, PROJECT_FIXTURE,
        "round-trip diverged for sample_project.rge-project"
    );
}

#[test]
fn scene_fixture_round_trips_byte_identical() {
    let parsed: Scene = ron::from_str(SCENE_FIXTURE).expect("parse scene fixture");
    let re_emitted = ser(&parsed);
    assert_eq!(
        re_emitted, SCENE_FIXTURE,
        "round-trip diverged for sample_scene.rge-scene"
    );
}

#[test]
fn prefab_fixture_round_trips_byte_identical() {
    let parsed: Prefab = ron::from_str(PREFAB_FIXTURE).expect("parse prefab fixture");
    let re_emitted = ser(&parsed);
    assert_eq!(
        re_emitted, PREFAB_FIXTURE,
        "round-trip diverged for sample_prefab.rge-prefab"
    );
}

// -- canonical builder ↔ fixture ----------------------------------------

#[test]
fn canonical_project_matches_fixture() {
    let want = ser(&canonical_project());
    assert_eq!(
        want, PROJECT_FIXTURE,
        "canonical Project struct must serialize to the on-disk fixture"
    );
}

#[test]
fn canonical_scene_matches_fixture() {
    let want = ser(&canonical_scene());
    assert_eq!(
        want, SCENE_FIXTURE,
        "canonical Scene struct must serialize to the on-disk fixture"
    );
}

#[test]
fn canonical_prefab_matches_fixture() {
    let want = ser(&canonical_prefab());
    assert_eq!(
        want, PREFAB_FIXTURE,
        "canonical Prefab struct must serialize to the on-disk fixture"
    );
}

// -- regression guards on identity types --------------------------------

#[test]
fn entity_id_display_is_e_underscore_8hex_in_practice() {
    // The PLAN §1.6.3 contract: Display = "e_<8 hex chars>".
    let id = EntityId::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888_u128);
    let s = format!("{id}");
    assert!(s.starts_with("e_"));
    assert_eq!(s.len(), 10);
    assert!(s.chars().skip(2).all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn migration_registry_default_has_chain_for_each_kind() {
    use rge_data::MigrationRegistry;
    let registry = MigrationRegistry::with_builtin();
    for kind in [FileKind::Project, FileKind::Scene, FileKind::Prefab] {
        let chain = registry
            .chain(kind, SchemaVersion::V0_0_0, SchemaVersion::V0_1_0)
            .expect("chain present");
        assert_eq!(chain.len(), 1, "{kind:?} should have a v0.0→v0.1 step");
    }
}

#[test]
fn add_version_field_is_idempotent_on_versioned_text() {
    let mig = builtin::AddVersionField {
        kind: FileKind::Scene,
    };
    let out = mig.apply(SCENE_FIXTURE).expect("apply");
    assert_eq!(
        out, SCENE_FIXTURE,
        "already-versioned input must pass through"
    );
}
