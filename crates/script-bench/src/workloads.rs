//! Workload definitions for the script-engine benchmark suite.
//!
//! Each workload is defined here as **pure data + a deterministic generator**
//! so that the native baseline ([`crate::native_baseline`]) and any future
//! engine harness (post-W04) operate on bit-identical inputs.  This is the
//! defence against "cherry-picked counter-benches" called out in
//! [PLAN.md §14](../../plans/PLAN.md).
//!
//! ## Workload roster (per PLAN.md §5.6)
//!
//! | id  | name                          | what it measures                                         |
//! | --- | ----------------------------- | -------------------------------------------------------- |
//! | W1  | `script_tick_1m_iters`        | tight-loop arithmetic throughput (1M iterations)         |
//! | W2  | `per_frame_tick_10k_entities` | per-frame ECS-component mutation across 10k entities     |
//! | W3  | `cold_start`                  | module load + ready-to-tick latency                      |
//! | W4  | `hot_reload_swap`             | swap-old-module-for-new latency over 100 cycles          |
//! | W5  | `memory_overhead`             | resident bytes per loaded script module                  |
//!
//! All numeric constants live here so callers cannot drift (an engine
//! benchmark that quietly halves the entity count would invalidate the
//! comparison).

use serde::{Deserialize, Serialize};

/// Iteration count for [`WorkloadId::ScriptTick1M`].
pub const SCRIPT_TICK_ITERATIONS: u32 = 1_000_000;

/// Entity count for [`WorkloadId::PerFrameTick10kEntities`].
pub const PER_FRAME_ENTITY_COUNT: u32 = 10_000;

/// Cycle count for [`WorkloadId::HotReloadSwap`].
pub const HOT_RELOAD_CYCLES: u32 = 100;

/// Fixed timestep used by every per-tick workload (60 Hz, in seconds).
pub const FIXED_DT: f32 = 1.0 / 60.0;

/// Deterministic seed for the entity-array generator. Changing this number
/// changes every published baseline number; bump only with a methodology
/// version bump in `METHODOLOGY.md`.
///
/// Mnemonic: ASCII `RGE-W20-1` packed into a `u64`.
pub const ENTITY_SEED: u64 = 0x5247_452D_5732_3031;

/// Stable identifier for each workload. Used as the key in the JSON output and
/// the row label in `BASELINE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadId {
    /// W1 — `script_tick_1m_iters`.
    ScriptTick1M,
    /// W2 — `per_frame_tick_10k_entities`.
    PerFrameTick10kEntities,
    /// W3 — `cold_start`.
    ColdStart,
    /// W4 — `hot_reload_swap`.
    HotReloadSwap,
    /// W5 — `memory_overhead`.
    MemoryOverhead,
}

impl WorkloadId {
    /// All workloads in canonical iteration order.
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::ScriptTick1M,
            Self::PerFrameTick10kEntities,
            Self::ColdStart,
            Self::HotReloadSwap,
            Self::MemoryOverhead,
        ]
    }

    /// Stable kebab-case name used in `BASELINE.md` and the JSON keys.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScriptTick1M => "script_tick_1m_iters",
            Self::PerFrameTick10kEntities => "per_frame_tick_10k_entities",
            Self::ColdStart => "cold_start",
            Self::HotReloadSwap => "hot_reload_swap",
            Self::MemoryOverhead => "memory_overhead",
        }
    }

    /// One-line description suitable for the methodology table.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::ScriptTick1M => {
                "tight loop: Transform.translation += dt * v, 1_000_000 iterations"
            }
            Self::PerFrameTick10kEntities => {
                "iterate 10_000 entities; mutate Transform component once per frame"
            }
            Self::ColdStart => "module load + ready-to-tick latency (single shot, cold cache)",
            Self::HotReloadSwap => "swap loaded module for a new build; measured over 100 cycles",
            Self::MemoryOverhead => "resident bytes per loaded script module (RSS delta)",
        }
    }
}

/// 3-component vector. Mirrors `crates/components-spatial::Vec3` shape so that
/// later integration can substitute the real type without changing the
/// workload code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// Construct from components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// In-place fused-multiply-add: `self += other * scalar`.
    #[inline]
    pub fn add_scaled(&mut self, other: Self, scalar: f32) {
        self.x += other.x * scalar;
        self.y += other.y * scalar;
        self.z += other.z * scalar;
    }
}

/// Minimal Transform component for benchmarks. Layout-stable so future
/// engine memory views can mmap the same buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Transform {
    /// Position.
    pub translation: Vec3,
    /// Linear velocity (per second).
    pub velocity: Vec3,
}

impl Transform {
    /// Apply one tick at fixed timestep `dt`: `translation += velocity * dt`.
    /// This is the kernel exercised by both [`WorkloadId::ScriptTick1M`] and
    /// [`WorkloadId::PerFrameTick10kEntities`].
    #[inline]
    pub fn integrate(&mut self, dt: f32) {
        self.translation.add_scaled(self.velocity, dt);
    }
}

/// Deterministic `SplitMix64` — used to populate the 10k-entity buffer in a
/// reproducible way without pulling in an RNG dependency.
#[inline]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "intentional: producing a uniform `f32` in [-1.0, 1.0] from RNG bits is precision-bounded by f32's 24-bit mantissa anyway; the >> 8 shift down to 56 bits then dividing by 2^56 lands inside the representable f32 range"
)]
fn next_f32(state: &mut u64) -> f32 {
    // Map to [-1.0, 1.0]
    let bits = splitmix64(state);
    let unit = (bits >> 8) as f32 / ((1u64 << 56) as f32);
    unit * 2.0 - 1.0
}

/// Generate the canonical entity buffer for [`WorkloadId::PerFrameTick10kEntities`].
/// Deterministic given [`ENTITY_SEED`]; identical bytes on every host.
#[must_use]
pub fn generate_entities(count: u32, seed: u64) -> Vec<Transform> {
    let mut state = seed;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(Transform {
            translation: Vec3::new(
                next_f32(&mut state),
                next_f32(&mut state),
                next_f32(&mut state),
            ),
            velocity: Vec3::new(
                next_f32(&mut state),
                next_f32(&mut state),
                next_f32(&mut state),
            ),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_ids_have_unique_names() {
        let names: Vec<_> = WorkloadId::all().iter().map(|w| w.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "workload names must be unique");
    }

    #[test]
    fn entity_generator_is_deterministic() {
        let a = generate_entities(64, ENTITY_SEED);
        let b = generate_entities(64, ENTITY_SEED);
        assert_eq!(a, b, "same seed must yield identical buffers");
    }

    #[test]
    fn integrate_kernel_is_correct() {
        let mut t = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.5, -0.5, 1.0),
        };
        t.integrate(2.0);
        assert!((t.translation.x - 2.0).abs() < 1e-6);
        assert!((t.translation.y - 1.0).abs() < 1e-6);
        assert!((t.translation.z - 5.0).abs() < 1e-6);
    }
}
