//! Tests for `#[reflect(validate = "...")]`, `#[reflect(custom_drawer = "...")]`
//! and `#[reflect(skip)]`. The validate / custom_drawer attrs do not run code
//! at this phase — they propagate into `DefaultValue::Custom` / a skipped
//! TODO field — but the macro must accept them and emit the descriptor
//! correctly. Skip semantics are exercised end-to-end: a skipped field
//! refuses dyn writes but is still visible in the descriptor table.

mod fixtures {
    pub(super) mod render_pass;
}

use fixtures::render_pass::WithValidate;
use rge_kernel_types::{Reflect, ReflectError, ReflectValue};

#[test]
fn skipped_field_appears_in_descriptors() {
    let names: Vec<&str> = WithValidate::FIELDS.iter().map(|f| f.name).collect();
    // `cache` is `#[reflect(skip)]` but still appears in the descriptor —
    // skip only affects serde and dyn-set, not the field listing.
    assert!(names.contains(&"cache"));
    let cache = WithValidate::FIELDS
        .iter()
        .find(|f| f.name == "cache")
        .unwrap();
    assert!(cache.serde_skip);
}

#[test]
fn skipped_field_refuses_dyn_set() {
    let mut w = WithValidate::default();
    let err = w.set_field_dyn("cache", ReflectValue::U64(42)).unwrap_err();
    assert!(matches!(err, ReflectError::SkippedField(_)));
}

#[test]
fn non_skipped_fields_still_writable() {
    let mut w = WithValidate::default();
    w.set_field_dyn("count", ReflectValue::U64(5)).unwrap();
    assert_eq!(w.count, 5);
    w.set_field_dyn("trigger", ReflectValue::Bool(true))
        .unwrap();
    assert!(w.trigger);
}

#[test]
fn validate_and_drawer_attrs_compile() {
    // Compiles is the assertion. Phase 2 will wire validate / custom_drawer
    // into runtime behaviour; Phase 1.1 only verifies the macro accepts the
    // attribute shape without erroring.
    let w = WithValidate::default();
    assert_eq!(w.count, 1);
}
