//! `rge-kernel-asset-streaming` — residency record tracking substrate.
//!
//! Failure class: recoverable
//!
//! Implements the residency-tracking substrate listed in PLAN.md §1.6.5 /
//! §10.1 alongside `kernel/io-scheduler`, `kernel/job-system`, and
//! `kernel/asset-view`. PLAN frames the eventual implementation as a
//! "residency manager" with 4-tier streaming priorities + 1s hysteresis +
//! predictive prefetch (§10.1); v0 ships only the residency-state +
//! record + tracker vocabulary substrate without any algorithm,
//! hysteresis, or prefetch behaviour.
//!
//! # NON-GOALS
//!
//! v0 establishes vocabulary and ownership boundaries; it deliberately
//! does NOT establish behaviour richness. The strongest part of this
//! crate's v0 is the list of what it intentionally is **not**:
//!
//! - No residency algorithm. v0 tracks state transitions caller drives;
//!   the eventual residency manager (which decides WHEN to load /
//!   unload based on view frustum + memory budget) lands in dedicated
//!   future dispatches.
//! - No hysteresis. The PLAN §10.1 "Hysteresis 1s" feature is NOT here;
//!   v0 has no time concept.
//! - No predictive prefetch. The PLAN §10.1 "predictive prefetch"
//!   feature is NOT here; v0 has no view frustum or visibility input.
//! - No GPU upload. Residency is tracked in vocabulary only; actual
//!   memory transfers are downstream `kernel/gfx` / future
//!   `kernel/gpu-resources` work.
//! - No actual I/O. Residency requests are tracked; their dispatch to a
//!   real IO driver is `kernel/io-scheduler` + future driver crates.
//! - No job execution. State transitions are records; the work that
//!   produces them is `kernel/job-system` + downstream loaders.
//! - No closures / callbacks / observers. The tracker is passive
//!   lookup; consumers poll.
//! - No memory budget enforcement. `byte_size` is informational; the
//!   tracker does NOT enforce a sum or reject inserts at a budget.
//! - No `kernel/asset` integration. Callers route ID generation; v0
//!   does NOT tie [`ResidencyId`] to `kernel/asset::AssetId`.
//! - No `kernel/io-scheduler` priority coupling. Records have no
//!   embedded `Priority`; if a future variant needs streaming priority
//!   it can add the field via `#[non_exhaustive]` evolution.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new
//!   §18 companion.
//!
//! # What this crate is
//!
//! Vocabulary, ownership boundaries, and future-safe seams. Future
//! dispatches extend this substrate incrementally without undoing the
//! foundational choices made here: the [`ResidencyState`] enum is
//! `#[non_exhaustive]` so new variants (`Failed` / `Evicted` / `Pinned`)
//! may be added; the [`record::RecordKind`] is `#[non_exhaustive]` so
//! domain-specific variants (`Mesh` / `Texture` / `Audio` / `Script`)
//! may be added; the tracker is `BTreeMap`-backed so iteration is
//! deterministic and reproducible.
//!
//! # Cavity-pattern self-check
//!
//! Shares the precedent structure with `kernel/io-scheduler`,
//! `kernel/job-system`, and `kernel/asset-view`: opaque id type with
//! `const` accessors + `#[non_exhaustive]` kind + carrier struct +
//! `BTreeMap`-backed registry/queue + reciprocal NON-GOALS
//! cross-references. Adds a 4-tier `#[non_exhaustive]` `ResidencyState`
//! enum capturing the lifecycle progression (asset-view's substrate
//! does not have a state because views are passive descriptors;
//! io-scheduler / job-system have priority because they're queues).
//! The four v0 cavities are now structurally interlocked.

pub mod record;
pub mod state;
pub mod tracker;

pub use record::{RecordKind, ResidencyId, ResidencyRecord};
pub use state::ResidencyState;
pub use tracker::ResidencyTracker;

#[cfg(test)]
mod smoke {
    use super::*;

    /// End-to-end: construct tracker, insert a record at NotResident,
    /// transition through Loading → Resident → Unloading, then remove.
    #[test]
    fn tracker_round_trip_through_lifecycle() {
        let mut t = ResidencyTracker::new();
        let id = ResidencyId::from_bytes([0xaa; 16]);

        t.insert(ResidencyRecord::new(
            id,
            ResidencyState::NotResident,
            RecordKind::Placeholder,
            1024,
        ));
        assert_eq!(t.len(), 1);
        assert_eq!(t.lookup(id).unwrap().state, ResidencyState::NotResident);

        let prev = t.set_state(id, ResidencyState::Loading);
        assert_eq!(prev, Some(ResidencyState::NotResident));
        assert_eq!(t.lookup(id).unwrap().state, ResidencyState::Loading);

        let prev = t.set_state(id, ResidencyState::Resident);
        assert_eq!(prev, Some(ResidencyState::Loading));

        let prev = t.set_state(id, ResidencyState::Unloading);
        assert_eq!(prev, Some(ResidencyState::Resident));

        let removed = t.remove(id).expect("removed");
        assert_eq!(removed.state, ResidencyState::Unloading);
        assert!(t.is_empty());
    }
}
