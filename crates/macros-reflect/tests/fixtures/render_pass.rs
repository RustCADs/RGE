//! Pilot fixture: a `RenderPass`-shaped struct deriving `Reflect` via the
//! `rge-macros-reflect` proc-macro. Cross-checked against the hand-rolled
//! variant in `kernel/types/tests/reflect_round_trip.rs`.
//!
//! Marked `#[allow(dead_code)]` because each test in `derive_test.rs` /
//! `ui_hints_test.rs` only uses a subset of the surface.

#![allow(dead_code, unreachable_pub)]

use rge_macros_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// W02 pilot type — mirrors the rustforge `editor-app/RenderPass` shape.
#[derive(Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(version = "1.0.0")]
pub struct RenderPass {
    pub name: String,

    pub enabled: bool,

    #[reflect(ui = "Slider", min = 0, max = 1000)]
    pub priority: i32,

    #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.001)]
    pub clear_color_r: f32,

    #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.001)]
    pub clear_color_g: f32,

    #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.001)]
    pub clear_color_b: f32,

    #[reflect(ui = "Slider", min = 0.0, max = 1.0, step = 0.001)]
    pub clear_color_a: f32,

    #[reflect(ui = "Slider", min = 1, max = 16, default = 4)]
    pub msaa_samples: u32,
}

impl Default for RenderPass {
    fn default() -> Self {
        Self {
            name: String::from("Pass"),
            enabled: true,
            priority: 100,
            clear_color_r: 0.0,
            clear_color_g: 0.0,
            clear_color_b: 0.0,
            clear_color_a: 1.0,
            msaa_samples: 4,
        }
    }
}

/// Fixture used by `validate_attr_test.rs` to exercise `validate` and
/// `custom_drawer` plumbing without compile-failing.
#[derive(Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(version = "0.1.0")]
pub struct WithValidate {
    #[reflect(validate = "validators::nonzero")]
    pub count: u32,

    #[reflect(custom_drawer = "drawers::big_red_button")]
    pub trigger: bool,

    #[reflect(skip)]
    pub cache: u64,
}

impl Default for WithValidate {
    fn default() -> Self {
        Self {
            count: 1,
            trigger: false,
            cache: 0,
        }
    }
}

/// Sample value for round-trip tests.
pub fn sample_render_pass() -> RenderPass {
    RenderPass {
        name: String::from("MainColorPass"),
        enabled: true,
        priority: 100,
        clear_color_r: 0.05,
        clear_color_g: 0.10,
        clear_color_b: 0.20,
        clear_color_a: 1.0,
        msaa_samples: 4,
    }
}
