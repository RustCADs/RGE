//! Macro-driven derive test. Parallel to the hand-rolled variant in
//! `kernel/types/tests/reflect_round_trip.rs`. The derive must produce
//! identical observable behaviour: byte-identical RON round-trip,
//! correct field count, dynamic accessors that match.

mod fixtures {
    pub(super) mod render_pass;
}

use fixtures::render_pass::{sample_render_pass, RenderPass};
use rge_kernel_types::{from_ron, to_ron, Reflect, ReflectKind, ReflectValue, SchemaVersion};

#[test]
fn derived_render_pass_round_trips_byte_identically() {
    let original = sample_render_pass();
    let s1 = to_ron(&original).expect("first ron serialization");
    let reconstructed: RenderPass = from_ron(&s1).expect("ron deserialization");
    assert_eq!(reconstructed, original);
    let s2 = to_ron(&reconstructed).expect("second ron serialization");
    assert_eq!(s1, s2, "byte-identity failed:\n{}\n!=\n{}\n", s1, s2);
}

#[test]
fn derived_render_pass_descriptors() {
    assert_eq!(RenderPass::TYPE_NAME, "RenderPass");
    assert_eq!(RenderPass::SCHEMA_VERSION, SchemaVersion::new(1, 0, 0));
    assert!(matches!(RenderPass::KIND, ReflectKind::NamedStruct));
    let names: Vec<&str> = RenderPass::FIELDS.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        vec![
            "name",
            "enabled",
            "priority",
            "clear_color_r",
            "clear_color_g",
            "clear_color_b",
            "clear_color_a",
            "msaa_samples"
        ]
    );
}

#[test]
fn derived_dyn_field_get_and_set() {
    let mut rp = sample_render_pass();
    let v = rp.get_field_dyn("priority").unwrap();
    assert!(matches!(v, ReflectValue::I64(100)));

    rp.set_field_dyn("priority", ReflectValue::I64(999))
        .unwrap();
    assert_eq!(rp.priority, 999);

    rp.set_field_dyn("clear_color_r", ReflectValue::F64(0.5))
        .unwrap();
    assert!((rp.clear_color_r - 0.5).abs() < 1e-6);

    rp.set_field_dyn("name", ReflectValue::String("DepthPass".into()))
        .unwrap();
    assert_eq!(rp.name, "DepthPass");

    // Unknown field
    let err = rp.set_field_dyn("nonexistent", ReflectValue::Bool(false));
    assert!(err.is_err());

    // Type mismatch
    let err = rp
        .set_field_dyn("priority", ReflectValue::Bool(true))
        .unwrap_err();
    assert!(matches!(
        err,
        rge_kernel_types::ReflectError::TypeMismatch { .. }
    ));
}

#[test]
fn derived_fq_type_name_includes_module_path() {
    // The macro emits FQ_TYPE_NAME via `concat!(module_path!(), "::", ...)`.
    // The integration-test crate's module path is `derive_test::fixtures::render_pass`.
    assert!(RenderPass::FQ_TYPE_NAME.ends_with("::RenderPass"));
    assert!(RenderPass::FQ_TYPE_NAME.contains("render_pass"));
}
