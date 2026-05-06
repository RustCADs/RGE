// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// CI lint: every vendored *base* theme passes WCAG AA on the
// canonical fg/bg pairs. Variant overlays are tested against their
// natural base.

use std::path::PathBuf;

use rge_ui_theme::ThemeRegistry;

fn assets_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("assets");
    p.push("themes");
    p
}

fn audit_base(theme_name: &str) {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active(theme_name).unwrap();
    let failures = r.audit_contrast().unwrap();
    if !failures.is_empty() {
        let report = failures
            .iter()
            .map(|f| {
                format!(
                    "  - '{}' on '{}' = {:.2} (need >= 4.5)",
                    f.fg_token, f.bg_token, f.ratio
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "WCAG AA contrast failures on theme '{}':\n{}",
            theme_name, report
        );
    }
}

#[test]
fn dark_default_passes_wcag_aa() {
    audit_base("dark-default");
}

#[test]
fn light_default_passes_wcag_aa() {
    audit_base("light-default");
}

#[test]
fn studio_pro_passes_wcag_aa() {
    audit_base("studio-pro");
}

#[test]
fn daylight_passes_wcag_aa() {
    audit_base("daylight");
}

#[test]
fn high_contrast_overlay_passes_on_dark() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();
    let stack = rge_ui_theme::VariantStack::new().with(rge_ui_theme::VariantTag::Accessibility(
        rge_ui_theme::Accessibility::HighContrast,
    ));
    r.set_variants(stack);
    let failures = r.audit_contrast().unwrap();
    assert!(
        failures.is_empty(),
        "high-contrast overlay regressed AA on dark base"
    );
}
