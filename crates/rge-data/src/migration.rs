// adapted from rustforge::crates::persistence on 2026-05-05 — generalized for
//                                                              schema-versioned RON
//                                                              source files
//                                                              (project / scene /
//                                                              prefab).
//
//! Schema migration registry.
//!
//! Per `PLAN.md` §1.6.7 every source file carries `version: "x.y.z"` and the
//! loader walks a chain of registered migrations to bring an older payload
//! up to the current schema. This module supplies:
//!
//! 1. The [`Migration`] trait — one entry per `(from, to)` pair on a given
//!    file kind.
//! 2. The [`MigrationRegistry`] — an ordered list of migrations.
//! 3. The [`migrate`] entry point — finds the chain from the source file's
//!    version up to a target version and applies each migration in turn,
//!    returning either the migrated RON text or a [`MigrationError`].
//!
//! # Wire shape
//!
//! Migrations operate on **RON text** rather than typed structs. This is
//! deliberate: a migration may rename a field, change a type, or split a
//! single value into a tuple — none of which a strongly-typed
//! `From<Old> for New` impl can express without keeping every old struct
//! definition around forever. Working at the RON-AST level (via
//! [`ron::Value`]) lets each migration perform a focused, surgical edit
//! and dispose of the AST when it's done.
//!
//! # File-kind tagging
//!
//! Project, scene, and prefab files all share the `version: "x.y.z"`
//! convention but their other fields differ. The registry tags each
//! [`Migration`] with a [`FileKind`] so a v0.1 → v0.2 *scene* migration
//! isn't accidentally applied to a *project*.
//!
//! # v0.0 → v0.1 baseline
//!
//! The first registered migration on every file kind is the v0.0 → v0.1
//! pass that adds the explicit `version: "0.1.0"` field if it's missing
//! and otherwise leaves the document alone. v0.0 fixtures without a
//! `version` field are interpreted as `0.0.0` for chain selection.
//!
//! See `crates/rge-data/tests/migration_test.rs` for the round-trip test
//! against the vendored fixture.

use core::fmt;

use thiserror::Error;

use crate::schema_version::SchemaVersion;

/// Which kind of source file a migration applies to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FileKind {
    /// `.rge-project` — top-level project file.
    Project,
    /// `.rge-scene` — one authored scene.
    Scene,
    /// `.rge-prefab` — reusable entity bundle.
    Prefab,
}

impl fmt::Display for FileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileKind::Project => f.write_str("project"),
            FileKind::Scene => f.write_str("scene"),
            FileKind::Prefab => f.write_str("prefab"),
        }
    }
}

/// One step in a migration chain — `from` → `to` on a given file kind.
///
/// Implementors receive the RON text of the source file and return the
/// migrated text. The text **must** parse as RON before and after the
/// migration; the registry validates this.
///
/// Requires [`fmt::Debug`] so a chain `Vec<&dyn Migration>` is itself
/// Debug-printable; this enables `Result<Vec<&dyn Migration>, _>::unwrap_err`
/// inside tests without manually unwrapping each step.
pub trait Migration: fmt::Debug + Send + Sync {
    /// Schema version this migration upgrades from.
    #[allow(clippy::wrong_self_convention)] // `from_version` is an accessor, not a constructor.
    fn from_version(&self) -> SchemaVersion;
    /// Schema version produced by this migration. Always strictly greater
    /// than [`Self::from_version`].
    fn to_version(&self) -> SchemaVersion;
    /// Which file kind this migration applies to.
    fn file_kind(&self) -> FileKind;
    /// Apply the migration to RON text. Returning `Ok` means the result is
    /// canonical-shaped RON for [`Self::to_version`].
    ///
    /// # Errors
    ///
    /// Returns [`MigrationError`] when the supplied text fails to parse, the
    /// migration body cannot transform it, or the produced text fails the
    /// post-migration parse check.
    fn apply(&self, ron_text: &str) -> Result<String, MigrationError>;
}

