//! Residency tracker substrate.
//!
//! # NON-GOALS (mirror of crate-level doc, restated for in-module ownership clarity)
//!
//! - No residency algorithm. v0 tracks state transitions caller drives;
//!   the eventual residency manager (which decides WHEN to load / unload
//!   based on view frustum + memory budget) lands in dedicated future
//!   dispatches.
//! - No hysteresis. The PLAN §10.1 "Hysteresis 1s" feature is NOT here;
//!   v0 has no time concept.
//! - No predictive prefetch. The PLAN §10.1 "predictive prefetch" feature
//!   is NOT here; v0 has no view frustum or visibility input.
//! - No GPU upload. Residency is tracked in vocabulary only; actual
//!   memory transfers are downstream `kernel/gfx` / future
//!   `kernel/gpu-resources` work.
//! - No actual I/O. Residency requests are tracked; their dispatch to a
//!   real IO driver is `kernel/io-scheduler` + future driver crates.
//! - No job execution. State transitions are records; the work that
//!   produces them is `kernel/job-system` + downstream loaders.
//! - No closures / callbacks / observers. The tracker is passive lookup;
//!   consumers poll via [`ResidencyTracker::lookup`] /
//!   [`ResidencyTracker::iter`].
//! - No memory budget enforcement. `byte_size` is informational; the
//!   tracker does NOT enforce a sum or reject inserts at a budget.
//! - No `kernel/asset` integration. Callers route ID generation; v0 does
//!   NOT tie [`crate::ResidencyId`] to `kernel/asset::AssetId`.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new §18
//!   companion.

use std::collections::BTreeMap;

use crate::record::{ResidencyId, ResidencyRecord};
use crate::state::ResidencyState;

/// Registry of residency records.
///
/// v0 stub: `BTreeMap`-backed for deterministic iteration order. Lookup
/// is O(log n); iteration yields records in [`ResidencyId`] byte-order.
///
/// The tracker owns no buffers, no IO state, no job pool, no scheduling
/// mechanism. It is purely a substrate for tracking which residency
/// records exist and what state they are in; consumers (eventual
/// residency manager, asset-streaming dispatcher, future visibility
/// system) look up records and drive state transitions via mechanisms
/// that land in dedicated future dispatches.
#[derive(Default, Debug)]
pub struct ResidencyTracker {
    /// Tracked records keyed by [`ResidencyId`] for deterministic
    /// iteration. `BTreeMap` rather than `HashMap` so `iter()` is stable
    /// and reproducible across runs.
    records: BTreeMap<ResidencyId, ResidencyRecord>,
}

impl ResidencyTracker {
    /// Create an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `record`. If a record with the same [`ResidencyId`] was
    /// already tracked, returns the previous record (replaces in-place;
    /// matches `BTreeMap::insert` upsert semantics).
    pub fn insert(&mut self, record: ResidencyRecord) -> Option<ResidencyRecord> {
        self.records.insert(record.id, record)
    }

    /// Remove the record for `id`. Returns the removed record, or `None`
    /// if no record was tracked under `id`.
    pub fn remove(&mut self, id: ResidencyId) -> Option<ResidencyRecord> {
        self.records.remove(&id)
    }

    /// Look up the record for `id`, or `None` if no record is tracked.
    #[must_use]
    pub fn lookup(&self, id: ResidencyId) -> Option<&ResidencyRecord> {
        self.records.get(&id)
    }

    /// True iff a record for `id` is currently tracked.
    #[must_use]
    pub fn contains(&self, id: ResidencyId) -> bool {
        self.records.contains_key(&id)
    }

    /// Mutate the [`ResidencyState`] of the record at `id`. Returns the
    /// previous state, or `None` if no record was tracked under `id`.
    ///
    /// v0 does NOT validate the transition (e.g. `NotResident -> Resident`
    /// directly is permitted even though the eventual residency manager
    /// would route through `Loading`). Future dispatches may add a
    /// transition-table check; v0 keeps the surface bounded.
    pub fn set_state(&mut self, id: ResidencyId, state: ResidencyState) -> Option<ResidencyState> {
        let record = self.records.get_mut(&id)?;
        let previous = record.state;
        record.state = state;
        Some(previous)
    }

    /// Number of tracked records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// `true` when no records are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterate tracked records in [`ResidencyId`] byte-order.
    ///
    /// Iteration is deterministic and does not consume the tracker.
    pub fn iter(&self) -> impl Iterator<Item = &ResidencyRecord> {
        self.records.values()
    }

