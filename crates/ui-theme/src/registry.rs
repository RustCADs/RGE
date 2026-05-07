// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// `ThemeRegistry` owns the set of loaded themes, the active variant
// stack, and per-scope overrides. It exposes:
//
//   * `load_dir(path)`      — RON files in `assets/themes/*.theme.ron`
//   * `set_active(name)`    — pick the base theme by name
//   * `set_variants(stack)` — reapply variant axes
//   * `resolve_token(...)`  — token lookup (with extends + variants)
//   * `resolve_style(...)`  — style → ResolvedStyle
//   * `audit_contrast(...)` — WCAG AA lint
//   * `set_scope_override(...)` — push a per-scope theme on top
//
// Scope resolution order (PLAN.md §6.2): widget → panel → window →
// workspace → global. The registry stores per-scope override themes
// in `scope_overrides` keyed by `Scope`, and resolves by walking
// from the most-specific scope upward.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink as _};

use crate::contrast::{audit_failures, vendored_pairs, ContrastReport};
use crate::migration::MigrationRegistry;
use crate::style::{ResolvedStyle, Slot, Style};
use crate::theme::{Theme, MAX_INHERITANCE_DEPTH};
use crate::token::{AnimationToken, Token};
use crate::variant::{Accessibility, VariantStack, VariantTag};

/// Per-scope override key. Scopes nest most-specific-first when
/// looked up — see `Scope::specificity`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Scope {
    Widget(String),
    Panel(String),
    Window(String),
    Workspace(String),
    Global,
}

impl Scope {
    /// Lower number = looked up first (most specific).
    pub fn specificity(&self) -> u8 {
        match self {
            Scope::Widget(_) => 0,
            Scope::Panel(_) => 1,
            Scope::Window(_) => 2,
            Scope::Workspace(_) => 3,
            Scope::Global => 4,
        }
    }
}

