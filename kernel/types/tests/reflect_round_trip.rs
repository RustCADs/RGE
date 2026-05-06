//! Pilot round-trip test: a `RenderPass`-shaped fixture must serialize via
//! RON and deserialize back to a byte-identical re-serialization.
//!
//! Per W02 dispatch: this is THE exit criterion for Phase 1.1. The fixture
//! mirrors the shape of `rustforge/apps/editor-app/`'s render-pass struct
//! so the pilot mirrors a real consumer.
//!
//! The struct here uses hand-written `Reflect` + serde derives (not the
//! macro) so this test depends on `rge-kernel-types` ALONE. The macro-driven
//! variant lives in `rge-macros-reflect/tests/derive_test.rs`.

use rge_kernel_types::field_descriptor::FieldDescriptor;
use rge_kernel_types::reflect::{Reflect, ReflectError, ReflectKind};
use rge_kernel_types::schema_version::SchemaVersion;
use rge_kernel_types::serde_bridge::{from_ron, to_ron, ReflectValue};
use rge_kernel_types::type_id::TypeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RenderPass {
    name: String,
    enabled: bool,
    priority: i32,
    clear_color_r: f32,
    clear_color_g: f32,
    clear_color_b: f32,
    clear_color_a: f32,
    msaa_samples: u32,
}

