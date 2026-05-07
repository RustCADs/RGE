//! [`Reflect`] — trait emitted by `#[derive(Reflect)]`.
//!
//! Every reflected type exposes:
//! - A stable [`TypeId`] derived from its module path + name.
//! - A `&'static [FieldDescriptor]` slice.
//! - A `SCHEMA_VERSION` constant (PLAN.md hot-reload migration prerequisite).
//! - Dynamic field get / set via the [`ReflectValue`](crate::ReflectValue)
//!   sum type. We deliberately avoid `dyn Any` downcasts: each primitive has
//!   an explicit `ReflectValue` variant. This makes the surface auditable and
//!   keeps the `unsafe_code = forbid` lint clean.
//!
//! # Phase 1.1 scope
//!
//! Phase 1.1 covers structs with named fields. Enum reflection (variant
//! switch) and tuple-struct reflection are forward-declared via the trait
//! shape (see the `kind()` method) but the derive macro will reject those
//! shapes for now with a clear error.

use thiserror::Error;

use crate::field_descriptor::FieldDescriptor;
use crate::schema_version::SchemaVersion;
use crate::serde_bridge::ReflectValue;
use crate::type_id::TypeId;

/// Kind of reflected type. Forward-declares enum / tuple-struct support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReflectKind {
    /// Struct with named fields. Phase 1.1 only supports this shape.
    NamedStruct,
    /// Tuple struct — Phase 2 deliverable.
    TupleStruct,
    /// Enum — Phase 2 deliverable; `EnumDropdown` `UiHint` is a placeholder.
    Enum,
}

