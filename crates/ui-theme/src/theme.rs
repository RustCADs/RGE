// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// `Theme` is the top-level RON-deserializable unit. Files in
// `assets/themes/*.theme.ron` parse into one of these.
//
// Wire format
// -----------
// ```ron
// (
//     name: "dark-default",
//     version: 1,
//     extends: None,
//     variants: [],
//     tokens: {
//         "color.background": Color(...),
//         "length.spacing.md": Length(Px(8.0)),
//     },
//     styles: {
//         "Button": (background: "color.surface", ...)
//     },
// )
// ```
//
// `extends:` is the inheritance pointer (max chain depth 3, enforced
// by the registry). `variants:` tags overlay themes — when a base
// theme is loaded with `[Scheme(Dark), Accessibility(HighContrast)]`
// in its variants list it is merged on top of any base it extends
// when those tags appear in the user's `VariantStack`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::style::Style;
use crate::token::Token;
use crate::variant::VariantTag;

/// Currently-supported max version. Loader emits a deprecation warning
/// when it migrates from older versions and an error if the file is
/// from a newer one.
pub const CURRENT_THEME_VERSION: u32 = 1;

/// Maximum allowed length of an `extends:` chain. Per PLAN.md §6.2 we
/// cap at 3 to keep token resolution traceable.
pub const MAX_INHERITANCE_DEPTH: usize = 3;

/// On-disk theme document. Every file in `assets/themes/` parses to
/// one of these.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Stable theme identifier, also the file stem.
    pub name: String,

    /// Schema version of the file. The migration registry runs
    /// against this on load.
    #[serde(default = "default_version")]
    pub version: u32,

    /// Optional parent theme — token lookups walk up the chain.
    #[serde(default)]
    pub extends: Option<String>,

    /// Variant tags this theme applies under. Empty for the base; an
    /// overlay theme with `[Scheme(Dark)]` activates only when the
    /// user picks `dark`.
    #[serde(default)]
    pub variants: Vec<VariantTag>,

    /// Token map. Keys follow the dotted-namespace convention
    /// (`color.bg.panel`, `length.spacing.md`, `motion.fade.in`).
    /// `BTreeMap` so RON output is stable.
    #[serde(default)]
    pub tokens: BTreeMap<String, Token>,

    /// Style map. Keys are style identifiers (typically widget
    /// names: `Button`, `Toolbar.Tab`).
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
}

fn default_version() -> u32 {
    CURRENT_THEME_VERSION
}

impl Theme {
    /// Construct an empty theme with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: CURRENT_THEME_VERSION,
            extends: None,
            variants: Vec::new(),
            tokens: BTreeMap::new(),
            styles: BTreeMap::new(),
        }
    }

    /// Insert or replace a token by name.
    pub fn set_token(&mut self, name: impl Into<String>, token: Token) {
        self.tokens.insert(name.into(), token);
    }

    /// Insert or replace a style by name.
    pub fn set_style(&mut self, name: impl Into<String>, style: Style) {
        self.styles.insert(name.into(), style);
    }

    /// Token lookup local to this theme (does not walk `extends`).
    pub fn local_token(&self, name: &str) -> Option<&Token> {
        self.tokens.get(name)
    }

    /// Style lookup local to this theme (does not walk `extends`).
    pub fn local_style(&self, name: &str) -> Option<&Style> {
        self.styles.get(name)
    }

    /// Merge `other` on top of `self`. Tokens and styles in `other`
    /// take precedence; values present only in `self` are preserved.
    /// Does not touch `extends` / `variants` / `name` / `version`.
    pub fn merge_in_place(&mut self, other: &Theme) {
        for (k, v) in &other.tokens {
            self.tokens.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.styles {
            self.styles.insert(k.clone(), v.clone());
        }
    }

    /// Pretty-print to RON for hand-editable theme files.
    pub fn to_ron_pretty(&self) -> Result<String, ron::Error> {
        let cfg = ron::ser::PrettyConfig::new()
            .depth_limit(8)
            .indentor("    ".to_string());
        ron::ser::to_string_pretty(self, cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{Color, Length};

    #[test]
    fn default_version_is_current() {
        let t = Theme::new("x");
        assert_eq!(t.version, CURRENT_THEME_VERSION);
    }

    #[test]
    fn merge_takes_other_on_conflict() {
        let mut a = Theme::new("a");
        a.set_token("color.bg", Token::Color(Color::from_srgb(0, 0, 0)));
        a.set_token("color.fg", Token::Color(Color::from_srgb(255, 255, 255)));

        let mut b = Theme::new("b");
        b.set_token("color.bg", Token::Color(Color::from_srgb(50, 50, 50)));

        a.merge_in_place(&b);
        match a.tokens["color.bg"] {
            Token::Color(c) => assert_eq!(c.srgb, [50, 50, 50, 255]),
            _ => panic!("wrong token"),
        }
        // unaffected keys preserved
        assert!(a.tokens.contains_key("color.fg"));
    }

    #[test]
    fn ron_round_trip_preserves_tokens() {
        let mut t = Theme::new("rt");
        t.set_token("length.gap", Token::Length(Length::Px(4.0)));
        let s = t.to_ron_pretty().unwrap();
        let back: Theme = ron::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn ron_minimal_omits_optional_fields() {
        // Tolerant deserialise from a minimal file with no extends /
        // variants / styles.
        let src = r#"(
            name: "min",
            version: 1,
            tokens: {},
        )"#;
        let t: Theme = ron::from_str(src).unwrap();
        assert_eq!(t.name, "min");
        assert!(t.extends.is_none());
        assert!(t.variants.is_empty());
        assert!(t.styles.is_empty());
    }
}
