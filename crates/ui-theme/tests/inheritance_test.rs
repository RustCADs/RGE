// adapted from rustforge::apps::editor-app::egui_overlay on 2026-05-05 — extracted ThemeRegistry
//
// Integration test: themes load from the vendored `assets/themes/`
// directory and resolve through their `extends:` chains correctly.

use std::path::PathBuf;

use rge_ui_theme::{Theme, ThemeRegistry, Token};

fn assets_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("assets");
    p.push("themes");
    p
}

#[test]
fn loads_all_vendored_themes() {
    let mut r = ThemeRegistry::new();
    let n = r.load_dir(assets_dir()).expect("load_dir failed");
    assert!(n >= 4, "expected at least 4 vendored theme files, got {n}");

    for required in ["dark-default", "light-default", "studio-pro", "daylight"] {
        assert!(
            r.get(required).is_some(),
            "vendored theme '{required}' missing"
        );
    }
}

#[test]
fn dark_default_resolves_core_tokens() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("dark-default").unwrap();

    let bg = r.resolve_token("color.background").unwrap();
    let fg = r.resolve_token("color.foreground").unwrap();
    matches!(bg, Token::Color(_));
    matches!(fg, Token::Color(_));
}

#[test]
fn light_default_resolves_core_tokens() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("light-default").unwrap();
    let bg = r.resolve_token("color.background").unwrap();
    let fg = r.resolve_token("color.foreground").unwrap();
    matches!(bg, Token::Color(_));
    matches!(fg, Token::Color(_));
}

#[test]
fn studio_pro_inherits_from_dark_default() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("studio-pro").unwrap();

    // Token defined only on dark-default but inherited:
    let _font = r.resolve_token("font.body").unwrap();

    // Token overridden on studio-pro:
    let bg = r.resolve_token("color.background").unwrap();
    if let Token::Color(c) = bg {
        // studio-pro overrides to (18, 18, 22)
        assert_eq!(c.srgb[0], 18, "studio-pro override not applied");
    } else {
        panic!("expected Color");
    }
}

#[test]
fn daylight_inherits_from_light_default() {
    let mut r = ThemeRegistry::new();
    r.load_dir(assets_dir()).unwrap();
    r.set_active("daylight").unwrap();

    let _font = r.resolve_token("font.body").unwrap(); // inherited
    let bg = r.resolve_token("color.background").unwrap();
    if let Token::Color(c) = bg {
        assert_eq!(c.srgb[0], 252, "daylight override not applied");
    } else {
        panic!();
    }
}

#[test]
fn inheritance_chain_max_depth_3() {
    use rge_ui_theme::MAX_INHERITANCE_DEPTH;
    let mut r = ThemeRegistry::new();
    // Build a chain of length 4 (which exceeds 3).
    let a = Theme::new("a");
    let mut b = Theme::new("b");
    b.extends = Some("a".into());
    let mut c = Theme::new("c");
    c.extends = Some("b".into());
    let mut d = Theme::new("d");
    d.extends = Some("c".into());
    let mut e = Theme::new("e");
    e.extends = Some("d".into());
    r.insert(a);
    r.insert(b);
    r.insert(c);
    r.insert(d);
    r.insert(e);
    let err = r.flatten("e").unwrap_err();
    assert!(matches!(
        err,
        rge_ui_theme::RegistryError::DepthExceeded { .. }
    ));
    // A chain of length exactly MAX_INHERITANCE_DEPTH succeeds.
    const _: () = assert!(MAX_INHERITANCE_DEPTH == 3);
    let f = r.flatten("c").unwrap();
    assert_eq!(f.name, "c");
}