impl Reflect for RenderPass {
    const TYPE_ID: TypeId = TypeId::from_bytes([
        // Stable hand-rolled bytes for the fixture; the real macro path
        // computes this from the qualified name. ASCII for "RenderPass" + pad.
        b'R', b'e', b'n', b'd', b'e', b'r', b'P', b'a', b's', b's', 0, 0, 0, 0, 0, 1,
    ]);
    const TYPE_NAME: &'static str = "RenderPass";
    const FQ_TYPE_NAME: &'static str = "reflect_round_trip::RenderPass";
    const SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);
    const FIELDS: &'static [FieldDescriptor] = &[
        FieldDescriptor::new("name", "String", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("enabled", "bool", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("priority", "i32", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("clear_color_r", "f32", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("clear_color_g", "f32", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("clear_color_b", "f32", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("clear_color_a", "f32", TypeId::from_bytes([0u8; 16])),
        FieldDescriptor::new("msaa_samples", "u32", TypeId::from_bytes([0u8; 16])),
    ];
    const KIND: ReflectKind = ReflectKind::NamedStruct;

    fn get_field_dyn(&self, name: &str) -> Result<ReflectValue, ReflectError> {
        match name {
            "name" => Ok(ReflectValue::String(self.name.clone())),
            "enabled" => Ok(ReflectValue::Bool(self.enabled)),
            "priority" => Ok(ReflectValue::I64(i64::from(self.priority))),
            "clear_color_r" => Ok(ReflectValue::F64(f64::from(self.clear_color_r))),
            "clear_color_g" => Ok(ReflectValue::F64(f64::from(self.clear_color_g))),
            "clear_color_b" => Ok(ReflectValue::F64(f64::from(self.clear_color_b))),
            "clear_color_a" => Ok(ReflectValue::F64(f64::from(self.clear_color_a))),
            "msaa_samples" => Ok(ReflectValue::U64(u64::from(self.msaa_samples))),
            _ => Err(ReflectError::UnknownField(static_field_name(name))),
        }
    }

    fn set_field_dyn(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError> {
        match (name, value) {
            ("name", ReflectValue::String(s)) => {
                self.name = s;
                Ok(())
            }
            ("enabled", ReflectValue::Bool(b)) => {
                self.enabled = b;
                Ok(())
            }
            ("priority", ReflectValue::I64(v)) => {
                self.priority = v as i32;
                Ok(())
            }
            ("clear_color_r", ReflectValue::F64(v)) => {
                self.clear_color_r = v as f32;
                Ok(())
            }
            ("clear_color_g", ReflectValue::F64(v)) => {
                self.clear_color_g = v as f32;
                Ok(())
            }
            ("clear_color_b", ReflectValue::F64(v)) => {
                self.clear_color_b = v as f32;
                Ok(())
            }
            ("clear_color_a", ReflectValue::F64(v)) => {
                self.clear_color_a = v as f32;
                Ok(())
            }
            ("msaa_samples", ReflectValue::U64(v)) => {
                self.msaa_samples = v as u32;
                Ok(())
            }
            (n, v) => match n {
                "name" | "enabled" | "priority" | "clear_color_r" | "clear_color_g"
                | "clear_color_b" | "clear_color_a" | "msaa_samples" => {
                    Err(ReflectError::TypeMismatch {
                        field: static_field_name(n),
                        expected: expected_for(n),
                        got: v.variant_name(),
                    })
                }
                other => Err(ReflectError::UnknownField(static_field_name(other))),
            },
        }
    }
}

fn static_field_name(name: &str) -> &'static str {
    match name {
        "name" => "name",
        "enabled" => "enabled",
        "priority" => "priority",
        "clear_color_r" => "clear_color_r",
        "clear_color_g" => "clear_color_g",
        "clear_color_b" => "clear_color_b",
        "clear_color_a" => "clear_color_a",
        "msaa_samples" => "msaa_samples",
        _ => "<unknown>",
    }
}

fn expected_for(name: &str) -> &'static str {
    match name {
        "name" => "String",
        "enabled" => "bool",
        "priority" => "i32",
        "clear_color_r" | "clear_color_g" | "clear_color_b" | "clear_color_a" => "f32",
        "msaa_samples" => "u32",
        _ => "<unknown>",
    }
}

fn sample() -> RenderPass {
    RenderPass {
        name: "MainColorPass".into(),
        enabled: true,
        priority: 100,
        clear_color_r: 0.05,
        clear_color_g: 0.10,
        clear_color_b: 0.20,
        clear_color_a: 1.0,
        msaa_samples: 4,
    }
}

#[test]
fn render_pass_round_trips_byte_identically_via_ron() {
    let original = sample();

    // Phase 1.1 byte-identity claim:
    //   serialize(original) -> s1
    //   deserialize(s1)     -> reconstructed
    //   serialize(reconstructed) -> s2
    //   assert s1 == s2  (byte-identical)
    //   assert reconstructed == original (value-identical)
    let s1 = to_ron(&original).expect("first ron serialization");
    let reconstructed: RenderPass = from_ron(&s1).expect("ron deserialization");
    assert_eq!(reconstructed, original, "value round-trip failed");

    let s2 = to_ron(&reconstructed).expect("second ron serialization");
    assert_eq!(s1, s2, "byte-identity failed: \n{}\n!=\n{}\n", s1, s2);
}

#[test]
fn render_pass_reflection_descriptors_match_struct() {
    assert_eq!(RenderPass::FIELDS.len(), 8);
    assert_eq!(RenderPass::TYPE_NAME, "RenderPass");
    assert_eq!(RenderPass::SCHEMA_VERSION.major, 1);
    assert!(matches!(RenderPass::KIND, ReflectKind::NamedStruct));

    // Field-by-name access matches the descriptor order.
    let expected: Vec<&str> = vec![
        "name",
        "enabled",
        "priority",
        "clear_color_r",
        "clear_color_g",
        "clear_color_b",
        "clear_color_a",
        "msaa_samples",
    ];
    let actual: Vec<&str> = RenderPass::FIELDS.iter().map(|f| f.name).collect();
    assert_eq!(actual, expected);
}

#[test]
fn dyn_field_mutation_round_trip() {
    let mut rp = sample();
    rp.set_field_dyn("priority", ReflectValue::I64(999))
        .unwrap();
    rp.set_field_dyn("enabled", ReflectValue::Bool(false))
        .unwrap();
    assert_eq!(rp.priority, 999);
    assert!(!rp.enabled);

    let s1 = to_ron(&rp).unwrap();
    let back: RenderPass = from_ron(&s1).unwrap();
    assert_eq!(back, rp);
}
