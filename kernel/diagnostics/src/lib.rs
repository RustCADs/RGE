//! `rge-kernel-diagnostics` — unified diagnostic substrate for the RGE engine.
//!
//! Failure class: recoverable
//!
//! Provides the canonical API for structured diagnostic emission across every
//! kernel and Tier-2 crate. Implements the philosophy described in PLAN.md §1.7:
//! rich span context, error aggregation (never fail-fast), a suggestion engine,
//! and the five failure classes from PLAN.md §1.13.
//!
//! # Design goals
//!
//! * **Lightweight** — zero heavy deps (no `miette`, no `ariadne`). Adoptable by
//!   all ~80 downstream crates without compile-time penalty.
//! * **Object-safe** — [`DiagnosticSink`] is a plain `dyn`-safe trait so
//!   subsystems accept `&mut dyn DiagnosticSink` without generics explosion.
//! * **Stable surface** — every `pub` item here is load-bearing for later kernel
//!   crates; additions require deliberate review.
//!
//! # Quick start
//!
//! ```rust
//! use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink, Severity, Span};
//!
//! let mut agg = DiagnosticAggregator::new();
//! agg.emit(Diagnostic::error("something went wrong")
//!     .with_span(Span::at_file("foo.rs", 12, 5)));
//! assert!(agg.has_errors());
//! ```

pub mod aggregator;
pub mod diagnostic;
pub mod failure_class;
pub mod severity;
pub mod sink;
pub mod span;

pub use aggregator::DiagnosticAggregator;
pub use diagnostic::{Diagnostic, Suggestion};
pub use failure_class::FailureClass;
pub use severity::Severity;
pub use sink::DiagnosticSink;
pub use span::Span;