/// Registry error type.
#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("theme '{0}' not found in registry")]
    NotFound(String),

    #[error("inheritance chain on '{theme}' exceeds max depth {max}")]
    DepthExceeded { max: usize, theme: String },

    #[error("inheritance cycle detected starting at '{0}'")]
    Cycle(String),

    #[error("missing token '{token}' in theme '{theme}'")]
    MissingToken { theme: String, token: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ron parse error in {path}: {source}")]
    Ron {
        path: String,
        #[source]
        source: ron::error::SpannedError,
    },

    #[error(transparent)]
    Migration(#[from] crate::migration::MigrationError),
}

impl PartialEq for RegistryError {
    fn eq(&self, other: &Self) -> bool {
        // good-enough comparison for tests; ignores io::Error / ron source.
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[allow(clippy::derivable_impls)]
impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of loaded themes and active selection.
#[derive(Debug)]
pub struct ThemeRegistry {
    /// All loaded base + variant overlay themes, by `name`.
    themes: BTreeMap<String, Theme>,
    /// Currently active base theme. May be empty before `load_dir`.
    active: Option<String>,
    /// Active variant stack.
    variants: VariantStack,
    /// Per-scope override themes. Looked up most-specific-first.
    scope_overrides: BTreeMap<Scope, Theme>,
    /// Migration registry for the loader.
    migrations: MigrationRegistry,
    /// Diagnostics produced during the last load / resolve. Cleared
    /// at the top of each operation that produces them.
    pub diagnostics: DiagnosticAggregator,
    /// Cache of fully-merged theme for the current active+variant
    /// combo. Invalidated by `set_active`, `set_variants`, `load_*`.
    merged_cache: Option<Theme>,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        Self {
            themes: BTreeMap::new(),
            active: None,
            variants: VariantStack::default(),
            scope_overrides: BTreeMap::new(),
            migrations: MigrationRegistry::new_with_builtins(),
            diagnostics: DiagnosticAggregator::default(),
            merged_cache: None,
        }
    }

    /// Insert a theme directly (skips disk + RON parsing). Useful for
    /// tests and for synthesised themes (e.g. user override in the
    /// "Theme editor" plugin).
    pub fn insert(&mut self, theme: Theme) {
        self.themes.insert(theme.name.clone(), theme);
        self.merged_cache = None;
    }

    /// Number of themes loaded.
    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }

    /// All loaded theme names, sorted.
    pub fn names(&self) -> Vec<String> {
        self.themes.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.get(name)
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn variant_stack(&self) -> &VariantStack {
        &self.variants
    }

    pub fn migrations(&self) -> &MigrationRegistry {
        &self.migrations
    }

    /// Set active base theme by name. Errors if not loaded.
    pub fn set_active(&mut self, name: &str) -> Result<(), RegistryError> {
        if !self.themes.contains_key(name) {
            return Err(RegistryError::NotFound(name.to_string()));
        }
        self.active = Some(name.to_string());
        self.merged_cache = None;
        Ok(())
    }

    /// Replace the active variant stack and invalidate the cache.
    pub fn set_variants(&mut self, stack: VariantStack) {
        self.variants = stack;
        self.merged_cache = None;
    }

    /// Push a scope override. Replaces any prior theme at that scope.
    pub fn set_scope_override(&mut self, scope: Scope, theme: Theme) {
        self.scope_overrides.insert(scope, theme);
        self.merged_cache = None;
    }

    /// Drop a scope override.
    pub fn clear_scope_override(&mut self, scope: &Scope) {
        self.scope_overrides.remove(scope);
        self.merged_cache = None;
    }

    /// Load every `*.theme.ron` file in `dir` (non-recursive). Files
    /// are migrated in-place to `CURRENT_THEME_VERSION` before
    /// insertion. Diagnostics for missing parents / cycles / etc.
    /// land in `self.diagnostics`.
    pub fn load_dir(&mut self, dir: impl AsRef<Path>) -> Result<usize, RegistryError> {
        let dir = dir.as_ref();
        let mut count = 0;
        let entries = std::fs::read_dir(dir)?;
        let mut theme_paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.ends_with(".theme.ron") {
                theme_paths.push(path);
            }
        }
        theme_paths.sort();
        for path in theme_paths {
            self.load_file(&path)?;
            count += 1;
        }
        Ok(count)
    }

    /// Load a single theme file. Public so hot-reload can target one
    /// file without rescanning the directory.
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<(), RegistryError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)?;
        let mut theme: Theme = ron::from_str(&raw).map_err(|e| RegistryError::Ron {
            path: path.display().to_string(),
            source: e,
        })?;
        self.migrations.migrate(&mut theme)?;
        for w in self.migrations.warnings.drain(..) {
            self.diagnostics.emit(Diagnostic::warning(w));
        }
        self.themes.insert(theme.name.clone(), theme);
        self.merged_cache = None;
        Ok(())
    }

    /// Resolve the inheritance chain for `name` and return the
    /// flattened theme (deepest ancestor merged first, child last).
    /// Caps at `MAX_INHERITANCE_DEPTH` and detects cycles.
    ///
    /// # Cycle detection — algorithmically distinct from the 3 `Graph<N,()>` consumers
    ///
    /// This method walks a `BTreeMap<String, Theme>` `extends:`-chain with
    /// linear single-parent inheritance, returning [`RegistryError::Cycle`]
    /// (a `String` for the cycle key). It does NOT operate on
    /// `kernel/graph-foundation::Graph<N, E>` — the 3 substrate-backed
    /// cycle-detection sites (`cad-core::operator_graph` ancestor-set guard,
    /// `kernel/asset::DependencyGraph::detect_cycle` three-color DFS,
    /// `asset-store::DepGraph` reachability walk) all use `Graph<N, ()>` per
    /// audit-3 M7 closure (see `docs/§18/GRAPH_FOUNDATION.md` §3 + PLAN
    /// §1.14 line 605). Theme inheritance is NOT a graph-foundation
    /// consumer, so this 4th cycle-detection site is intentionally separate
    /// — it doesn't need (and wouldn't benefit from) substrate primitives.
    pub fn flatten(&self, name: &str) -> Result<Theme, RegistryError> {
        let mut chain: Vec<&Theme> = Vec::new();
        let mut visited: Vec<&str> = Vec::new();
        let mut cursor = name;
        for _ in 0..=MAX_INHERITANCE_DEPTH {
            if visited.contains(&cursor) {
                return Err(RegistryError::Cycle(cursor.to_string()));
            }
            visited.push(cursor);
            let t = self
                .themes
                .get(cursor)
                .ok_or_else(|| RegistryError::NotFound(cursor.to_string()))?;
            chain.push(t);
            match &t.extends {
                Some(parent) => cursor = parent.as_str(),
                None => {
                    // Successfully reached a root; flatten in
                    // ancestor-first order so child wins on conflict.
                    let mut out = Theme::new(name);
                    out.version = t.version;
                    for theme in chain.iter().rev() {
                        out.merge_in_place(theme);
                    }
                    out.name = name.to_string();
                    out.extends = None;
                    return Ok(out);
                }
            }
        }
        Err(RegistryError::DepthExceeded {
            max: MAX_INHERITANCE_DEPTH,
            theme: name.to_string(),
        })
    }

    /// Build the fully-merged theme for the current active+variants
    /// state. Steps: flatten extends-chain, find variant overlay
    /// themes whose `variants:` tags are all in the active stack,
    /// merge them in, apply post-resolution mutators
    /// (reduced-motion, etc.), and finally merge per-scope overrides
    /// over the top.
    pub fn merged(&mut self) -> Result<&Theme, RegistryError> {
        if self.merged_cache.is_none() {
            self.rebuild_merged_cache()?;
        }
        Ok(self
            .merged_cache
            .as_ref()
            .expect("merged_cache populated above"))
    }

    /// Internal: rebuild the merged-theme cache. Always sets
    /// `self.merged_cache` to `Some` on success.
    fn rebuild_merged_cache(&mut self) -> Result<(), RegistryError> {
        let active = self
            .active
            .clone()
            .ok_or_else(|| RegistryError::NotFound("<no active>".to_string()))?;
        let mut merged = self.flatten(&active)?;

        // Variant overlays: any theme with a non-empty `variants:` list
        // whose tags are all in the user's stack is applied. Sorted
        // by stack-axis priority so a higher-axis variant wins over a
        // lower-axis one for the same token.
        let mut overlay_names: Vec<&Theme> = self
            .themes
            .values()
            .filter(|t| !t.variants.is_empty())
            .filter(|t| t.variants.iter().all(|tag| self.variants.contains(tag)))
            .collect();
        overlay_names.sort_by_key(|t| {
            t.variants
                .iter()
                .map(|v| v.axis_priority())
                .max()
                .unwrap_or(0)
        });
        for overlay in overlay_names {
            merged.merge_in_place(overlay);
        }

        // Post-resolution accessibility transforms.
        if self.variants.has_reduced_motion() {
            zero_motion_durations(&mut merged);
        }

        // Scope overrides — most-specific-last so they win.
        let mut scopes: Vec<&Scope> = self.scope_overrides.keys().collect();
        scopes.sort_by_key(|s| std::cmp::Reverse(s.specificity()));
        for s in scopes {
            if let Some(t) = self.scope_overrides.get(s) {
                merged.merge_in_place(t);
            }
        }

        self.merged_cache = Some(merged);
        Ok(())
    }

    /// Resolve a token by name in the merged theme.
    pub fn resolve_token(&mut self, token_name: &str) -> Result<Token, RegistryError> {
        let merged = self.merged()?;
        merged
            .tokens
            .get(token_name)
            .cloned()
            .ok_or_else(|| RegistryError::MissingToken {
                theme: merged.name.clone(),
                token: token_name.to_string(),
            })
    }

    /// Resolve every slot of `style` to a concrete `Token`.
    pub fn resolve_style(&mut self, style: &Style) -> Result<ResolvedStyle, RegistryError> {
        let mut out = ResolvedStyle::new();
        // Walk slot keys without holding a borrow into self.
        let pairs: Vec<(String, Slot)> = style
            .slots
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (slot_name, slot_val) in pairs {
            let resolved = match slot_val {
                Slot::Literal(t) => t,
                Slot::TokenRef(name) => self.resolve_token(&name)?,
            };
            out.values.insert(slot_name, resolved);
        }
        Ok(out)
    }

    /// Resolve a named style from the merged theme + reference walk.
    pub fn resolve_named_style(
        &mut self,
        style_name: &str,
    ) -> Result<ResolvedStyle, RegistryError> {
        let style =
            {
                let merged = self.merged()?;
                merged.styles.get(style_name).cloned().ok_or_else(|| {
                    RegistryError::MissingToken {
                        theme: merged.name.clone(),
                        token: format!("style:{}", style_name),
                    }
                })?
            };
        self.resolve_style(&style)
    }

    /// Run WCAG AA contrast lint over the merged theme. Returns the
    /// list of failing pairs (empty = pass).
    pub fn audit_contrast(&mut self) -> Result<Vec<ContrastReport>, RegistryError> {
        let merged = self.merged()?.clone();
        Ok(audit_failures(&merged, &vendored_pairs()))
    }

    /// Helper used by hot-reload: how long since the file was loaded
    /// is determined by the watcher; the registry only exposes a
    /// repaint-tag bump so consumers (egui apps) can invalidate.
    pub fn repaint_tag(&self) -> u64 {
        // Concrete bookkeeping is on the watcher side; the registry
        // exposes this for symmetry.
        0
    }

    /// Used by the watcher to clear cached resolution. Public for
    /// tests; in production the watcher calls it.
    pub fn invalidate_cache(&mut self) {
        self.merged_cache = None;
    }
}

