//! Tessellation cache: memoizes operator-graph evaluation results keyed on
//! `(structural_hash, tolerance)`.
//!
//! Failure class: snapshot-recoverable
//!
//! The cache is owned by the application (typically `cad-projection`) and
//! threaded through [`crate::OperatorGraph::evaluate`] explicitly. Hits and
//! misses are tracked so the editor can surface cache-effectiveness metrics.
//!
//! Uses `HashMap` (not `BTreeMap`) intentionally — determinism is not a
//! requirement here (the cache key fully encodes the inputs and the value is
//! always recomputable on miss) and we want hashing speed.

use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;

use crate::tessellation::mesh::Tessellation;

// ---------------------------------------------------------------------------
// Tolerance
// ---------------------------------------------------------------------------

/// Errors produced when constructing a [`Tolerance`].
#[derive(Debug, Error, PartialEq)]
pub enum ToleranceError {
    /// Tolerance must be a positive, finite `f32`.
    #[error("tolerance must be finite and > 0 (got {value})")]
    Invalid {
        /// The invalid value supplied.
        value: f32,
    },
}

/// Quantization factor used when comparing tolerances for equality.
///
/// Two `Tolerance` values whose `(value * Q) as u64` quantized representation
/// matches are considered equal. With `Q = 1e9` this means tolerances that
/// agree to ~1 nanometer (when interpreted in meters) hash and compare equal.
const TOLERANCE_QUANTIZE: f64 = 1.0e9;

/// Newtype wrapping a positive, finite tessellation tolerance.
///
/// `Hash` / `Eq` quantize the inner `f32` so that floating-point tolerances
/// that differ only by epsilon hash and compare equal — important so that
/// `Tolerance(0.001)` and `Tolerance(0.001 + 1e-15)` produce the same cache
/// key.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance(pub f32);

impl Tolerance {
    /// Construct a tolerance after validating finiteness and positivity.
    ///
    /// # Errors
    ///
    /// Returns [`ToleranceError::Invalid`] when `t` is not finite or `<= 0.0`.
    pub fn new(t: f32) -> Result<Self, ToleranceError> {
        if !t.is_finite() || t <= 0.0 {
            return Err(ToleranceError::Invalid { value: t });
        }
        Ok(Self(t))
    }

    /// Return the inner `f32` value.
    #[must_use]
    pub fn value(self) -> f32 {
        self.0
    }

    /// Quantized integer representation used for `Hash` + `Eq`.
    #[must_use]
    fn quantized(self) -> u64 {
        // Multiply in f64 to avoid losing precision on large tolerance values.
        let q = f64::from(self.0) * TOLERANCE_QUANTIZE;
        // q is finite + positive (validated by `new`); cast saturates negatives
        // to 0 in case of any FP edge — already excluded but be defensive.
        if q < 0.0 || !q.is_finite() {
            0
        } else {
            // Saturate above u64::MAX rather than panic.
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            {
                q as u64
            }
        }
    }
}

impl PartialEq for Tolerance {
    fn eq(&self, other: &Self) -> bool {
        self.quantized() == other.quantized()
    }
}

impl Eq for Tolerance {}

impl std::hash::Hash for Tolerance {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.quantized().hash(state);
    }
}

// ---------------------------------------------------------------------------
// CacheKey
// ---------------------------------------------------------------------------

/// Composite key into the tessellation cache.
///
/// `structural_hash` is computed by the operator (recursively over its inputs
/// when applicable) and `tolerance` is the requested tessellation tolerance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// 32-byte BLAKE3 over operator + parameters + (recursively) input hashes.
    pub structural_hash: [u8; 32],
    /// Tessellation tolerance used during evaluation.
    pub tolerance: Tolerance,
}

// ---------------------------------------------------------------------------
// TessellationCache
// ---------------------------------------------------------------------------

/// Hash-keyed memoization cache for `Tessellation` results.
///
/// Values are stored behind `Arc` so multiple evaluators / readers can share
/// the same allocation cheaply.
#[derive(Debug, Default)]
pub struct TessellationCache {
    entries: HashMap<CacheKey, Arc<Tessellation>>,
    hits: u64,
    misses: u64,
}

impl TessellationCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up an entry. Does NOT mutate hit/miss counters — call
    /// [`Self::record_hit`] / [`Self::record_miss`] explicitly so callers
    /// (specifically `OperatorGraph::evaluate`) own their accounting.
    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<Arc<Tessellation>> {
        self.entries.get(key).cloned()
    }

    /// Insert (or overwrite) an entry. Returns the cloned `Arc` so the caller
    /// can both store and use the value without an extra lookup.
    pub fn insert(&mut self, key: CacheKey, value: Tessellation) -> Arc<Tessellation> {
        let arc = Arc::new(value);
        self.entries.insert(key, arc.clone());
        arc
    }

    /// Record a cache hit (incremented by `OperatorGraph::evaluate`).
    pub fn record_hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    /// Record a cache miss (incremented by `OperatorGraph::evaluate`).
    pub fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }

    /// Number of recorded hits.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Number of recorded misses.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Hit ratio as `Some(hits / (hits + misses))`, or `None` when both
    /// counters are zero.
    #[must_use]
    pub fn hit_rate(&self) -> Option<f64> {
        let total = self.hits + self.misses;
        if total == 0 {
            None
        } else {
            #[allow(clippy::cast_precision_loss)]
            Some(self.hits as f64 / total as f64)
        }
    }

    /// Drop all entries and reset hit/miss counters.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8, tol: f32) -> CacheKey {
        CacheKey {
            structural_hash: [byte; 32],
            tolerance: Tolerance::new(tol).expect("valid tol"),
        }
    }

    #[test]
    fn empty_cache_returns_none() {
        let cache = TessellationCache::new();
        assert!(cache.get(&key(0, 0.001)).is_none());
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0);
        assert_eq!(cache.hit_rate(), None);
    }

    #[test]
    fn insert_and_get_round_trip() {
        let mut cache = TessellationCache::new();
        let mesh =
            Tessellation::new(vec![[0.0_f32, 0.0, 0.0]], vec![]).expect("empty-index mesh ok");
        let k = key(7, 0.01);
        let arc_in = cache.insert(k, mesh.clone());
        let arc_out = cache.get(&k).expect("inserted");
        assert!(Arc::ptr_eq(&arc_in, &arc_out));
        assert_eq!(*arc_out, mesh);
    }

    #[test]
    fn hit_miss_tracking_accurate() {
        let mut cache = TessellationCache::new();
        cache.record_hit();
        cache.record_hit();
        cache.record_miss();
        assert_eq!(cache.hits(), 2);
        assert_eq!(cache.misses(), 1);
        assert!((cache.hit_rate().unwrap() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn tolerance_equality_under_epsilon() {
        let mut cache = TessellationCache::new();
        let mesh = Tessellation::new(vec![[1.0_f32, 2.0, 3.0]], vec![]).expect("mesh ok");
        let k1 = CacheKey {
            structural_hash: [42; 32],
            tolerance: Tolerance::new(0.001).expect("tol"),
        };
        cache.insert(k1, mesh);
        let k2 = CacheKey {
            structural_hash: [42; 32],
            // Different f32 bit pattern but quantizes to the same u64.
            tolerance: Tolerance::new(0.001 + 1e-15_f32).expect("tol2"),
        };
        assert_eq!(k1, k2, "epsilon-different tolerances must compare equal");
        assert!(
            cache.get(&k2).is_some(),
            "epsilon-different tolerance must hit the same cache slot"
        );
    }
}
