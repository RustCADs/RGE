// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Theme schema migrations. Themes carry a `version:` field; when the
// loader encounters a file from an older schema it walks a chain of
// migrations to bring it up to `CURRENT_THEME_VERSION` before the
// merged registry sees it. Each migration is a small in-place
// rewrite:
//
//   * token rename (most common — `color.bg` → `color.background`)
//   * default-injection (synthesize a token if absent)
//   * deprecation (warn 2 minor versions before removal)
//
// The registry stores deprecations in `MigrationRegistry::warnings`
// after a load — callers can pump them into `kernel/diagnostics`.

use std::collections::BTreeMap;

use crate::theme::{Theme, CURRENT_THEME_VERSION};

/// One migration step. `from_version` is the schema-version the file
/// must currently have; the step rewrites it in place and bumps the
/// version on success.
#[derive(Clone)]
pub struct Migration {
    pub from_version: u32,
    pub to_version: u32,
    pub apply: fn(&mut Theme, &mut Vec<String>),
}

impl std::fmt::Debug for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Migration")
            .field("from_version", &self.from_version)
            .field("to_version", &self.to_version)
            .field("apply", &"<fn>")
            .finish()
    }
}

/// Registry of migration steps. Callers can extend it by pushing
/// custom migrations before running the loader.
#[derive(Debug)]
pub struct MigrationRegistry {
    pub steps: Vec<Migration>,
    /// Deprecation / rename warnings collected during the last
    /// `migrate()` call. Cleared at the start of each call.
    pub warnings: Vec<String>,
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new_with_builtins()
    }
}

