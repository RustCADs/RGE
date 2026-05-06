// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// WCAG 2.x contrast ratio + AA / AAA pass tests. Used by the
// vendored-themes lint and surfaced via `ThemeRegistry::audit_contrast`
// so CI fails when a theme drops below AA on its critical
// foreground/background pairs.

use crate::theme::Theme;
use crate::token::{Color, Token};

/// Standard WCAG 2.x AA threshold for normal text.
pub const AA_NORMAL: f32 = 4.5;
/// AA threshold for large text (>= 18pt or 14pt bold).
pub const AA_LARGE: f32 = 3.0;
/// AAA threshold for normal text — used as informational only.
pub const AAA_NORMAL: f32 = 7.0;

/// WCAG contrast ratio for two colours (alpha ignored).
///
/// Defined as (L1 + 0.05) / (L2 + 0.05) where L1 is the lighter and
/// L2 the darker relative-luminance.
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let la = a.relative_luminance();
    let lb = b.relative_luminance();
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// Result for a single foreground/background pair.
#[derive(Clone, Debug, PartialEq)]
pub struct ContrastReport {
    pub fg_token: String,
    pub bg_token: String,
    pub ratio: f32,
    pub passes_aa_normal: bool,
    pub passes_aa_large: bool,
    pub passes_aaa_normal: bool,
}

/// Audit a list of fg/bg token pairs against AA normal-text. Returns
/// a list of failures (empty = pass). Pairs whose tokens are missing
/// or are not `Token::Color` are skipped silently — the registry's
/// missing-token diagnostics handle them.
pub fn audit(theme: &Theme, pairs: &[(&str, &str)]) -> Vec<ContrastReport> {
    let mut reports = Vec::new();
    for (fg_name, bg_name) in pairs {
        let (Some(fg_tok), Some(bg_tok)) = (theme.tokens.get(*fg_name), theme.tokens.get(*bg_name))
        else {
            continue;
        };
        let (Token::Color(fg), Token::Color(bg)) = (fg_tok, bg_tok) else {
            continue;
        };
        let r = contrast_ratio(*fg, *bg);
        reports.push(ContrastReport {
            fg_token: (*fg_name).to_string(),
            bg_token: (*bg_name).to_string(),
            ratio: r,
            passes_aa_normal: r >= AA_NORMAL,
            passes_aa_large: r >= AA_LARGE,
            passes_aaa_normal: r >= AAA_NORMAL,
        });
    }
    reports
}

/// Convenience: only the failing reports.
pub fn audit_failures(theme: &Theme, pairs: &[(&str, &str)]) -> Vec<ContrastReport> {
    audit(theme, pairs)
        .into_iter()
        .filter(|r| !r.passes_aa_normal)
        .collect()
}

/// The set of fg/bg pairs every vendored theme must declare and
/// pass against. Critical text/background combos only — not every
/// possible combination.
pub fn vendored_pairs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("color.foreground", "color.background"),
        ("color.foreground", "color.surface"),
        ("color.foreground", "color.panel"),
        ("color.text.muted", "color.background"),
        ("color.accent.on", "color.accent"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_white_max_contrast() {
        let r = contrast_ratio(Color::from_srgb(0, 0, 0), Color::from_srgb(255, 255, 255));
        // exact ratio is 21:1.
        assert!((r - 21.0).abs() < 0.05);
    }

    #[test]
    fn same_colour_min_contrast() {
        let r = contrast_ratio(
            Color::from_srgb(128, 128, 128),
            Color::from_srgb(128, 128, 128),
        );
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn aa_threshold_check() {
        // Mid grey on white — borderline.
        let r = contrast_ratio(
            Color::from_srgb(118, 118, 118),
            Color::from_srgb(255, 255, 255),
        );
        assert!(r >= AA_NORMAL); // 4.5+ at 118
    }
}
