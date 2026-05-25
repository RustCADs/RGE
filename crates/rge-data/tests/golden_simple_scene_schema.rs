//! Schema-load regression test for `golden-projects/simple-scene`.
//!
//! Reads the tracked golden `.rge-project` manifest from disk and parses it
//! against the current `rge_data::Project` schema, then resolves the single
//! scene reference relative to the project manifest directory and parses
//! the referenced `.rge-scene` as `rge_data::Scene`. Asserts schema facts on
//! both sides (versions, names, empty plugins, scene roots, the one expected
//! scene path) plus the typed-payload shape of the simple-scene fixture:
//! every `ComponentValue.type_id` uses the canonical `rge::components::*` name
//! and every `ComponentValue.data` payload parses as a [`ron::Value`]. The
//! payload shape is checked through generic [`ron::Value`] inspection only —
//! no component crate is imported and no renderer, GPU, asset-store, cook,
//! screenshot, editor, or runtime fact is asserted.

use std::fs;
use std::path::{Path, PathBuf};

use rge_data::{ComponentValue, Entity, Project, Scene, ScenePath, SchemaVersion, TargetTier};
use ron::value::{Number, Value};

const CAMERA_ENTITY_ID: &str = "0000000000000G000000000000";
const LIGHT_ENTITY_ID: &str = "00000000000010000000000000";

const TRANSFORM_TYPE_ID: &str = "rge::components::Transform";
const CAMERA_TYPE_ID: &str = "rge::components::Camera";
const LIGHT_TYPE_ID: &str = "rge::components::Light";
const VISIBILITY_TYPE_ID: &str = "rge::components::Visibility";

const CAMERA_TRANSFORM_DATA: &str =
    "(translation:(0.0,0.0,5.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))";
const CAMERA_CAMERA_DATA: &str = "(projection: Perspective(fov_y_radians: 1.0471976, near: 0.05, far: 1000.0), viewport: (0.0, 0.0, 1.0, 1.0), priority: 0, is_active: true)";
const CAMERA_VISIBILITY_DATA: &str = "Visible";
const LIGHT_TRANSFORM_DATA: &str =
    "(translation:(0.0,0.0,0.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))";
const LIGHT_LIGHT_DATA: &str =
    "(color:(1.0,1.0,1.0),kind:Directional(illuminance_lux:100000.0),affects_indirect:true)";

fn simple_scene_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

fn read_simple_scene_manifest() -> String {
    let path = simple_scene_manifest_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_scene_referenced_by(project: &Project, manifest_path: &Path) -> (PathBuf, String) {
    let project_dir = manifest_path
        .parent()
        .expect("project manifest path has a parent directory");
    let scene_rel = project
        .scenes
        .first()
        .expect("project must reference at least one scene");
    let scene_path = project_dir.join(scene_rel.as_str());
    let text = fs::read_to_string(&scene_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", scene_path.display()));
    (scene_path, text)
}

fn load_simple_scene() -> Scene {
    let manifest_path = simple_scene_manifest_path();
    let manifest_text = read_simple_scene_manifest();
    let project: Project =
        ron::from_str(&manifest_text).expect("parse simple-scene manifest as Project");
    let (_scene_path, scene_text) = read_scene_referenced_by(&project, &manifest_path);
    ron::from_str(&scene_text).expect("parse referenced scene as Scene")
}

fn find_component<'a>(entity: &'a Entity, type_id: &str) -> &'a ComponentValue {
    entity
        .components
        .iter()
        .find(|component| component.type_id == type_id)
        .unwrap_or_else(|| {
            panic!(
                "entity {} must have a component with type_id {type_id}",
                entity.id
            )
        })
}

fn parse_payload(data: &str) -> Value {
    ron::from_str::<Value>(data)
        .unwrap_or_else(|e| panic!("payload {data:?} must parse as a ron::Value: {e}"))
}

fn expect_map<'a>(value: &'a Value, context: &str) -> &'a ron::value::Map {
    match value {
        Value::Map(map) => map,
        other => panic!("expected {context} to be a Value::Map, got {other:?}"),
    }
}

fn expect_seq<'a>(value: &'a Value, context: &str) -> &'a [Value] {
    match value {
        Value::Seq(seq) => seq.as_slice(),
        other => panic!("expected {context} to be a Value::Seq, got {other:?}"),
    }
}

fn expect_float(value: &Value, context: &str) -> f64 {
    match value {
        Value::Number(Number::F32(wrapped)) => f64::from(wrapped.get()),
        Value::Number(Number::F64(wrapped)) => wrapped.get(),
        other => panic!("expected {context} to be a Value::Number(F32|F64), got {other:?}"),
    }
}