    /// Remove every tracked record.
    pub fn clear(&mut self) {
        self.records.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordKind;

    fn rec(id_byte: u8, state: ResidencyState, byte_size: u64) -> ResidencyRecord {
        ResidencyRecord::new(
            ResidencyId::from_bytes([id_byte; 16]),
            state,
            RecordKind::Placeholder,
            byte_size,
        )
    }

    #[test]
    fn empty_new_is_empty() {
        let t = ResidencyTracker::new();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn insert_returns_none_for_new_id() {
        let mut t = ResidencyTracker::new();
        let prev = t.insert(rec(1, ResidencyState::NotResident, 100));
        assert!(prev.is_none());
        assert_eq!(t.len(), 1);
        assert!(!t.is_empty());
    }

    #[test]
    fn insert_returns_previous_for_duplicate_id() {
        let mut t = ResidencyTracker::new();
        t.insert(rec(1, ResidencyState::NotResident, 100));
        let prev = t.insert(rec(1, ResidencyState::Loading, 200));
        assert!(prev.is_some());
        let p = prev.unwrap();
        assert_eq!(p.state, ResidencyState::NotResident);
        assert_eq!(p.byte_size, 100);
        // Length unchanged (replacement, not addition).
        assert_eq!(t.len(), 1);
        // Looking up now yields the new record.
        let now = t.lookup(ResidencyId::from_bytes([1u8; 16])).unwrap();
        assert_eq!(now.state, ResidencyState::Loading);
        assert_eq!(now.byte_size, 200);
    }

    #[test]
    fn lookup_finds_tracked_record() {
        let mut t = ResidencyTracker::new();
        t.insert(rec(7, ResidencyState::Resident, 42));
        let found = t.lookup(ResidencyId::from_bytes([7u8; 16]));
        assert!(found.is_some());
        let f = found.unwrap();
        assert_eq!(f.state, ResidencyState::Resident);
        assert_eq!(f.byte_size, 42);
    }

    #[test]
    fn lookup_returns_none_for_missing_id() {
        let t = ResidencyTracker::new();
        assert!(t.lookup(ResidencyId::from_bytes([0u8; 16])).is_none());
    }

    #[test]
    fn remove_returns_record_and_decrements_len() {
        let mut t = ResidencyTracker::new();
        t.insert(rec(5, ResidencyState::Resident, 64));
        let removed = t.remove(ResidencyId::from_bytes([5u8; 16]));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().byte_size, 64);
        assert_eq!(t.len(), 0);
        // Subsequent remove returns None.
        assert!(t.remove(ResidencyId::from_bytes([5u8; 16])).is_none());
    }

    #[test]
    fn set_state_returns_previous_state() {
        let mut t = ResidencyTracker::new();
        t.insert(rec(3, ResidencyState::NotResident, 10));
        let id = ResidencyId::from_bytes([3u8; 16]);

        let prev = t.set_state(id, ResidencyState::Loading);
        assert_eq!(prev, Some(ResidencyState::NotResident));

        let prev2 = t.set_state(id, ResidencyState::Resident);
        assert_eq!(prev2, Some(ResidencyState::Loading));

        // Final state is Resident.
        assert_eq!(t.lookup(id).unwrap().state, ResidencyState::Resident);
    }

    #[test]
    fn set_state_on_missing_returns_none() {
        let mut t = ResidencyTracker::new();
        let prev = t.set_state(ResidencyId::from_bytes([99u8; 16]), ResidencyState::Loading);
        assert!(prev.is_none());
    }

    #[test]
    fn iter_yields_records_in_id_byte_order() {
        let mut t = ResidencyTracker::new();
        // Insert out of byte order; iteration must yield by-id-order.
        t.insert(rec(3, ResidencyState::Resident, 30));
        t.insert(rec(1, ResidencyState::Loading, 10));
        t.insert(rec(2, ResidencyState::NotResident, 20));

        let byte_sizes: Vec<u64> = t.iter().map(|r| r.byte_size).collect();
        assert_eq!(byte_sizes, vec![10, 20, 30]);
        // Iteration does not consume.
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn clear_empties_tracker() {
        let mut t = ResidencyTracker::new();
        t.insert(rec(1, ResidencyState::NotResident, 10));
        t.insert(rec(2, ResidencyState::Loading, 20));
        t.clear();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn default_impl_matches_new() {
        let a = ResidencyTracker::new();
        let b = ResidencyTracker::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}