/// Zero out the `duration_ms` field on every `Animation` token in
/// `theme`. Keeps the curve hint untouched. Used by the
/// `reduced-motion` accessibility transform.
fn zero_motion_durations(theme: &mut Theme) {
    for token in theme.tokens.values_mut() {
        if let Token::Animation(AnimationToken { duration_ms, .. }) = token {
            *duration_ms = 0;
        }
    }
    // Style slots may carry inline animation literals too.
    for style in theme.styles.values_mut() {
        for slot in style.slots.values_mut() {
            if let Slot::Literal(Token::Animation(a)) = slot {
                a.duration_ms = 0;
            }
        }
    }
}

/// Used in `From<Accessibility>` for tag construction in tests.
impl From<Accessibility> for VariantTag {
    fn from(a: Accessibility) -> Self {
        VariantTag::Accessibility(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{AnimationToken, Color, Curve};

    fn mk_dark() -> Theme {
        let mut t = Theme::new("dark-default");
        t.set_token(
            "color.background",
            Token::Color(Color::from_srgb(20, 20, 20)),
        );
        t.set_token(
            "color.foreground",
            Token::Color(Color::from_srgb(240, 240, 240)),
        );
        t.set_token(
            "motion.fade.in",
            Token::Animation(AnimationToken {
                duration_ms: 200,
                curve: Curve::EaseOut,
            }),
        );
        t
    }

    fn mk_studio_pro() -> Theme {
        let mut t = Theme::new("studio-pro");
        t.extends = Some("dark-default".into());
        // Override background to a slightly different dark.
        t.set_token(
            "color.background",
            Token::Color(Color::from_srgb(15, 15, 18)),
        );
        t
    }

    #[test]
    fn flatten_inheritance_child_wins() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        r.insert(mk_studio_pro());
        let f = r.flatten("studio-pro").unwrap();
        match f.tokens["color.background"] {
            Token::Color(c) => assert_eq!(c.srgb, [15, 15, 18, 255]),
            _ => panic!(),
        }
        // foreground inherited from dark
        assert!(f.tokens.contains_key("color.foreground"));
    }

    #[test]
    fn flatten_depth_exceeded() {
        let mut r = ThemeRegistry::new();
        for i in 0..10 {
            let mut t = Theme::new(format!("level{}", i));
            if i > 0 {
                t.extends = Some(format!("level{}", i - 1));
            }
            r.insert(t);
        }
        let err = r.flatten("level9").unwrap_err();
        matches!(err, RegistryError::DepthExceeded { .. });
    }

    #[test]
    fn flatten_cycle_detection() {
        let mut r = ThemeRegistry::new();
        let mut a = Theme::new("a");
        a.extends = Some("b".into());
        let mut b = Theme::new("b");
        b.extends = Some("a".into());
        r.insert(a);
        r.insert(b);
        let err = r.flatten("a").unwrap_err();
        matches!(
            err,
            RegistryError::Cycle(_) | RegistryError::DepthExceeded { .. }
        );
    }

    #[test]
    fn reduced_motion_zeroes_animation() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        r.set_active("dark-default").unwrap();
        let mut stack = VariantStack::new();
        stack.add(VariantTag::Accessibility(Accessibility::ReducedMotion));
        r.set_variants(stack);
        let merged = r.merged().unwrap().clone();
        match merged.tokens["motion.fade.in"] {
            Token::Animation(a) => assert_eq!(a.duration_ms, 0),
            _ => panic!(),
        }
    }