fn get_map_field<'a>(map: &'a ron::value::Map, key: &str, context: &str) -> &'a Value {
    map.get(&Value::String(key.to_string()))
        .unwrap_or_else(|| panic!("expected {context} to contain key {key:?}"))
}

#[test]
fn simple_scene_manifest_parses_as_project() {
    let text = read_simple_scene_manifest();
    let project: Project = ron::from_str(&text).expect("parse simple-scene manifest as Project");

    assert_eq!(project.version, SchemaVersion::V0_1_0);
    assert_eq!(project.name, "simple-scene");
    assert!(
        !project.description.trim().is_empty(),
        "simple-scene description must be non-empty after trimming"
    );
    assert_eq!(project.target_tiers, vec![TargetTier::Desktop]);
    assert!(project.plugins.is_empty(), "plugins must be empty");
    assert_eq!(
        project.scenes,
        vec![ScenePath("scenes/main.rge-scene".to_string())],
        "scenes must be exactly one relative path to scenes/main.rge-scene"
    );
}

#[test]
fn simple_scene_manifest_with_required_field_removed_fails_to_parse() {
    let text = read_simple_scene_manifest();
    let required_line = "    name: \"simple-scene\",";
    assert!(
        text.contains(required_line),
        "expected required `name` field line in manifest text"
    );
    let mutated = text.replace(required_line, "");

    let result: Result<Project, _> = ron::from_str(&mutated);
    assert!(
        result.is_err(),
        "manifest with required `name` field removed must fail to parse as Project, got: {result:?}"
    );
}

#[test]
fn simple_scene_referenced_scene_parses_as_scene() {
    let scene = load_simple_scene();

    assert_eq!(scene.version, SchemaVersion::V0_1_0);
    assert_eq!(scene.name, "main");

    let camera_id = CAMERA_ENTITY_ID
        .parse()
        .expect("camera entity id must parse as EntityId");
    let light_id = LIGHT_ENTITY_ID
        .parse()
        .expect("light entity id must parse as EntityId");

    assert_eq!(
        scene.root_entities,
        vec![camera_id, light_id],
        "root_entities must list the camera id then the light id, in that order"
    );

    for root in &scene.root_entities {
        assert!(
            scene.entities.iter().any(|entity| entity.id == *root),
            "root entity id {root} must exist in scene.entities"
        );
    }

    let camera = scene
        .find_entity(camera_id)
        .expect("camera entity must exist");
    assert_eq!(camera.name, "Camera");
    assert!(camera.relations.is_empty(), "camera has no relations");

    let light = scene
        .find_entity(light_id)
        .expect("light entity must exist");
    assert_eq!(light.name, "KeyLight");
    assert!(light.relations.is_empty(), "light has no relations");
}

#[test]
fn simple_scene_camera_entity_has_canonical_typed_components() {
    let scene = load_simple_scene();
    let camera_id = CAMERA_ENTITY_ID
        .parse()
        .expect("camera entity id must parse as EntityId");
    let camera = scene
        .find_entity(camera_id)
        .expect("camera entity must exist");

    let observed_type_ids: Vec<&str> = camera
        .components
        .iter()
        .map(|component| component.type_id.as_str())
        .collect();
    assert_eq!(
        observed_type_ids,
        vec![TRANSFORM_TYPE_ID, CAMERA_TYPE_ID, VISIBILITY_TYPE_ID],
        "camera component type_ids must be the canonical Transform, Camera, Visibility envelopes"
    );

    let transform = find_component(camera, TRANSFORM_TYPE_ID);
    assert_eq!(
        transform.data, CAMERA_TRANSFORM_DATA,
        "camera Transform payload must match the canonical raw RON literal"
    );
    let _transform_value = parse_payload(&transform.data);

    let camera_component = find_component(camera, CAMERA_TYPE_ID);
    assert_eq!(
        camera_component.data, CAMERA_CAMERA_DATA,
        "camera Camera payload must match the canonical raw RON literal"
    );
    assert!(
        camera_component
            .data
            .contains("Perspective(fov_y_radians: 1.0471976"),
        "camera Camera payload must bind fov_y_radians: 1.0471976 to the Perspective variant"
    );

    let visibility = find_component(camera, VISIBILITY_TYPE_ID);
    assert_eq!(
        visibility.data, CAMERA_VISIBILITY_DATA,
        "camera Visibility payload must be the raw RON variant string `Visible`"
    );
    assert_eq!(
        parse_payload(&visibility.data),
        Value::Unit,
        "camera Visibility payload must parse as a unit-shaped ron::Value (the `Visible` variant)"
    );
}

