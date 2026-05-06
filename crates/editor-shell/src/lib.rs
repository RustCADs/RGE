//! `rge-editor-shell` — editor host: winit lifecycle + Play-in-Editor (PIE) state machine.
//!
//! Phase 5 deliverable per [`IMPLEMENTATION.md`](../../plans/IMPLEMENTATION.md).
//! Implements W03 dispatch (PLAN.md §6.13 PIE; §1.15 editor-state coordination).
//!
//! # Architecture
//!
//! `EditorShell` owns:
//! - the winit `ApplicationHandler` impl (in [`lifecycle`])
//! - the [`PlayState`] state machine
//! - the [`WorldSnapshot`] that backs Play/Stop round-trip
//! - the play-mode toolbar registration ([`play_toolbar`])
//! - the [`TimeScale`] slider (game systems scale; editor systems don't)
//! - a placeholder [`Viewport`] widget (renders "Editing"/"Playing" text)
//!
//! Authority boundaries (per PLAN.md §1.15):
//! - **runtime entity state** lives in `kernel/ecs::World` (stubbed locally
//!   here as [`world::World`]).
//! - **editor coordination state** (selection, active tool) lives in
//!   `crates/editor-state` and is re-exported via [`coord`].
//! - editor-state **does not** participate in `WorldSnapshot` — selection
//!   and active tool persist across Play/Stop cycles by virtue of living
//!   on the editor side of the boundary.
//!
//! # Phase 5 abort condition
//!
//! Per `IMPLEMENTATION.md` Phase 5: if PIE snapshot/restore exceeds 500ms on
//! a 10k-entity scene, ECS storage layout needs redesign. Timing harness
//! lives in [`snapshot::measure_round_trip`]; results are documented in
//! `RGE/plans/BASELINE.md`.

#![allow(clippy::module_name_repetitions)]

pub mod audit;
pub mod coord;
pub mod lifecycle;
pub mod play_state;
pub mod play_toolbar;
pub mod snapshot;
pub mod time_scale;
pub mod viewport;
pub mod world;

pub use lifecycle::EditorShell;
pub use play_state::{PlayState, PlayStateError, PlayStateTransition};
pub use play_toolbar::{PlayToolbar, ToolbarButton, ToolbarButtonId};
pub use snapshot::{SnapshotMetrics, WorldSnapshot};
pub use time_scale::{TimeScale, TimeScaleClass};
pub use viewport::Viewport;
