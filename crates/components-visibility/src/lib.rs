//! `rge-components-visibility` — visibility / disabled / highlight markers.
//!
//! [`Visibility`] is a tri-state enum (PLAN.md §1.5.1: `Visible | Hidden |
//! Inherited`). [`Hidden`] is a zero-sized override marker; [`Disabled`]
//! freezes simulation while leaving the entity drawable; [`Highlight`] is the
//! editor's selection / hover badge.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod disabled;
mod hidden;
mod highlight;
mod visibility;

pub use disabled::Disabled;
pub use hidden::Hidden;
pub use highlight::Highlight;
pub use visibility::Visibility;
