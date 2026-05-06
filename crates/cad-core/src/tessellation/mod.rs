//! `cad_core::tessellation` — output mesh + memoization cache.
//!
//! Failure class: snapshot-recoverable
//!
//! # Modules
//!
//! * [`mesh`] — flat triangle-list `Tessellation`.
//! * [`cache`] — `(structural_hash, tolerance)`-keyed `TessellationCache`.
//!
//! Operators in [`crate::operators`] consume zero or more upstream
//! tessellations and produce one output tessellation. The cache memoizes
//! sub-tree evaluation by recursive structural hash — see
//! [`crate::OperatorGraph::evaluate`].

pub mod cache;
pub mod mesh;

pub use cache::{CacheKey, TessellationCache, Tolerance, ToleranceError};
pub use mesh::{Tessellation, TessellationError};
