// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// `Style` describes how a widget consumes tokens. A style is a flat
// map of slot names (`background`, `border`, `padding`, ...) to
// either a token reference (`"color.surface"`) or an inline literal
// (Color/Length/etc). The resolver walks the style, looks up the
// token in the active theme, and produces a concrete map of values
// that the renderer applies.
//
// Token resolution rules:
//   * `Slot::TokenRef("color.x")` — resolve through the active theme
//     (with extends-chain lookup), error if missing.
//   * `Slot::Literal(token)` — use the embedded value directly.
//
// Falling back to a literal is useful for one-off overrides; keeping
// it on the `Slot` enum (instead of allowing arbitrary tokens in any
// resolved field) keeps the lint-friendly invariant that "the theme
// owns the token vocabulary".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::token::Token;

/// One slot value in a style. Either a named reference or an inline
/// literal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Slot {
    /// Look up `Token` in the active theme by name.
    TokenRef(String),
    /// Inline override. Skips the registry.
    Literal(Token),
}

/// Widget style: a map of named slots to resolvable values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Slot map. Stable ordering for predictable RON output.
    #[serde(default)]
    pub slots: BTreeMap<String, Slot>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_ref(&mut self, slot: impl Into<String>, token_name: impl Into<String>) {
        self.slots
            .insert(slot.into(), Slot::TokenRef(token_name.into()));
    }

    pub fn set_literal(&mut self, slot: impl Into<String>, token: Token) {
        self.slots.insert(slot.into(), Slot::Literal(token));
    }

    pub fn get(&self, slot: &str) -> Option<&Slot> {
        self.slots.get(slot)
    }
}

/// Resolved style: every slot is now a concrete `Token` value (no
/// `TokenRef` remains). Output of `ThemeRegistry::resolve_style`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedStyle {
    pub values: BTreeMap<String, Token>,
}

impl ResolvedStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, slot: &str) -> Option<&Token> {
        self.values.get(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Color;

    #[test]
    fn slot_set_and_get() {
        let mut s = Style::new();
        s.set_ref("background", "color.surface");
        s.set_literal("border", Token::Color(Color::from_srgb(0, 0, 0)));
        match s.get("background").unwrap() {
            Slot::TokenRef(n) => assert_eq!(n, "color.surface"),
            _ => panic!(),
        }
        match s.get("border").unwrap() {
            Slot::Literal(_) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn style_serde_round_trip() {
        let mut s = Style::new();
        s.set_ref("background", "color.surface");
        let txt = ron::to_string(&s).unwrap();
        let back: Style = ron::from_str(&txt).unwrap();
        assert_eq!(s, back);
    }
}
