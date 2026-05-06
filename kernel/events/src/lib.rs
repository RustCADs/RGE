//! `rge-kernel-events` — minimal typed event bus.
//!
//! Failure class: recoverable
//!
//! Implements the frame-queued event substrate described in PLAN.md §1.3.
//! Events emitted during a frame are not visible to consumers until
//! [`EventBus::advance_frame`] is called; this keeps delivery synchronous,
//! deterministic, and allocation-free between frames.
//!
//! # Design goals
//!
//! * **No callbacks** — subscribers iterate channels themselves; no closure
//!   storage, no dynamic dispatch explosion.
//! * **No async** — frame-queued delivery is fully synchronous within a frame
//!   tick (no `tokio`, no `futures`).
//! * **Diagnostics-first** — [`EventBus::advance_frame`] emits one [`Info`]
//!   diagnostic per channel with queued events, using the standard
//!   [`DiagnosticSink`] interface from `rge-kernel-diagnostics`.
//!
//! # Quick start
//!
//! ```rust
//! use rge_kernel_events::{EventBus, EventChannel};
//! use rge_kernel_diagnostics::DiagnosticAggregator;
//!
//! #[derive(Clone)]
//! struct DamageEvent { amount: u32 }
//!
//! let mut bus = EventBus::new();
//! bus.emit(DamageEvent { amount: 42 });
//!
//! let mut sink = DiagnosticAggregator::new();
//! bus.advance_frame(&mut sink);
//!
//! let count = bus
//!     .channel::<DamageEvent>()
//!     .map(|ch| ch.iter_current().count())
//!     .unwrap_or(0);
//! assert_eq!(count, 1);
//! ```
//!
//! [`Info`]: rge_kernel_diagnostics::Severity::Info
//! [`DiagnosticSink`]: rge_kernel_diagnostics::DiagnosticSink

pub mod bus;
pub mod channel;
pub mod subscription;

pub use bus::EventBus;
pub use channel::EventChannel;
pub use subscription::SubscriptionId;