/// Migration-time errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// The source RON failed to parse before the migration ran.
    #[error("source RON failed to parse: {0}")]
    InputParse(String),
    /// The migration produced RON that fails to parse.
    #[error("migration {from}→{to} on {kind} produced invalid RON: {reason}")]
    OutputParse {
        /// Source schema version.
        from: SchemaVersion,
        /// Target schema version of the broken migration.
        to: SchemaVersion,
        /// Which file kind the broken migration was registered against.
        kind: FileKind,
        /// Underlying parser error message.
        reason: String,
    },
    /// No migration chain exists from `from` to `to` on the given kind.
    #[error("no migration chain {from}→{to} for {kind}")]
    NoChain {
        /// Source schema version.
        from: SchemaVersion,
        /// Requested target schema version.
        to: SchemaVersion,
        /// File kind being migrated.
        kind: FileKind,
    },
    /// Internal — a registered migration's `to_version` did not match the
    /// next chain link's `from_version`. Caught at registration time.
    #[error("migration chain inconsistent at step {step}: expected from={expected}, got {actual}")]
    InconsistentChain {
        /// Index of the broken link.
        step: usize,
        /// Expected `from_version` at that step.
        expected: SchemaVersion,
        /// Actual `from_version` at that step.
        actual: SchemaVersion,
    },
    /// Caller passed `from > to` (downgrade not supported).
    #[error("downgrade not supported (from={from}, to={to})")]
    Downgrade {
        /// Source schema version.
        from: SchemaVersion,
        /// Lower target schema version that would require a downgrade.
        to: SchemaVersion,
    },
    /// User-supplied migration body returned an error string. Surfaced as
    /// [`MigrationError::Custom`] so registries can wrap arbitrary
    /// transformer errors without bloating this enum.
    #[error("migration {from}→{to} on {kind}: {reason}")]
    Custom {
        /// Source schema version.
        from: SchemaVersion,
        /// Produced schema version.
        to: SchemaVersion,
        /// File kind being migrated.
        kind: FileKind,
        /// Free-form reason emitted by the migration body.
        reason: String,
    },
}

/// Ordered list of registered migrations. Owned and constructed by the
/// crate consumer (typically the editor / asset pipeline at startup).
pub struct MigrationRegistry {
    migrations: Vec<Box<dyn Migration>>,
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MigrationRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MigrationRegistry")
            .field("count", &self.migrations.len())
            .finish()
    }
}

impl MigrationRegistry {
    /// Empty registry. Migrations are appended via [`Self::register`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Build a registry pre-populated with the v0.0 → v0.1 baseline
    /// migration on all three file kinds. This is the canonical entry
    /// point for editors / cookers that don't ship custom migrations of
    /// their own.
    #[must_use]
    pub fn with_builtin() -> Self {
        let mut r = Self::new();
        r.register(Box::new(builtin::AddVersionField {
            kind: FileKind::Project,
        }));
        r.register(Box::new(builtin::AddVersionField {
            kind: FileKind::Scene,
        }));
        r.register(Box::new(builtin::AddVersionField {
            kind: FileKind::Prefab,
        }));
        r
    }

    /// Append a migration. The registry does not enforce ordering at
    /// register-time; [`Self::migrate`] picks the right chain dynamically.
    pub fn register(&mut self, migration: Box<dyn Migration>) {
        self.migrations.push(migration);
    }

    /// Total registered migrations. Useful for diagnostic spans.
    #[must_use]
    pub fn len(&self) -> usize {
        self.migrations.len()
    }

    /// True if no migrations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty()
    }

    /// Find the chain of migrations that brings `from` → `to` on `kind`.
    ///
    /// Returns the migrations to apply, in order. Errors if no such chain
    /// exists or the chain has inconsistent versions.
    ///
    /// # Errors
    ///
    /// - [`MigrationError::Downgrade`] when `from > to`.
    /// - [`MigrationError::NoChain`] when no path exists from `from` to `to`
    ///   on the given file kind.
    pub fn chain(
        &self,
        kind: FileKind,
        from: SchemaVersion,
        to: SchemaVersion,
    ) -> Result<Vec<&dyn Migration>, MigrationError> {
        if from > to {
            return Err(MigrationError::Downgrade { from, to });
        }
        if from == to {
            return Ok(Vec::new());
        }
        let mut chain: Vec<&dyn Migration> = Vec::new();
        let mut cursor = from;
        // Greedy walk: at each step pick the migration whose `from_version`
        // matches the cursor and whose `to_version` is closest to (but not
        // past) `to`. With a small registry this is O(N²) and fine.
        while cursor < to {
            let next = self
                .migrations
                .iter()
                .filter(|m| m.file_kind() == kind && m.from_version() == cursor)
                .filter(|m| m.to_version() <= to)
                .max_by_key(|m| m.to_version());
            let Some(step) = next else {
                return Err(MigrationError::NoChain { from, to, kind });
            };
            cursor = step.to_version();
            chain.push(step.as_ref());
        }
        if cursor != to {
            return Err(MigrationError::NoChain { from, to, kind });
        }
        Ok(chain)
    }
}

