//! Integration test: load a vendored Lucide icon, tint it at three
//! different theme colors, and assert that:
//!
//! 1. The tinted SVG byte streams are pairwise distinct.
//! 2. Rasterizing each produces a bitmap whose dominant non-zero
//!    pixels match the requested tint color.
//!
//! Satisfies the W06 exit criterion:
//!     Tint at 3 theme colors (accent.action, error.500, text.muted)
//!     verifies correctness.

// Test averaging arithmetic produces values in 0..=255 by construction;
// the truncation cast is the obvious form.
#![allow(clippy::cast_possible_truncation)]

use std::path::PathBuf;

use rge_ui_icons::tint::{apply_tint, rasterize};
use rge_ui_icons::{Color, IconRegistry};

fn lucide_manifest_path() -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root.join("assets/sets/lucide.icons.ron")
}

/// Surrogate for `ui_theme::accent::action` until W05 lands.
const THEME_ACCENT_ACTION: Color = Color {
    r: 64,
    g: 160,
    b: 220,
};
/// Surrogate for `ui_theme::error::500`.
const THEME_ERROR_500: Color = Color {
    r: 220,
    g: 64,
    b: 64,
};
/// Surrogate for `ui_theme::text::muted`.
const THEME_TEXT_MUTED: Color = Color {
    r: 130,
    g: 130,
    b: 130,
};

fn dominant_color(pixels: &[u8]) -> (u8, u8, u8) {
    // Average the non-transparent pixels to find the icon's "tint".
    let mut sum_r: u64 = 0;
    let mut sum_g: u64 = 0;
    let mut sum_b: u64 = 0;
    let mut count: u64 = 0;
    for px in pixels.chunks_exact(4) {
        if px[3] >= 200 {
            // Pixels with high alpha — these are the stroke pixels.
            // tiny_skia output is premultiplied, so divide by alpha
            // (which is ~255) to get the source color back.
            sum_r += u64::from(px[0]);
            sum_g += u64::from(px[1]);
            sum_b += u64::from(px[2]);
            count += 1;
        }
    }
    if count == 0 {
        return (0, 0, 0);
    }
    (
        (sum_r / count) as u8,
        (sum_g / count) as u8,
        (sum_b / count) as u8,
    )
}

fn near(a: u8, b: u8, tol: i32) -> bool {
    (i32::from(a) - i32::from(b)).abs() <= tol
}

#[test]
fn tint_three_theme_colors_produce_distinct_svgs() {
    let mut registry = IconRegistry::new();
    registry.register_set(&lucide_manifest_path()).unwrap();
    let handle = registry.lookup("save").unwrap();
    let svg = registry.svg_bytes(&handle).unwrap().to_owned();

    let a = apply_tint(&svg, THEME_ACCENT_ACTION);
    let b = apply_tint(&svg, THEME_ERROR_500);
    let c = apply_tint(&svg, THEME_TEXT_MUTED);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
    // None of them should still contain `currentColor`.
    for v in [&a, &b, &c] {
        assert!(!v.contains("currentColor"), "got: {v}");
    }
}

#[test]
fn rasterize_tinted_save_icon_at_three_colors() {
    let mut registry = IconRegistry::new();
    registry.register_set(&lucide_manifest_path()).unwrap();
    let handle = registry.lookup("save").unwrap();
    let svg = registry.svg_bytes(&handle).unwrap().to_owned();

    for (label, c) in [
        ("accent_action", THEME_ACCENT_ACTION),
        ("error_500", THEME_ERROR_500),
        ("text_muted", THEME_TEXT_MUTED),
    ] {
        let tinted = apply_tint(&svg, c);
        let img = rasterize(&tinted, 64, 64).expect("rasterize");
        assert_eq!(img.width, 64);
        assert_eq!(img.height, 64);
        assert_eq!(img.pixels.len(), 64 * 64 * 4);

        // At least one stroke pixel must exist.
        let any_stroke = img.pixels.chunks_exact(4).any(|p| p[3] > 0);
        assert!(any_stroke, "tint {label}: no stroke pixels rendered");

        // Un-premultiply and check dominant color.
        let unmul = img.pixels_unmultiplied();
        let (dr, dg, db) = dominant_color(&unmul);
        assert!(
            near(dr, c.r, 32) && near(dg, c.g, 32) && near(db, c.b, 32),
            "tint {label}: expected ~RGB({}, {}, {}), got ({}, {}, {})",
            c.r,
            c.g,
            c.b,
            dr,
            dg,
            db
        );
    }
}

#[test]
fn rasterize_every_vendored_icon() {
    // CI test: every icon × every (vendored) theme renders without
    // panicking and produces non-empty output. Per the W06 spec this
    // is the contrast/render smoke test — full WCAG-AA contrast
    // measurement happens in editor-ui at integration time.
    let mut registry = IconRegistry::new();
    let id = registry.register_set(&lucide_manifest_path()).unwrap();
    let names: Vec<_> = registry
        .set_info(&id)
        .unwrap()
        .entries
        .keys()
        .cloned()
        .collect();

    let themes = [THEME_ACCENT_ACTION, THEME_ERROR_500, THEME_TEXT_MUTED];

    for name in &names {
        let h = rge_ui_icons::IconHandle::new(id.clone(), name.clone());
        let svg = registry
            .svg_bytes(&h)
            .unwrap_or_else(|e| panic!("read {name}: {e}"))
            .to_owned();
        for c in themes {
            let tinted = apply_tint(&svg, c);
            let img = rasterize(&tinted, 24, 24)
                .unwrap_or_else(|e| panic!("rasterize {name} at {c:?}: {e}"));
            assert!(
                img.pixels.chunks_exact(4).any(|p| p[3] > 0),
                "icon {name} at color {c:?} produced empty bitmap"
            );
        }
    }
}
