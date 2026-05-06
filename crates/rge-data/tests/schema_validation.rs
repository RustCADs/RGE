//! Schema-level validation guarantees: identity scheme, content addressing,
//! and version field invariants.
//!
//! These tests are the W14 exit-criteria checklist transcribed into Rust.

use rge_data::{AssetId, EntityId, MigrationRegistry, Project, Scene, SchemaVersion};

const PROJECT_FIXTURE: &str = include_str!("fixtures/sample_project.rge-project");
const SCENE_FIXTURE: &str = include_str!("fixtures/sample_scene.rge-scene");
const PREFAB_FIXTURE: &str = include_str!("fixtures/sample_prefab.rge-prefab");

// -- EntityId Display -----------------------------------------------------

#[test]
fn entity_id_display_is_e_underscore_8_hex_chars() {
    // Per PLAN §1.6.3: Display = "e_<8 hex chars>".
    let id = EntityId::from_u128(0xABCD_EF01_2345_6789_FEDC_BA98_7654_3210_u128);
    let s = format!("{id}");
    assert!(s.starts_with("e_"));
    assert_eq!(s.len(), 10, "expected `e_` + 8 hex chars, got `{s}`");
    let hex = &s[2..];
    assert_eq!(hex.len(), 8);
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "hex chars must be lowercase: {hex}"
    );
}

#[test]
fn entity_id_display_is_deterministic() {
    let a = EntityId::from_u128(42);
    let b = EntityId::from_u128(42);
    assert_eq!(format!("{a}"), format!("{b}"));
}

#[test]
fn entity_id_display_distinguishes_distinct_ids() {
    // Two IDs whose Display-relevant slice differs must have distinct
    // Display strings.
    let a = EntityId::from_u128((1u128 << 64) | 0x1111_2222_3333_4444_u128);
    let b = EntityId::from_u128((1u128 << 64) | 0x5555_6666_3333_4444_u128);
    assert_ne!(format!("{a}"), format!("{b}"));
}

// -- AssetId content stability -------------------------------------------

#[test]
fn asset_id_blake3_content_stable_across_calls() {
    let a = AssetId::from_bytes(b"the-source-bytes");
    let b = AssetId::from_bytes(b"the-source-bytes");
    assert_eq!(a, b, "same source bytes must produce same AssetId");
}

#[test]
fn asset_id_canonical_form_has_blake3_prefix() {
    let id = AssetId::from_bytes(b"foo");
    let s = id.to_string();
    assert!(
        s.starts_with("blake3:"),
        "expected `blake3:` prefix, got `{s}`"
    );
}

#[test]
fn asset_id_canonical_hex_is_64_lowercase_chars() {
    let id = AssetId::from_bytes(b"foo");
    let s = id.to_string();
    let hex = s.strip_prefix("blake3:").expect("prefix");
    assert_eq!(hex.len(), 64);
    assert!(hex
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn asset_id_round_trips_through_string() {
    let id = AssetId::from_bytes(b"some content");
    let s = id.to_string();
    let back: AssetId = s.parse().expect("parse");
    assert_eq!(id, back);
}

#[test]
fn asset_id_round_trips_through_ron() {
    let id = AssetId::from_bytes(b"some content");
    let ron_text = ron::to_string(&id).expect("serialize");
    let back: AssetId = ron::from_str(&ron_text).expect("deserialize");
    assert_eq!(id, back);
}

#[test]
fn asset_id_canonical_for_empty_input_known_hash() {
    // Regression guard: this is the canonical BLAKE3 hash of an empty
    // input. If this test fails, the BLAKE3 dep silently shifted.
    let id = AssetId::from_bytes(b"");
    assert_eq!(
        id.to_string(),
        "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
}

// -- SchemaVersion ordering & display ------------------------------------

#[test]
fn schema_version_orders_lexicographically() {
    assert!(SchemaVersion::new(0, 9, 99) < SchemaVersion::new(1, 0, 0));
    assert!(SchemaVersion::new(1, 0, 99) < SchemaVersion::new(1, 1, 0));
    assert!(SchemaVersion::new(1, 0, 0) < SchemaVersion::new(1, 0, 1));
}

#[test]
fn schema_version_string_is_dotted_triple() {
    assert_eq!(SchemaVersion::new(2, 17, 99).to_string(), "2.17.99");
}

#[test]
fn schema_version_round_trips_via_ron_as_string() {
    let v = SchemaVersion::new(1, 2, 3);
    let s = ron::to_string(&v).expect("ser");
    assert_eq!(s, "\"1.2.3\"", "wire form must be a quoted dotted triple");
    let back: SchemaVersion = ron::from_str(&s).expect("de");
    assert_eq!(back, v);
}

// -- Fixtures parse --------------------------------------------------------

#[test]
fn project_fixture_parses() {
    let p: Project = ron::from_str(PROJECT_FIXTURE).expect("parse project fixture");
    assert_eq!(p.version, SchemaVersion::V0_1_0);
    assert_eq!(p.name, "demo");
    assert!(!p.scenes.is_empty());
}

#[test]
fn scene_fixture_parses() {
    let s: Scene = ron::from_str(SCENE_FIXTURE).expect("parse scene fixture");
    assert_eq!(s.version, SchemaVersion::V0_1_0);
    assert!(!s.entities.is_empty());
    assert!(!s.root_entities.is_empty());
}

#[test]
fn prefab_fixture_parses() {
    let p: rge_data::Prefab = ron::from_str(PREFAB_FIXTURE).expect("parse prefab fixture");
    assert_eq!(p.version, SchemaVersion::V0_1_0);
    assert_eq!(p.name, "EnemyArcher");
    assert!(!p.parameters.is_empty());
    assert!(!p.exposed_overrides.is_empty());
}

// -- Migration registry ---------------------------------------------------

#[test]
fn registry_default_is_non_empty() {
    let r = MigrationRegistry::with_builtin();
    assert!(!r.is_empty());
    assert_eq!(r.len(), 3, "one v0.0→v0.1 migration per file kind");
}

#[test]
fn registry_chain_v0_0_to_v0_1_for_each_kind() {
    use rge_data::migration::FileKind;
    let r = MigrationRegistry::with_builtin();
    for kind in [FileKind::Project, FileKind::Scene, FileKind::Prefab] {
        let chain = r
            .chain(kind, SchemaVersion::V0_0_0, SchemaVersion::V0_1_0)
            .expect("chain");
        assert_eq!(
            chain.len(),
            1,
            "each kind should have exactly one v0.0→v0.1 step"
        );
        assert_eq!(chain[0].file_kind(), kind);
    }
}
