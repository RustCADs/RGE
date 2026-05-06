//! Glyph-cache swap budget — exit-criterion gate per W07 PLAN.
//!
//! "Font swap < 100 ms (file load + glyph cache rebuild)."

use std::time::Instant;

use rge_ui_fonts::{FontRegistry, GlyphCache, Measure};

#[test]
fn font_swap_under_100ms_budget() {
    // Baseline: load Inter, prime the glyph cache for a representative
    // string, then swap to JetBrains Mono and re-prime. The swap budget
    // covers `clear` + new file load + first re-rasterization.

    let mut reg = FontRegistry::new_empty();
    reg.load_dir(&FontRegistry::vendored_fonts_dir().join("Inter"))
        .expect("Inter must load");

    let mut cache = GlyphCache::new();

    // Warm cache once with the existing face.
    let measure = Measure::new("Inter").with_size(13.0);
    let buffer = measure.build_buffer(&mut reg, "The quick brown fox jumps over the lazy dog");
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            let _ = cache.get_or_render(&mut reg, g.font_id, g.physical((0., 0.), 1.0).cache_key);
        }
    }
    assert!(
        !cache.is_empty(),
        "cache must hold at least one glyph after warm"
    );

    // Now swap to JetBrainsMono and time the entire swap.
    let started = Instant::now();
    cache.clear();
    let mut new_reg = FontRegistry::new_empty();
    new_reg
        .load_dir(&FontRegistry::vendored_fonts_dir().join("JetBrainsMono"))
        .expect("JetBrainsMono must load");
    let new_measure = Measure::new("JetBrains Mono").with_size(13.0);
    let new_buffer =
        new_measure.build_buffer(&mut new_reg, "The quick brown fox jumps over the lazy dog");
    for run in new_buffer.layout_runs() {
        for g in run.glyphs {
            let _ =
                cache.get_or_render(&mut new_reg, g.font_id, g.physical((0., 0.), 1.0).cache_key);
        }
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed.as_millis() < 100,
        "font-swap budget exceeded: {}ms",
        elapsed.as_millis()
    );
}