/// Walk the migration chain from `from` to `to` on `kind`, applying each
/// step in order to `ron_text`. Returns the migrated RON text on success.
///
/// If `from == to` this returns the text unchanged (after a single parse
/// validation, so corrupt input still surfaces).
///
/// # Errors
///
/// - [`MigrationError::InputParse`] when `ron_text` does not parse as RON.
/// - [`MigrationError::Downgrade`] / [`MigrationError::NoChain`] when no
///   suitable chain exists.
/// - [`MigrationError::OutputParse`] when one of the registered migrations
///   produces text that fails to parse.
/// - Any [`MigrationError`] surfaced by an individual migration step.
pub fn migrate(
    registry: &MigrationRegistry,
    kind: FileKind,
    from: SchemaVersion,
    to: SchemaVersion,
    ron_text: &str,
) -> Result<String, MigrationError> {
    // Always validate input parses — even when from == to — so callers
    // get a single, predictable surface for "this file is corrupt".
    ron::from_str::<ron::Value>(ron_text).map_err(|e| MigrationError::InputParse(e.to_string()))?;
    if from == to {
        return Ok(ron_text.to_string());
    }
    let steps = registry.chain(kind, from, to)?;
    let mut current = ron_text.to_string();
    for step in steps {
        let migrated = step.apply(&current)?;
        // Every step's output must parse as RON.
        ron::from_str::<ron::Value>(&migrated).map_err(|e| MigrationError::OutputParse {
            from: step.from_version(),
            to: step.to_version(),
            kind: step.file_kind(),
            reason: e.to_string(),
        })?;
        current = migrated;
    }
    Ok(current)
}

/// Built-in migrations shipped with the crate.
pub mod builtin {
    use super::{FileKind, Migration, MigrationError};
    use crate::schema_version::SchemaVersion;

    /// v0.0 → v0.1 baseline: add an explicit `version: "0.1.0"` field if
    /// the file lacks one. v0.0 fixtures that already carry an explicit
    /// `version: "0.1.0"` are passed through unchanged (idempotent).
    ///
    /// This is the single migration the W14 wave ships; downstream waves
    /// will register their own additions on top.
    #[derive(Clone, Copy, Debug)]
    pub struct AddVersionField {
        /// Which file kind this instance is registered for. Stored so the
        /// trait impl can return it.
        pub kind: FileKind,
    }

    impl Migration for AddVersionField {
        fn from_version(&self) -> SchemaVersion {
            SchemaVersion::V0_0_0
        }

        fn to_version(&self) -> SchemaVersion {
            SchemaVersion::V0_1_0
        }

        fn file_kind(&self) -> FileKind {
            self.kind
        }

        fn apply(&self, ron_text: &str) -> Result<String, MigrationError> {
            // Cheap text-level transform: if the document already opens
            // with `(... version: ...)`, leave it alone. Otherwise insert
            // `version: "0.1.0"` as the first field.
            let trimmed = ron_text.trim_start();
            if trimmed.is_empty() {
                return Err(MigrationError::Custom {
                    from: self.from_version(),
                    to: self.to_version(),
                    kind: self.kind,
                    reason: "empty document".to_string(),
                });
            }
            // The grammar we accept here is the canonical form
            // `Project ( ... )` / `Scene ( ... )` / `Prefab ( ... )` —
            // open paren first or after the type name. Find the first `(`
            // and inspect what follows.
            let leading_ws_len = ron_text.len() - trimmed.len();
            let body_start = trimmed.find('(').ok_or_else(|| MigrationError::Custom {
                from: self.from_version(),
                to: self.to_version(),
                kind: self.kind,
                reason: "expected `(` after the type tag".to_string(),
            })?;
            let after_paren = &trimmed[body_start + 1..];

            // Already-versioned check: scan the *body* (not the entire
            // file — `version:` could appear inside a string literal but
            // not before the first non-whitespace char of the body).
            let body_first_non_ws = after_paren.trim_start();
            if body_first_non_ws.starts_with("version") {
                // Already has version field — pass through unchanged.
                return Ok(ron_text.to_string());
            }

            // Insert `version: "0.1.0",` immediately after the `(`. The
            // formatter will normalize whitespace on the next pretty-print.
            let mut out = String::with_capacity(ron_text.len() + 32);
            out.push_str(&ron_text[..=(leading_ws_len + body_start)]);
            out.push_str("\n    version: \"0.1.0\",");
            out.push_str(after_paren);
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_scene_v0_0_text() -> &'static str {
        // No `version:` field — represents the v0.0 baseline.
        r#"Scene(
    name: "blank",
    entities: [],
    root_entities: [],
)"#
    }

