// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Integration test: variant stacking. Verifies that `dark +
// high-contrast + protanopia + reduced-transparency` resolves to
// the right merged token set (overlays applied; reduced-motion
// post-mutator runs when present).

use std::path::PathBuf;

use rge_ui_theme::{
    Accessibility, AnimationToken, ColorBlind, Curve, ThemeRegistry, Token, VariantStack,
    VariantTag,
};

fn assets_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("assets");
    p.push("themes");
    p
}

#[test]
fn variant_stack_resolves_overlays() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();

    let stack = VariantStack::new()
        .with(VariantTag::Accessibility(Accessibility::HighContrast))
        .with(VariantTag::ColorBlind(ColorBlind::Protanopia))
        .with(VariantTag::Accessibility(
            Accessibility::ReducedTransparency,
        ));
    r.set_variants(stack);

    // high-contrast forces fg to pure white
    let fg = r.resolve_token("color.foreground").unwrap();
    if let Token::Color(c) = fg {
        assert_eq!(c.srgb, [255, 255, 255, 255], "high-contrast fg not applied");
    } else {
        panic!();
    }

    // protanopia rewrites the success/warning/error palette
    let warn = r.resolve_token("color.warning").unwrap();
    if let Token::Color(c) = warn {
        // protanopia warning is (236, 156, 24)
        assert_eq!(c.srgb[0], 236);
        assert_eq!(c.srgb[1], 156);
    } else {
        panic!();
    }

    // reduced-transparency forces popup-shadow alpha to 255
    let shadow = r.resolve_token("shadow.popup").unwrap();
    if let Token::Shadow(s) = shadow {
        assert_eq!(s.color.srgb[3], 255);
    } else {
        panic!();
    }
}

#[test]
fn reduced_motion_zeros_all_motion_tokens() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();

    let stack = VariantStack::new().with(VariantTag::Accessibility(Accessibility::ReducedMotion));
    r.set_variants(stack);

    let merged = r.merged().unwrap();
    let mut animation_count = 0;
    for (name, tok) in &merged.tokens {
        if let Token::Animation(AnimationToken { duration_ms, .. }) = tok {
            assert_eq!(
                *duration_ms, 0,
                "motion token '{name}' not zeroed by reduced-motion"
            );
            animation_count += 1;
        }
    }
    assert!(animation_count >= 4, "expected at least 4 motion tokens");
}

#[test]
fn variant_stack_with_no_active_panics_clean() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    let _err = r.merged().unwrap_err(); // active not set yet
}

#[test]
fn variant_overlay_does_not_apply_without_tag() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();
    // No variants — high-contrast overlay should NOT apply.
    let fg = r.resolve_token("color.foreground").unwrap();
    if let Token::Color(c) = fg {
        // base dark fg is (240, 240, 240); high-contrast would be (255, 255, 255).
        assert_eq!(c.srgb, [240, 240, 240, 255]);
    } else {
        panic!();
    }
}

#[test]
fn dark_high_contrast_protanopia_full_stack() {
    // The exact stack the dispatch package calls out as exit criterion.
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();
    let stack = VariantStack::new()
        .with(VariantTag::Accessibility(Accessibility::HighContrast))
        .with(VariantTag::ColorBlind(ColorBlind::Protanopia))
        .with(VariantTag::Accessibility(
            Accessibility::ReducedTransparency,
        ));
    r.set_variants(stack);
    // Resolve must not error; named style must resolve too.
    let merged = r.merged().unwrap();
    assert!(merged.tokens.contains_key("color.foreground"));
    assert!(merged.tokens.contains_key("color.warning")); // from protanopia
    assert!(merged.tokens.contains_key("shadow.popup")); // from reduced-transparency

    // Curves preserved by reduced-motion-absent path.
    if let Token::Animation(a) = merged.tokens.get("motion.fade.in").unwrap() {
        assert!(a.duration_ms > 0);
        // also verify Curve enum still typechecks
        let _ = matches!(
            a.curve,
            Curve::EaseOut | Curve::EaseIn | Curve::Linear | Curve::EaseInOut | Curve::Spring
        );
    }
}
