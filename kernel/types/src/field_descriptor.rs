//! [`FieldDescriptor`] — per-field metadata emitted by `#[derive(Reflect)]`.
//!
//! Each reflected struct exposes a `&'static [FieldDescriptor]` slice via
//! the [`Reflect::FIELDS`](crate::Reflect::FIELDS) constant. Inspector,
//! serde-bridge, and migration paths walk this slice without runtime
//! allocation.
//!
//! # Why `&'static`
//!
//! The slice is laid down as a const by the derive macro. Two consequences:
//! 1. Lookup is fully inlinable — no global registry round-trip.
//! 2. The reflection layer can be used in `no_std` profiles later (Phase 5
//!    — wasm runtime) without a heap allocator.
//!
//! # adapted from rustforge::macros::rcad-property — generalized
//!
//! Rustforge's `PropertyDescriptor` carried dental-domain `RcadUnit`. We drop
//! that and add `default: DefaultValue`, `ui_hint: UiHint`, `serde_skip: bool`
//! to fit the generic-engine target.

use serde::{Deserialize, Serialize};

use crate::type_id::TypeId;
use crate::ui_hint::UiHint;

// `Deserialize` is referenced by `RangeMeta` (a fully owned numeric pair).
// The other types in this module are Serialize-only — see the doc on
// `FieldDescriptor` and `DefaultValue`.

/// Static descriptor for a single reflected field.
///
/// All `&'static str` references are emitted as string-literal references
/// by the derive macro, so no allocation occurs at construction time.
///
/// Serialize-only — descriptors are owned by the binary, never round-tripped
/// through serde. The Serialize side is provided for diagnostic emission
/// (e.g. dumping the registry to JSON for a doc generator).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldDescriptor {
    /// Source-level field name (`stringify!(field_ident)`).
    pub name: &'static str,

    /// Source-level type spelling — kept as written (e.g. `"Vec3"`, not the
    /// canonicalized `"glam::Vec3"`). Drives audit-log diff lines.
    pub ty_name: &'static str,

    /// Stable content-derived id of the field's type. Filled in by the
    /// derive macro using `<FieldType as Reflect>::TYPE_ID`. For non-Reflect
    /// types (primitives), the macro emits a hand-built id from `ty_name`.
    pub ty_id: TypeId,

    /// Optional inclusive numeric range. Cross-checked by the inspector
    /// against [`UiHint::Slider`] when both are set.
    pub range: Option<RangeMeta>,

    /// Default value applied at deserialize-from-empty time. Stored as a
    /// closed-set [`DefaultValue`] enum rather than `String` so the
    /// migration layer can reason about defaults without re-parsing RON.
    pub default: DefaultValue,

    /// Inspector binding hint (PLAN.md §6.15 closed set).
    pub ui_hint: UiHint,

    /// If true, [`serde_bridge`](crate::serde_bridge) skips this field on
    /// both serialize and deserialize. The field still appears in the
    /// reflect walk (the inspector may show it as read-only).
    pub serde_skip: bool,
}

/// Inclusive numeric range. Float-typed so integer / float fields share.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RangeMeta {
    /// Inclusive lower bound.
    pub min: f64,
    /// Inclusive upper bound.
    pub max: f64,
}

/// Closed-set default value for a reflected field.
///
/// New variants require the same closed-set discipline as [`UiHint`]: bumping
/// the enum is a deliberate ABI change. The `Required` and `Custom` cases
/// handle the long tail without forcing the enum to grow.
///
/// Serialize-only at the level that uses `&'static str`. To avoid the
/// `Deserialize<'de>: 'static` lifetime trap, the de side is custom: a
/// helper [`DefaultValue::deserialize_owned`] is provided for round-trip
/// tests, but the macro-emitted const path never round-trips through serde.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum DefaultValue {
    /// Use the `Default::default()` impl of the field type at load time.
    /// Default for fields without an explicit `#[reflect(default = "...")]`.
    DeriveDefault,

    /// `()` — the field has no meaningful default; deserializing an empty
    /// payload yields a [`crate::serde_bridge::SerdeBridgeError::MissingField`].
    Required,

    /// Boolean literal default.
    Bool(bool),

    /// Integer literal default — coerced to i64 in storage; the deserializer
    /// validates against the field's source type at load time.
    Int(i64),

    /// Float literal default.
    Float(f64),

    /// String literal default.
    String(&'static str),

    /// Custom default expression — the macro emits the path to a `fn() -> T`,
    /// stored here as the function's symbol path. The serde bridge does not
    /// invoke it (would require an indirection table); the inspector layer
    /// uses it for "Reset" actions and shows the symbol name on hover.
    Custom(&'static str),
}

impl Default for DefaultValue {
    fn default() -> Self {
        DefaultValue::DeriveDefault
    }
}

impl FieldDescriptor {
    /// Construct a descriptor at compile time. The derive macro emits calls
    /// to this constructor in field order; tests in the kernel use it to
    /// hand-craft fixture descriptors.
    #[must_use]
    pub const fn new(name: &'static str, ty_name: &'static str, ty_id: TypeId) -> Self {
        Self {
            name,
            ty_name,
            ty_id,
            range: None,
            default: DefaultValue::DeriveDefault,
            ui_hint: UiHint::Default,
            serde_skip: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_builder_const() {
        const ID: TypeId = TypeId::from_bytes([0u8; 16]);
        const FD: FieldDescriptor = FieldDescriptor::new("foo", "u32", ID);
        assert_eq!(FD.name, "foo");
        assert!(matches!(FD.default, DefaultValue::DeriveDefault));
        assert!(matches!(FD.ui_hint, UiHint::Default));
        assert!(!FD.serde_skip);
    }

    #[test]
    fn range_meta_bounds() {
        let r = RangeMeta {
            min: -1.0,
            max: 1.0,
        };
        let s = ron::to_string(&r).unwrap();
        let back: RangeMeta = ron::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn default_value_serializes_to_ron() {
        // Serialize-only: descriptor types travel from binary-to-disk for
        // diagnostics, never disk-to-binary. The macro emits these as
        // const literals.
        let dv = DefaultValue::Float(3.14);
        let s = ron::to_string(&dv).unwrap();
        assert!(s.contains("3.14"));
    }
}