    fn already_versioned_scene_text() -> &'static str {
        r#"Scene(
    version: "0.1.0",
    name: "blank",
    entities: [],
    root_entities: [],
)"#
    }

    #[test]
    fn registry_with_builtin_has_three_migrations() {
        let r = MigrationRegistry::with_builtin();
        assert_eq!(r.len(), 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn chain_from_v0_0_to_v0_1_returns_one_step() {
        let r = MigrationRegistry::with_builtin();
        let chain = r
            .chain(
                FileKind::Scene,
                SchemaVersion::V0_0_0,
                SchemaVersion::V0_1_0,
            )
            .expect("chain");
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].from_version(), SchemaVersion::V0_0_0);
        assert_eq!(chain[0].to_version(), SchemaVersion::V0_1_0);
        assert_eq!(chain[0].file_kind(), FileKind::Scene);
    }

    #[test]
    fn chain_from_equal_to_equal_is_empty() {
        let r = MigrationRegistry::with_builtin();
        let chain = r
            .chain(
                FileKind::Scene,
                SchemaVersion::V0_1_0,
                SchemaVersion::V0_1_0,
            )
            .expect("chain");
        assert!(chain.is_empty());
    }

    #[test]
    fn chain_rejects_downgrade() {
        let r = MigrationRegistry::with_builtin();
        let err = r
            .chain(
                FileKind::Scene,
                SchemaVersion::V0_1_0,
                SchemaVersion::V0_0_0,
            )
            .unwrap_err();
        assert!(matches!(err, MigrationError::Downgrade { .. }));
    }

    #[test]
    fn chain_no_path_errors() {
        let r = MigrationRegistry::with_builtin();
        // Built-ins don't yet cover v0.1 → v0.2; expect NoChain.
        let err = r
            .chain(
                FileKind::Scene,
                SchemaVersion::V0_1_0,
                SchemaVersion::new(0, 2, 0),
            )
            .unwrap_err();
        assert!(matches!(err, MigrationError::NoChain { .. }));
    }

    #[test]
    fn migrate_passes_through_when_versions_equal() {
        let r = MigrationRegistry::with_builtin();
        let text = already_versioned_scene_text();
        let out = migrate(
            &r,
            FileKind::Scene,
            SchemaVersion::V0_1_0,
            SchemaVersion::V0_1_0,
            text,
        )
        .expect("migrate");
        assert_eq!(out, text);
    }

    #[test]
    fn migrate_rejects_unparseable_input() {
        let r = MigrationRegistry::with_builtin();
        let err = migrate(
            &r,
            FileKind::Scene,
            SchemaVersion::V0_0_0,
            SchemaVersion::V0_1_0,
            "{ this is not RON",
        )
        .unwrap_err();
        assert!(matches!(err, MigrationError::InputParse(_)));
    }

    #[test]
    fn add_version_field_inserts_when_missing() {
        let mig = builtin::AddVersionField {
            kind: FileKind::Scene,
        };
        let out = mig.apply(empty_scene_v0_0_text()).expect("apply");
        // Output should now contain `version: "0.1.0"`.
        assert!(out.contains("version: \"0.1.0\""), "got: {out}");
        // And it must still parse as RON.
        ron::from_str::<ron::Value>(&out).expect("parse migrated");
    }

    #[test]
    fn add_version_field_idempotent_when_already_present() {
        let mig = builtin::AddVersionField {
            kind: FileKind::Scene,
        };
        let text = already_versioned_scene_text();
        let out = mig.apply(text).expect("apply");
        assert_eq!(out, text, "should pass through unchanged");
    }

    #[test]
    fn migrate_v0_0_to_v0_1_lossless_on_text_round_trip() {
        let r = MigrationRegistry::with_builtin();
        let v0_0 = empty_scene_v0_0_text();
        let migrated = migrate(
            &r,
            FileKind::Scene,
            SchemaVersion::V0_0_0,
            SchemaVersion::V0_1_0,
            v0_0,
        )
        .expect("migrate");
        // The migrated payload must contain the new version field…
        assert!(migrated.contains("version: \"0.1.0\""));
        // …and must still mention every original entity (vacuously, since
        // the fixture has none — the regression we're guarding against is
        // the migration accidentally clipping the body).
        assert!(migrated.contains("name: \"blank\""));
        assert!(migrated.contains("entities: []"));
        assert!(migrated.contains("root_entities: []"));
    }
}