impl MigrationRegistry {
    /// Empty registry. Useful for tests that want to install fake
    /// migrations without inheriting the built-in chain.
    pub fn empty() -> Self {
        Self {
            steps: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Registry pre-populated with the canonical migration chain for
    /// the in-tree theme schema.
    pub fn new_with_builtins() -> Self {
        let mut r = Self::empty();
        // V0 → V1 — initial schema lift. v0 used `bg` / `fg` /
        // `accent` short names; v1 standardises on dotted namespaces.
        r.steps.push(Migration {
            from_version: 0,
            to_version: 1,
            apply: migrate_v0_to_v1,
        });
        r
    }

    /// Append a custom migration. Steps are tried in registration
    /// order; the first matching `from_version` runs. Order matters
    /// only when migrations share a `from_version` (last wins).
    pub fn push(&mut self, m: Migration) {
        self.steps.push(m);
    }

    /// Bring `theme` up to `CURRENT_THEME_VERSION`. Returns an error
    /// if the file is from a *newer* schema we don't understand.
    pub fn migrate(&mut self, theme: &mut Theme) -> Result<(), MigrationError> {
        self.warnings.clear();

        if theme.version > CURRENT_THEME_VERSION {
            return Err(MigrationError::FromFuture {
                file_version: theme.version,
                supported_max: CURRENT_THEME_VERSION,
            });
        }

        // Apply migrations until version reaches CURRENT.
        let mut bound = self.steps.len() + 4; // safety against infinite loops
        while theme.version < CURRENT_THEME_VERSION && bound > 0 {
            bound -= 1;
            let step = self
                .steps
                .iter()
                .find(|s| s.from_version == theme.version)
                .ok_or(MigrationError::NoPath {
                    from: theme.version,
                })?;
            (step.apply)(theme, &mut self.warnings);
            theme.version = step.to_version;
        }

        if theme.version != CURRENT_THEME_VERSION {
            return Err(MigrationError::Stuck { at: theme.version });
        }
        Ok(())
    }
}

/// Convenience: rename a single token, leaving its value intact.
/// Records a deprecation note in `warnings`.
pub fn rename_token(theme: &mut Theme, from: &str, to: &str, warnings: &mut Vec<String>) {
    let renames: Vec<(String, String)> = theme
        .tokens
        .keys()
        .filter(|k| *k == from)
        .map(|k| (k.clone(), to.to_string()))
        .collect();
    for (old, new) in renames {
        if let Some(v) = theme.tokens.remove(&old) {
            theme.tokens.insert(new.clone(), v);
            warnings.push(format!(
                "theme '{}' uses deprecated token '{}'; renamed to '{}'",
                theme.name, old, new
            ));
        }
    }
    // Also rewrite token-refs in styles.
    let mut style_renames: Vec<(String, BTreeMap<String, crate::style::Slot>)> = Vec::new();
    for (style_name, style) in &theme.styles {
        let mut updated = style.slots.clone();
        let mut changed = false;
        for v in updated.values_mut() {
            if let crate::style::Slot::TokenRef(name) = v {
                if name == from {
                    *name = to.to_string();
                    changed = true;
                }
            }
        }
        if changed {
            style_renames.push((style_name.clone(), updated));
        }
    }
    for (style_name, slots) in style_renames {
        if let Some(s) = theme.styles.get_mut(&style_name) {
            s.slots = slots;
        }
    }
}

/// Initial v0 → v1 step. Renames the legacy short names that
/// pre-architecture-freeze prototypes used.
fn migrate_v0_to_v1(theme: &mut Theme, warnings: &mut Vec<String>) {
    rename_token(theme, "bg", "color.background", warnings);
    rename_token(theme, "fg", "color.foreground", warnings);
    rename_token(theme, "accent", "color.accent", warnings);
    rename_token(theme, "panel", "color.panel", warnings);
    rename_token(theme, "surface", "color.surface", warnings);
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum MigrationError {
    #[error("theme is from a newer schema (file version {file_version}, supported up to {supported_max})")]
    FromFuture {
        file_version: u32,
        supported_max: u32,
    },

    #[error("no migration registered from version {from}")]
    NoPath { from: u32 },

    #[error("migration loop did not converge; stuck at version {at}")]
    Stuck { at: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Style;
    use crate::token::{Color, Token};

    #[test]
    fn migrates_v0_to_v1_and_renames() {
        let mut t = Theme::new("legacy");
        t.version = 0;
        t.tokens
            .insert("bg".into(), Token::Color(Color::from_srgb(0, 0, 0)));
        t.tokens
            .insert("accent".into(), Token::Color(Color::from_srgb(255, 0, 0)));

        let mut style = Style::new();
        style.set_ref("background", "bg");
        t.styles.insert("Button".into(), style);

        let mut reg = MigrationRegistry::new_with_builtins();
        reg.migrate(&mut t).unwrap();

        assert_eq!(t.version, CURRENT_THEME_VERSION);
        assert!(t.tokens.contains_key("color.background"));
        assert!(t.tokens.contains_key("color.accent"));
        assert!(!t.tokens.contains_key("bg"));
        match t.styles["Button"].get("background").unwrap() {
            crate::style::Slot::TokenRef(n) => assert_eq!(n, "color.background"),
            _ => panic!(),
        }
        assert!(reg.warnings.iter().any(|w| w.contains("'bg'")));
    }

    #[test]
    fn errors_on_future_version() {
        let mut t = Theme::new("future");
        t.version = CURRENT_THEME_VERSION + 1;
        let mut reg = MigrationRegistry::new_with_builtins();
        let err = reg.migrate(&mut t).unwrap_err();
        matches!(err, MigrationError::FromFuture { .. });
    }

    #[test]
    fn errors_on_no_path() {
        // Empty registry can't migrate v0.
        let mut t = Theme::new("orphan");
        t.version = 0;
        let mut reg = MigrationRegistry::empty();
        let err = reg.migrate(&mut t).unwrap_err();
        matches!(err, MigrationError::NoPath { from: 0 });
    }

    #[test]
    fn current_version_is_no_op() {
        let mut t = Theme::new("ok");
        t.version = CURRENT_THEME_VERSION;
        let mut reg = MigrationRegistry::new_with_builtins();
        reg.migrate(&mut t).unwrap();
        assert!(reg.warnings.is_empty());
    }
}
