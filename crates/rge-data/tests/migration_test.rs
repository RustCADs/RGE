//! Migration v0.0 → v0.1 lossless on the vendored fixture.
//!
//! Per `tasks/W14/PLAN.md` exit criterion: load
//! `tests/fixtures/v0.0_to_v0.1_migration_input.rge-scene` (a v0.0 file with
//! no `version:` field), feed it through the v0.0 → v0.1 migration, and
//! assert:
//!
//! 1. The migrated text now carries `version: "0.1.0"`.
//! 2. The migrated text deserializes into a [`Scene`].
//! 3. Every original `entities` / `root_entities` entry survives — the
//!    migration is **lossless**.
//! 4. Re-running the same migration is a no-op (idempotency).

use rge_data::migration::{builtin, FileKind, MigrationError};
use rge_data::{migrate, Migration, MigrationRegistry, Scene, ScenePath, SchemaVersion};

const V0_0_FIXTURE: &str = include_str!("fixtures/v0.0_to_v0.1_migration_input.rge-scene");

#[test]
fn v0_0_input_has_no_version_field() {
    // Sanity guard so we don't accidentally check in a "v0.0" fixture
    // that already carries `version:`.
    assert!(
        !V0_0_FIXTURE.contains("version:"),
        "v0.0 fixture must not contain `version:` — got:\n{V0_0_FIXTURE}"
    );
}

#[test]
fn migrate_v0_0_to_v0_1_adds_version_field() {
    let registry = MigrationRegistry::with_builtin();
    let migrated = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_0_0,
        SchemaVersion::V0_1_0,
        V0_0_FIXTURE,
    )
    .expect("migration succeeds");
    assert!(
        migrated.contains("version: \"0.1.0\""),
        "migrated text must declare v0.1: got:\n{migrated}"
    );
}

#[test]
fn migrate_v0_0_to_v0_1_lossless_preserves_body() {
    // The fixture's distinguishing fields must survive the migration.
    let registry = MigrationRegistry::with_builtin();
    let migrated = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_0_0,
        SchemaVersion::V0_1_0,
        V0_0_FIXTURE,
    )
    .expect("migration succeeds");

    // The unique scene name from the fixture must still be present.
    assert!(migrated.contains("main-menu-v0"), "name lost: {migrated}");

    // Every top-level field name must still appear.
    assert!(migrated.contains("name:"));
    assert!(migrated.contains("entities:"));
    assert!(migrated.contains("root_entities:"));

    // And the migrated text must still parse as a Scene at v0.1.
    let scene: Scene = ron::from_str(&migrated).expect("parse migrated");
    assert_eq!(scene.version, SchemaVersion::V0_1_0);
    assert_eq!(scene.name, "main-menu-v0");
    assert!(scene.entities.is_empty(), "entities accidentally added");
    assert!(
        scene.root_entities.is_empty(),
        "root_entities accidentally added"
    );
}

#[test]
fn migrate_is_idempotent_on_already_versioned_input() {
    let registry = MigrationRegistry::with_builtin();
    let migrated_once = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_0_0,
        SchemaVersion::V0_1_0,
        V0_0_FIXTURE,
    )
    .expect("first migration");

    // Now the input is at v0.1 — re-migrating "v0.1 → v0.1" should be a
    // no-op and yield identical bytes.
    let migrated_again = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_1_0,
        SchemaVersion::V0_1_0,
        &migrated_once,
    )
    .expect("identity migration");
    assert_eq!(migrated_again, migrated_once);
}

#[test]
fn migrate_empty_chain_preserves_input() {
    // from == to with a v0.1 input: returns the input verbatim after a
    // single parse-validation pass (which our text does pass).
    let registry = MigrationRegistry::with_builtin();
    let v0_1_text = "Scene(version:\"0.1.0\",name:\"x\",entities:[],root_entities:[])";
    let out = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_1_0,
        SchemaVersion::V0_1_0,
        v0_1_text,
    )
    .expect("identity migration");
    assert_eq!(out, v0_1_text);
}

#[test]
fn migrate_rejects_unparseable_input() {
    let registry = MigrationRegistry::with_builtin();
    let err = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_0_0,
        SchemaVersion::V0_1_0,
        "{ this is not RON",
    )
    .unwrap_err();
    assert!(matches!(err, MigrationError::InputParse(_)));
}

#[test]
fn migrate_rejects_downgrade() {
    let registry = MigrationRegistry::with_builtin();
    let err = migrate(
        &registry,
        FileKind::Scene,
        SchemaVersion::V0_1_0,
        SchemaVersion::V0_0_0,
        // Body must parse for the input check to pass — use a v0.1 doc.
        "Scene(version:\"0.1.0\",name:\"x\",entities:[],root_entities:[])",
    )
    .unwrap_err();
    assert!(matches!(err, MigrationError::Downgrade { .. }));
}

#[test]
fn migrate_no_chain_for_unsupported_target() {
    // No registered migration for v0.1 → v0.5; expect NoChain.
    let registry = MigrationRegistry::with_builtin();
    let err = registry
        .chain(
            FileKind::Scene,
            SchemaVersion::V0_1_0,
            SchemaVersion::new(0, 5, 0),
        )
        .unwrap_err();
    assert!(matches!(err, MigrationError::NoChain { .. }));
}

#[test]
fn migration_with_populated_v0_0_scene_keeps_entities() {
    // Direct unit-style coverage for the migration body itself: a v0.0
    // file with a real entity list must come out the other side with
    // every entity intact and the new version field prepended.
    let v0_0 = r#"Scene(
    name: "rich",
    entities: [
        Entity(
            id: "00000000000000000000000001",
            name: "A",
            components: [],
            relations: [],
        ),
        Entity(
            id: "00000000000000000000000002",
            name: "B",
            components: [],
            relations: [],
        ),
    ],
    root_entities: [
        "00000000000000000000000001",
        "00000000000000000000000002",
    ],
)
"#;
    let mig = builtin::AddVersionField {
        kind: FileKind::Scene,
    };
    let out = mig.apply(v0_0).expect("apply");
    let scene: Scene = ron::from_str(&out).expect("parse");
    assert_eq!(scene.name, "rich");
    assert_eq!(scene.entities.len(), 2);
    assert_eq!(scene.root_entities.len(), 2);
    assert_eq!(scene.version, SchemaVersion::V0_1_0);
}

#[test]
fn project_kind_migration_preserves_scene_paths() {
    // Repeat the lossless guarantee on Project kind. Use a v0.0 project
    // fixture inline.
    let v0_0_project = r#"Project(
    name: "demo",
    description: "test",
    target_tiers: [],
    plugins: [],
    scenes: [
        "scenes/a.rge-scene",
        "scenes/b.rge-scene",
    ],
)
"#;
    let registry = MigrationRegistry::with_builtin();
    let migrated = migrate(
        &registry,
        FileKind::Project,
        SchemaVersion::V0_0_0,
        SchemaVersion::V0_1_0,
        v0_0_project,
    )
    .expect("migrate");
    assert!(migrated.contains("\"scenes/a.rge-scene\""));
    assert!(migrated.contains("\"scenes/b.rge-scene\""));
    assert!(migrated.contains("version: \"0.1.0\""));

    // And it must round-trip through Project deserialization.
    let p: rge_data::Project = ron::from_str(&migrated).expect("parse");
    assert_eq!(p.scenes.len(), 2);
    assert_eq!(p.scenes[0], ScenePath("scenes/a.rge-scene".into()));
    assert_eq!(p.version, SchemaVersion::V0_1_0);
}
