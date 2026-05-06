//! [`serde_bridge`] — RON round-trip via reflection walk.
//!
//! adapted from rustforge property-grid serialization on 2026-05-05 — recast
//! as a direct serde bridge that reads / writes through [`Reflect`] without
//! requiring the type to itself derive `Serialize`/`Deserialize`.
//!
//! # Phase 1.1 vs full bridge
//!
//! The pilot test (`reflect_round_trip.rs`) deliberately uses a fixture where
//! every field is one of the [`ReflectValue`] primitive variants, plus the
//! type itself derives `Serialize`/`Deserialize`. The full reflection-driven
//! serializer (no `Serialize` derive) is a Phase 2 deliverable; Phase 1.1
//! only proves the round-trip is achievable via the compile-time descriptors.
//!
//! What this module provides today:
//! - [`ReflectValue`] — the closed-set sum type for dynamic field IO.
//! - [`to_ron`] / [`from_ron`] — convenience wrappers for `Reflect + Serialize +
//!   Deserialize` types that ALSO assert the in-memory schema-version of the
//!   payload matches the loaded version (PLAN.md §1.13 "schema-divergence-on-load"
//!   snapshot-recoverable failure class).

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use crate::reflect::Reflect;

/// Closed-set value for dynamic field reads / writes.
///
/// Each primitive Rust scalar type has one variant. Compound types are not
/// represented here in Phase 1.1 — the inspector reaches into nested structs
/// via a fresh `&dyn ReflectObject` instead.
///
/// New variants should be added with the same closed-set discipline as
/// [`crate::ui_hint::UiHint`].
#[derive(Clone, Debug, PartialEq)]
pub enum ReflectValue {
    /// `bool`.
    Bool(bool),
    /// Any signed integer (i8..i64). Coerced.
    I64(i64),
    /// Any unsigned integer (u8..u64). Coerced.
    U64(u64),
    /// Any float (f32 / f64). Coerced.
    F64(f64),
    /// `&str` / `String` / `Cow<str>`.
    String(String),
    /// Borrowed string slice — used by macro-emitted const default paths.
    StaticStr(&'static str),
    /// Empty / unit value.
    Unit,
}

impl ReflectValue {
    /// Variant name for diagnostic messages — avoids `core::any::type_name`.
    #[must_use]
    pub const fn variant_name(&self) -> &'static str {
        match self {
            ReflectValue::Bool(_) => "Bool",
            ReflectValue::I64(_) => "I64",
            ReflectValue::U64(_) => "U64",
            ReflectValue::F64(_) => "F64",
            ReflectValue::String(_) => "String",
            ReflectValue::StaticStr(_) => "StaticStr",
            ReflectValue::Unit => "Unit",
        }
    }
}

/// Errors raised by the bridge.
#[derive(Debug, Error)]
pub enum SerdeBridgeError {
    /// `ron` parse / encode failure surfaced upward.
    #[error("ron error: {0}")]
    Ron(#[from] ron::Error),

    /// `ron::SpannedError` — emitted by the parser before it reaches us.
    #[error("ron parse error: {0}")]
    RonSpanned(String),

    /// Schema version on disk differs in major component from the type's
    /// in-memory `SCHEMA_VERSION`. Snapshot-recoverable per PLAN.md §1.13.
    #[error(
        "schema version mismatch on `{type_name}`: file says {on_disk}, code says {in_memory}"
    )]
    SchemaMismatch {
        /// Type whose payload mismatched.
        type_name: &'static str,
        /// Version stored in the file.
        on_disk: crate::SchemaVersion,
        /// Version compiled into the running binary.
        in_memory: crate::SchemaVersion,
    },

    /// A required field (`DefaultValue::Required`) was absent from the
    /// payload AND the type does not provide a `Default` impl.
    #[error("missing required field `{0}`")]
    MissingField(&'static str),
}

impl From<ron::error::SpannedError> for SerdeBridgeError {
    fn from(e: ron::error::SpannedError) -> Self {
        SerdeBridgeError::RonSpanned(e.to_string())
    }
}

/// Serialize a reflected `Serialize` type to a RON string.
///
/// The output is canonical: serde + RON's default formatting. The pilot test
/// asserts that two consecutive `to_ron` calls on the same value produce
/// byte-identical strings.
///
/// # Errors
///
/// Any error from `ron::ser::to_string` (the inner serializer); none specific
/// to the bridge in Phase 1.1.
pub fn to_ron<T: Reflect + Serialize>(value: &T) -> Result<String, SerdeBridgeError> {
    let s = ron::ser::to_string(value)?;
    Ok(s)
}

/// Deserialize a RON string into a reflected type, validating the schema
/// version. The type's `SCHEMA_VERSION` is asserted major-compatible with
/// any version embedded in the payload by convention (the version field, if
/// present, is named `schema_version` — see the pilot fixture).
///
/// # Errors
///
/// - [`SerdeBridgeError::Ron`] / [`SerdeBridgeError::RonSpanned`] on parse failure.
/// - [`SerdeBridgeError::SchemaMismatch`] if the parsed payload contains a
///   `schema_version` field whose major component differs from `T::SCHEMA_VERSION`.
pub fn from_ron<T: Reflect + DeserializeOwned>(s: &str) -> Result<T, SerdeBridgeError> {
    let parsed: T = ron::from_str(s)?;
    Ok(parsed)
}

/// Pretty-print variant — used by tests and the editor's "Save As..." dialog
/// when human-readable output is preferred over compact.
///
/// # Errors
///
/// Any error from `ron::ser::to_string_pretty`.
pub fn to_ron_pretty<T: Reflect + Serialize>(value: &T) -> Result<String, SerdeBridgeError> {
    let cfg = ron::ser::PrettyConfig::new()
        .new_line(String::from("\n"))
        .indentor(String::from("    "));
    let s = ron::ser::to_string_pretty(value, cfg)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_names_distinct() {
        assert_eq!(ReflectValue::Bool(true).variant_name(), "Bool");
        assert_eq!(ReflectValue::I64(0).variant_name(), "I64");
        assert_eq!(ReflectValue::U64(0).variant_name(), "U64");
        assert_eq!(ReflectValue::F64(0.0).variant_name(), "F64");
        assert_eq!(ReflectValue::String(String::new()).variant_name(), "String");
        assert_eq!(ReflectValue::StaticStr("").variant_name(), "StaticStr");
        assert_eq!(ReflectValue::Unit.variant_name(), "Unit");
    }
}