/// Errors surfaced by the dynamic field accessors.
#[derive(Debug, Error)]
pub enum ReflectError {
    /// `name` does not match any field on this type.
    #[error("unknown field `{0}` on reflected type")]
    UnknownField(&'static str),

    /// The supplied [`ReflectValue`] variant did not match the field type.
    #[error("type mismatch on field `{field}`: expected `{expected}`, got `{got}`")]
    TypeMismatch {
        /// Field name on the destination struct.
        field: &'static str,
        /// Type spelling expected by the field descriptor.
        expected: &'static str,
        /// Variant name of the [`ReflectValue`] that was passed.
        got: &'static str,
    },

    /// The field is `#[reflect(skip)]` — refuse to set via reflection.
    #[error("field `{0}` is marked #[reflect(skip)] and cannot be set via reflection")]
    SkippedField(&'static str),
}

/// Compile-time reflection surface emitted by `#[derive(Reflect)]`.
///
/// # Object-safety note
///
/// The const-only items (`TYPE_ID`, `TYPE_NAME`, `SCHEMA_VERSION`, `FIELDS`,
/// `KIND`) prevent `dyn Reflect` directly. Tooling layers that need a
/// trait-object handle should use [`ReflectObject`] (defined below) which
/// mirrors the const-items as `fn`-shaped accessors.
pub trait Reflect: Sized {
    /// Stable content-derived id. Emitted as
    /// `TypeId::of_name(concat!(module_path!(), "::", stringify!(Self)))`.
    const TYPE_ID: TypeId;

    /// Source-level type name, e.g. `"RenderPass"`. Surface-name; no module
    /// path (cf. [`Reflect::FQ_TYPE_NAME`] for the qualified form).
    const TYPE_NAME: &'static str;

    /// Fully-qualified type name including module path.
    const FQ_TYPE_NAME: &'static str;

    /// Schema version. Required (see `crate::schema_version`).
    const SCHEMA_VERSION: SchemaVersion;

    /// Per-field metadata in source order.
    const FIELDS: &'static [FieldDescriptor];

    /// Shape of the reflected type (named struct / tuple / enum).
    const KIND: ReflectKind;

    /// Read a field by name into a [`ReflectValue`].
    ///
    /// Returns [`ReflectError::UnknownField`] if `name` is not on the type.
    /// Skipped fields are still readable (the inspector may show them).
    ///
    /// # Errors
    ///
    /// See [`ReflectError`].
    fn get_field_dyn(&self, name: &str) -> Result<ReflectValue, ReflectError>;

    /// Write a field by name from a [`ReflectValue`].
    ///
    /// # Errors
    ///
    /// - [`ReflectError::UnknownField`] if `name` is not on the type.
    /// - [`ReflectError::TypeMismatch`] if the value variant does not match
    ///   the field's source type.
    /// - [`ReflectError::SkippedField`] if the field is `#[reflect(skip)]`.
    fn set_field_dyn(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError>;
}

/// Object-safe shadow trait. Tooling that needs trait-object reflection
/// (the editor inspector, the W14 RON-emitter) constructs `&dyn ReflectObject`
/// via the blanket impl below; the trait holds only `fn`-shaped methods so
/// it is safely object-safe.
///
/// # Why distinct method names from `Reflect`
///
/// `Reflect` and `ReflectObject` both have field accessors, but the names
/// differ (`get_field_dyn` vs `field_get`) so that an `impl<T: Reflect>
/// ReflectObject for T` blanket impl does not create method-resolution
/// ambiguity at user call-sites that import both traits.
pub trait ReflectObject {
    /// Field-by-name read. Same shape as [`Reflect::get_field_dyn`].
    ///
    /// # Errors
    /// See [`ReflectError`].
    fn field_get(&self, name: &str) -> Result<ReflectValue, ReflectError>;

    /// Field-by-name write. Same shape as [`Reflect::set_field_dyn`].
    ///
    /// # Errors
    /// See [`ReflectError`].
    fn field_set(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError>;

    /// `Reflect::TYPE_ID` exposed via fn shape.
    fn reflect_type_id(&self) -> TypeId;

    /// `Reflect::TYPE_NAME` exposed via fn shape.
    fn reflect_type_name(&self) -> &'static str;

    /// `Reflect::SCHEMA_VERSION` exposed via fn shape.
    fn reflect_schema_version(&self) -> SchemaVersion;

    /// `Reflect::FIELDS` exposed via fn shape.
    fn reflect_fields(&self) -> &'static [FieldDescriptor];

    /// `Reflect::KIND` exposed via fn shape.
    fn reflect_kind(&self) -> ReflectKind;
}

impl<T: Reflect> ReflectObject for T {
    #[inline]
    fn field_get(&self, name: &str) -> Result<ReflectValue, ReflectError> {
        <Self as Reflect>::get_field_dyn(self, name)
    }
    #[inline]
    fn field_set(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError> {
        <Self as Reflect>::set_field_dyn(self, name, value)
    }
    #[inline]
    fn reflect_type_id(&self) -> TypeId {
        <Self as Reflect>::TYPE_ID
    }
    #[inline]
    fn reflect_type_name(&self) -> &'static str {
        <Self as Reflect>::TYPE_NAME
    }
    #[inline]
    fn reflect_schema_version(&self) -> SchemaVersion {
        <Self as Reflect>::SCHEMA_VERSION
    }
    #[inline]
    fn reflect_fields(&self) -> &'static [FieldDescriptor] {
        <Self as Reflect>::FIELDS
    }
    #[inline]
    fn reflect_kind(&self) -> ReflectKind {
        <Self as Reflect>::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_descriptor::FieldDescriptor;

    /// Hand-rolled fixture exercising the trait without the macro. The
    /// macro-driven path is covered in `rge-macros-reflect`'s tests.
    #[derive(Default)]
    struct Hand {
        a: i32,
        b: f32,
    }

    impl Reflect for Hand {
        const TYPE_ID: TypeId =
            TypeId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        const TYPE_NAME: &'static str = "Hand";
        const FQ_TYPE_NAME: &'static str = "tests::Hand";
        const SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0, 0);
        const FIELDS: &'static [FieldDescriptor] = &[
            FieldDescriptor::new("a", "i32", TypeId::from_bytes([0u8; 16])),
            FieldDescriptor::new("b", "f32", TypeId::from_bytes([0u8; 16])),
        ];
        const KIND: ReflectKind = ReflectKind::NamedStruct;

        fn get_field_dyn(&self, name: &str) -> Result<ReflectValue, ReflectError> {
            match name {
                "a" => Ok(ReflectValue::I64(i64::from(self.a))),
                "b" => Ok(ReflectValue::F64(f64::from(self.b))),
                other => Err(ReflectError::UnknownField(field_name_static(other))),
            }
        }

        fn set_field_dyn(&mut self, name: &str, value: ReflectValue) -> Result<(), ReflectError> {
            match (name, value) {
                ("a", ReflectValue::I64(v)) => {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "test fixture: hand-rolled `Hand` mirrors the i32-narrowing path the macro emits for fields whose source type is i32 read through a ReflectValue::I64 carrier"
                    )]
                    {
                        self.a = v as i32;
                    }
                    Ok(())
                }
                ("b", ReflectValue::F64(v)) => {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "test fixture: hand-rolled `Hand` mirrors the f32-narrowing path the macro emits for fields whose source type is f32 read through a ReflectValue::F64 carrier"
                    )]
                    {
                        self.b = v as f32;
                    }
                    Ok(())
                }
                ("a", v) => Err(ReflectError::TypeMismatch {
                    field: "a",
                    expected: "i32",
                    got: v.variant_name(),
                }),
                ("b", v) => Err(ReflectError::TypeMismatch {
                    field: "b",
                    expected: "f32",
                    got: v.variant_name(),
                }),
                (other, _) => Err(ReflectError::UnknownField(field_name_static(other))),
            }
        }
    }

    fn field_name_static(name: &str) -> &'static str {
        match name {
            "a" => "a",
            "b" => "b",
            _ => "<unknown>",
        }
    }

    #[test]
    fn named_struct_round_trip_via_dyn() {
        let mut h = Hand { a: 1, b: 2.0 };
        let dyn_ref: &dyn ReflectObject = &h;
        assert_eq!(dyn_ref.reflect_type_name(), "Hand");
        assert_eq!(dyn_ref.reflect_fields().len(), 2);

        let v = <Hand as Reflect>::get_field_dyn(&h, "a").unwrap();
        assert!(matches!(v, ReflectValue::I64(1)));

        <Hand as Reflect>::set_field_dyn(&mut h, "a", ReflectValue::I64(42)).unwrap();
        assert_eq!(h.a, 42);

        // unknown field
        assert!(matches!(
            <Hand as Reflect>::get_field_dyn(&h, "nope"),
            Err(ReflectError::UnknownField(_))
        ));

        // type mismatch
        let err =
            <Hand as Reflect>::set_field_dyn(&mut h, "a", ReflectValue::Bool(true)).unwrap_err();
        assert!(matches!(err, ReflectError::TypeMismatch { .. }));
    }
}
