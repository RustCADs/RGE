//! Per-field `UiHint` emission test. Verifies that the macro lowers the
//! `#[reflect(ui = "...", min = .., max = .., step = ..)]` attribute into
//! the closed-set `UiHint` variants.

mod fixtures {
    pub(super) mod render_pass;
}

use fixtures::render_pass::RenderPass;
use rge_kernel_types::{Reflect, UiHint};

fn find_field(name: &str) -> &'static rge_kernel_types::FieldDescriptor {
    RenderPass::FIELDS
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("missing field {name}"))
}

#[test]
fn slider_hint_carries_min_max_step() {
    let f = find_field("clear_color_r");
    match &f.ui_hint {
        UiHint::Slider { min, max, step } => {
            assert!((min - 0.0).abs() < 1e-9);
            assert!((max - 1.0).abs() < 1e-9);
            assert!((step - 0.001).abs() < 1e-9);
        }
        other => panic!("expected Slider, got {other:?}"),
    }
}

#[test]
fn slider_hint_on_integer_carries_min_max() {
    let f = find_field("priority");
    match &f.ui_hint {
        UiHint::Slider { min, max, .. } => {
            assert!((min - 0.0).abs() < 1e-9);
            assert!((max - 1000.0).abs() < 1e-9);
        }
        other => panic!("expected Slider, got {other:?}"),
    }
}

#[test]
fn fields_without_ui_get_default() {
    let f = find_field("name");
    assert!(matches!(f.ui_hint, UiHint::Default));
    let f = find_field("enabled");
    assert!(matches!(f.ui_hint, UiHint::Default));
}

#[test]
fn slider_field_has_range_meta() {
    // `RangeMeta` is populated whenever both `min` and `max` are set.
    let f = find_field("msaa_samples");
    let r = f.range.expect("msaa_samples should have RangeMeta");
    assert!((r.min - 1.0).abs() < 1e-9);
    assert!((r.max - 16.0).abs() < 1e-9);
}

#[test]
fn ty_name_preserved_as_source_text() {
    assert_eq!(find_field("name").ty_name, "String");
    assert_eq!(find_field("priority").ty_name, "i32");
    assert_eq!(find_field("clear_color_r").ty_name, "f32");
    assert_eq!(find_field("msaa_samples").ty_name, "u32");
    assert_eq!(find_field("enabled").ty_name, "bool");
}
