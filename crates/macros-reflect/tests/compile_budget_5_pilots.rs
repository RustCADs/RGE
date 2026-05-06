//! Compile-time budget probe: 5 reflected types in one TU.
//!
//! Per `IMPLEMENTATION.md` Phase 1.1 abort condition: if reflection
//! compile time on 5 pilot types > 30s, STOP and replan. This file is
//! a compile-time-only probe — the test body just asserts the types
//! were declared. The wall-clock value is captured by `cargo test
//! --timings` (or a hand-timed `cargo build`) and recorded in
//! `kernel/types/BUDGET.md`.
//!
//! The 5 pilot types deliberately differ in field count and type
//! shape so the extrapolation to ~100 types is meaningful.

use rge_macros_reflect::Reflect;
use serde::{Deserialize, Serialize};

#[derive(Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
struct Pilot1 {
    #[reflect(ui = "Slider", min = 0.0, max = 1.0)]
    a: f32,
    #[reflect(ui = "Slider", min = 0.0, max = 1.0)]
    b: f32,
    c: bool,
    d: i32,
}

#[derive(Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
struct Pilot2 {
    name: String,
    #[reflect(ui = "ColorRgb")]
    color: u32,
    #[reflect(ui = "Multiline", lines = 8)]
    description: String,
    enabled: bool,
}

#[derive(Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
struct Pilot3 {
    #[reflect(ui = "Slider", min = -1000.0, max = 1000.0)]
    x: f64,
    #[reflect(ui = "Slider", min = -1000.0, max = 1000.0)]
    y: f64,
    #[reflect(ui = "Slider", min = -1000.0, max = 1000.0)]
    z: f64,
    #[reflect(ui = "Slider", min = 0.0, max = 100.0)]
    radius: f32,
    #[reflect(skip)]
    cache: u64,
}

#[derive(Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
struct Pilot4 {
    #[reflect(ui = "FilePath", extensions = ["png", "jpg", "jpeg"])]
    texture_path: String,
    #[reflect(ui = "Slider", min = 0.0, max = 16.0)]
    mip_bias: f32,
    #[reflect(ui = "Slider", min = 1, max = 16)]
    anisotropy: u32,
    s_rgb: bool,
}

#[derive(Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
struct Pilot5 {
    #[reflect(ui = "Foldout", default_open = true)]
    section_open: bool,
    #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.01)]
    intensity: f32,
    #[reflect(ui = "ColorRgba")]
    tint: u32,
    #[reflect(ui = "Curve")]
    falloff: f32,
    #[reflect(ui = "Gradient")]
    gradient: u64,
    #[reflect(ui = "Hidden")]
    internal: i64,
    #[reflect(validate = "validators::nonnegative")]
    constrained: f32,
}

#[test]
fn five_pilots_compile_and_have_descriptors() {
    use rge_kernel_types::Reflect;
    assert_eq!(<Pilot1 as Reflect>::FIELDS.len(), 4);
    assert_eq!(<Pilot2 as Reflect>::FIELDS.len(), 4);
    assert_eq!(<Pilot3 as Reflect>::FIELDS.len(), 5);
    assert_eq!(<Pilot4 as Reflect>::FIELDS.len(), 4);
    assert_eq!(<Pilot5 as Reflect>::FIELDS.len(), 7);
    // 24 fields total across 5 types — close to the spec target.
    let total: usize = <Pilot1 as Reflect>::FIELDS.len()
        + <Pilot2 as Reflect>::FIELDS.len()
        + <Pilot3 as Reflect>::FIELDS.len()
        + <Pilot4 as Reflect>::FIELDS.len()
        + <Pilot5 as Reflect>::FIELDS.len();
    assert_eq!(total, 24);
}
