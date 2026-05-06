//! `rge-kernel-app` — Main-loop driver for the RGE engine.
//!
//! Failure class: recoverable
//!
//! Main-loop driver per IMPLEMENTATION.md Phase 1.4. The runtime heartbeat:
//! ordered frame phases, fixed-timestep sim, variable-rate update, diagnostic
//! hooks for budget overruns.
//!
//! # Design
//!
//! * **No allocations after warmup** — ring buffers, static slices, generic
//!   closures; no `Vec`/`String`/`Box` in the hot path.
//! * **No `tokio`, no `winit`** — this crate is the loop *driver*. Window
//!   events flow in via a callback boundary that the owner supplies.
//! * **Diagnostics integration** — frame budget overruns are emitted as
//!   [`Severity::Warning`] diagnostics through the caller-supplied sink.
//!
//! # Quick start
//!
//! ```rust
//! use rge_kernel_app::{App, AppBuilder, FramePhase};
//! use rge_kernel_diagnostics::DiagnosticAggregator;
//!
//! let mut app = AppBuilder::new().build();
//! let mut sink = DiagnosticAggregator::new();
//!
//! app.run_frame(1.0 / 60.0, &mut sink, |phase, ctx, _sink| {
//!     let _ = (phase, ctx);
//! });
//!
//! assert_eq!(app.frame(), 1);
//! ```

pub mod app;
pub mod fixed_step;
pub mod frame;
pub mod phase;

pub use app::{App, AppBuilder};
pub use fixed_step::FixedStepAccumulator;
pub use frame::{FrameContext, FrameStats};
pub use phase::FramePhase;