#[test]
fn simple_scene_camera_payload_pins_fov_y_radians_inside_projection() {
    let scene = load_simple_scene();
    let camera_id = CAMERA_ENTITY_ID
        .parse()
        .expect("camera entity id must parse as EntityId");
    let camera = scene
        .find_entity(camera_id)
        .expect("camera entity must exist");
    let camera_component = find_component(camera, CAMERA_TYPE_ID);

    let parsed = parse_payload(&camera_component.data);
    let outer = expect_map(&parsed, "camera Camera payload");

    let projection = get_map_field(outer, "projection", "camera Camera payload");
    let projection_fields = expect_map(projection, "camera Camera projection variant body");

    let fov_y_radians = get_map_field(
        projection_fields,
        "fov_y_radians",
        "camera Camera projection variant body",
    );
    let fov_value = expect_float(fov_y_radians, "camera Camera projection fov_y_radians");
    assert_eq!(
        fov_value, 1.047_197_6_f64,
        "camera projection must pin fov_y_radians to the FRAC_PI_3-compatible literal"
    );

    let viewport = get_map_field(outer, "viewport", "camera Camera payload");
    let viewport_seq = expect_seq(viewport, "camera Camera viewport");
    assert_eq!(
        viewport_seq.len(),
        4,
        "camera viewport must be a 4-tuple, got {} elements",
        viewport_seq.len()
    );

    assert!(
        camera_component.data.contains("fov_y_radians: 1.0471976"),
        "camera Camera payload must contain the exact fov_y_radians: 1.0471976 literal"
    );
}

#[test]
fn simple_scene_light_entity_has_canonical_typed_components() {
    let scene = load_simple_scene();
    let light_id = LIGHT_ENTITY_ID
        .parse()
        .expect("light entity id must parse as EntityId");
    let light = scene
        .find_entity(light_id)
        .expect("light entity must exist");

    let observed_type_ids: Vec<&str> = light
        .components
        .iter()
        .map(|component| component.type_id.as_str())
        .collect();
    assert_eq!(
        observed_type_ids,
        vec![TRANSFORM_TYPE_ID, LIGHT_TYPE_ID],
        "light component type_ids must be the canonical Transform and Light envelopes"
    );

    let transform = find_component(light, TRANSFORM_TYPE_ID);
    assert_eq!(
        transform.data, LIGHT_TRANSFORM_DATA,
        "light Transform payload must match the canonical raw RON literal"
    );
    let _transform_value = parse_payload(&transform.data);

    let light_component = find_component(light, LIGHT_TYPE_ID);
    assert_eq!(
        light_component.data, LIGHT_LIGHT_DATA,
        "light Light payload must match the canonical raw RON literal"
    );

    let parsed = parse_payload(&light_component.data);
    let outer = expect_map(&parsed, "light Light payload");

    let color = get_map_field(outer, "color", "light Light payload");
    let color_seq = expect_seq(color, "light Light color");
    assert_eq!(
        color_seq.len(),
        3,
        "light color must be a 3-tuple, got {} elements",
        color_seq.len()
    );

    let kind = get_map_field(outer, "kind", "light Light payload");
    let kind_fields = expect_map(kind, "light Light kind variant body");
    let illuminance = get_map_field(
        kind_fields,
        "illuminance_lux",
        "light Light kind variant body",
    );
    let illuminance_value = expect_float(illuminance, "light Light kind illuminance_lux");
    assert_eq!(
        illuminance_value, 100_000.0_f64,
        "light kind illuminance_lux must be the canonical 100000.0 literal"
    );

    assert!(
        light_component
            .data
            .contains("Directional(illuminance_lux:100000.0)"),
        "light Light payload must bind illuminance_lux to the Directional variant"
    );
}

#[test]
fn simple_scene_referenced_scene_with_required_field_removed_fails_to_parse() {
    let manifest_path = simple_scene_manifest_path();
    let manifest_text = read_simple_scene_manifest();
    let project: Project =
        ron::from_str(&manifest_text).expect("parse simple-scene manifest as Project");

    let (_scene_path, scene_text) = read_scene_referenced_by(&project, &manifest_path);
    let required_line = "    name: \"main\",";
    assert!(
        scene_text.contains(required_line),
        "expected required `name` field line in scene text"
    );
    let mutated = scene_text.replace(required_line, "");

    let result: Result<Scene, _> = ron::from_str(&mutated);
    assert!(
        result.is_err(),
        "scene with required `name` field removed must fail to parse as Scene, got: {result:?}"
    );
}