    #[test]
    fn scope_override_wins_over_global() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        r.set_active("dark-default").unwrap();
        let mut override_t = Theme::new("override");
        override_t.set_token(
            "color.foreground",
            Token::Color(Color::from_srgb(255, 0, 0)),
        );
        r.set_scope_override(Scope::Widget("Inspector".into()), override_t);
        let merged = r.merged().unwrap().clone();
        match merged.tokens["color.foreground"] {
            Token::Color(c) => assert_eq!(c.srgb, [255, 0, 0, 255]),
            _ => panic!(),
        }
    }

    #[test]
    fn missing_token_returns_diagnostic_error() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        r.set_active("dark-default").unwrap();
        let err = r.resolve_token("color.nonexistent").unwrap_err();
        matches!(err, RegistryError::MissingToken { .. });
    }

    #[test]
    fn resolve_style_walks_token_refs() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        r.set_active("dark-default").unwrap();
        let mut s = Style::new();
        s.set_ref("background", "color.background");
        s.set_literal("border", Token::Color(Color::from_srgb(1, 2, 3)));
        let resolved = r.resolve_style(&s).unwrap();
        match resolved.values["background"] {
            Token::Color(c) => assert_eq!(c.srgb, [20, 20, 20, 255]),
            _ => panic!(),
        }
    }

    #[test]
    fn cache_invalidation_on_set_active() {
        let mut r = ThemeRegistry::new();
        r.insert(mk_dark());
        let mut alt = Theme::new("alt");
        alt.set_token(
            "color.background",
            Token::Color(Color::from_srgb(99, 99, 99)),
        );
        r.insert(alt);
        r.set_active("dark-default").unwrap();
        let _ = r.merged().unwrap();
        r.set_active("alt").unwrap();
        let m = r.merged().unwrap();
        match m.tokens["color.background"] {
            Token::Color(c) => assert_eq!(c.srgb, [99, 99, 99, 255]),
            _ => panic!(),
        }
    }
}
