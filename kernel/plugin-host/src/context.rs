//! Context handle passed to plugins at lifecycle calls.
//!
//! v1 (post-2026-05-07-audit) extends v0's `&mut dyn DiagnosticSink` carrier
//! with a type-erased resource registry. Plugins extract owned resources via
//! [`PluginContext::take`], do their work, and put them back via
//! [`PluginContext::insert`]. The orchestrator wraps the plugin lifecycle
//! call by inserting before and taking after.
//!
//! # Why type-erased
//!
//! Per PLAN §10.4 dogfood rule: Tier-2 + Tier-3 plugins use the SAME
//! `Plugin` trait. plugin-host is Tier-1 and cannot import Tier-2 types
//! (`World`, `CadGraph`, `GfxContext`, …) per the `forbidden-dep` lint.
//! Type-erasure via `BTreeMap<TypeId, Box<dyn Any + Send>>` keeps plugin-host
//! Tier-1 while letting plugins parameterize over arbitrary resource types.
//!
//! # Why owned-resources-handoff (not borrowed references)
//!
//! Storing `&'a mut T` references in a runtime-typed map without `unsafe`
//! is genuinely impossible in safe Rust. Workspace forbids `unsafe`.
//! The compromise: orchestrator transfers OWNERSHIP of resources into the
//! context for the duration of the plugin call. The Box-per-resource
//! overhead is acceptable at plugin-tick rate (~60Hz).
//!
//! # Resource lifecycle
//!
//! 1. Orchestrator: `ctx.insert(world)` // moves World into ctx
//! 2. Plugin: `let mut world = ctx.take::<World>().ok_or(...)?;` // moves out
//! 3. Plugin: do work with world
//! 4. Plugin: `ctx.insert(world);` // moves back
//! 5. Orchestrator: `let world = ctx.take::<World>().expect("plugin returned World");`
//!
//! Plugins that fail to put a resource back leave the slot empty; the
//! orchestrator detects this via `expect` or graceful Option handling.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;

use rge_kernel_diagnostics::{Diagnostic, DiagnosticSink};

/// Context exposed to plugins during
/// [`Plugin::init`](crate::Plugin::init) /
/// [`Plugin::tick`](crate::Plugin::tick) /
/// [`Plugin::shutdown`](crate::Plugin::shutdown) calls.
///
/// Carries:
///
/// * `&mut dyn DiagnosticSink` — direct accessor preserved from v0 for
///   ergonomics; existing callers unaware of the resource registry see no
///   change.
/// * a type-erased resource registry keyed on [`TypeId`] (v1, post-2026-05-07
///   deep audit). Plugins extract owned resources, do their work, and put
///   them back. Resources are stored in a [`BTreeMap`] for deterministic
///   iteration matching the workspace convention.
pub struct PluginContext<'a> {
    diagnostics: &'a mut dyn DiagnosticSink,
    resources: BTreeMap<TypeId, Box<dyn Any + Send>>,
}

impl<'a> PluginContext<'a> {
    /// Construct a context wrapping a diagnostic sink. The sink is borrowed
    /// for the lifetime of the context; the resource registry starts empty.
    ///
    /// **Bit-identical to v0 `PluginContext::new`** — existing callers that
    /// don't use resources see no change in behaviour.
    pub fn new(diagnostics: &'a mut dyn DiagnosticSink) -> Self {
        Self {
            diagnostics,
            resources: BTreeMap::new(),
        }
    }

    // === Existing v0 API (unchanged) ===

    /// Emit a diagnostic through the host's sink.
    pub fn emit_diagnostic(&mut self, diag: Diagnostic) {
        self.diagnostics.emit(diag);
    }

    /// Borrow the diagnostic sink for advanced use (e.g. when the plugin
    /// needs to pass it deeper into a helper that takes
    /// `&mut dyn DiagnosticSink` directly).
    pub fn diagnostics(&mut self) -> &mut dyn DiagnosticSink {
        self.diagnostics
    }

    // === New v1 API (resource registry) ===

    /// Insert a resource. Replaces any previous value of the same type and
    /// returns the prior value if there was one.
    pub fn insert<T: Any + Send>(&mut self, value: T) -> Option<T> {
        self.resources
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|prev| prev.downcast::<T>().ok().map(|b| *b))
    }

    /// Borrow a resource mutably. Returns `None` if no resource of that type
    /// has been inserted.
    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Take ownership of a resource. The slot is left empty after this call.
    pub fn take<T: Any + Send>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast::<T>().ok().map(|b| *b))
    }

    /// Check whether a resource of the given type is present.
    #[must_use]
    pub fn contains<T: Any>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    /// Number of resources currently in the registry.
    #[must_use]
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Builder-style helper: insert a resource and return `self` for
    /// chaining. Useful for orchestrator setup.
    #[must_use]
    pub fn with_resource<T: Any + Send>(mut self, value: T) -> Self {
        drop(self.insert(value));
        self
    }

    /// Snapshot of [`TypeId`]s currently held in the resource registry.
    ///
    /// Used by [`crate::PluginHost`] to verify resource preservation across
    /// plugin lifecycle calls (per Pairing-3 / N1 panic-safety): the host
    /// snapshots before invoking the plugin and again after the call returns
    /// (or its panic is caught), then diffs to detect leaks. A
    /// [`std::collections::BTreeSet`] is returned for deterministic iteration
    /// ordering matching the workspace convention.
    pub(crate) fn snapshot_resource_ids(&self) -> std::collections::BTreeSet<TypeId> {
        self.resources.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, Severity};

    use super::*;

    #[test]
    fn plugin_context_emit_diagnostic_routes_to_sink() {
        let mut agg = DiagnosticAggregator::new();
        {
            let mut ctx = PluginContext::new(&mut agg);
            ctx.emit_diagnostic(Diagnostic::error("plugin oops"));
            ctx.emit_diagnostic(Diagnostic::warning("plugin warn"));
        }
        assert_eq!(agg.len(), 2);
        assert!(agg.has_errors());
        assert_eq!(agg.highest_severity(), Some(Severity::Error));
        let messages: Vec<&str> = agg.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, ["plugin oops", "plugin warn"]);
    }

    #[test]
    fn plugin_context_diagnostics_returns_borrow_for_advanced_use() {
        fn helper(sink: &mut dyn DiagnosticSink) {
            sink.emit(Diagnostic::info("from helper"));
        }

        let mut agg = DiagnosticAggregator::new();
        {
            let mut ctx = PluginContext::new(&mut agg);
            helper(ctx.diagnostics());
        }
        assert_eq!(agg.len(), 1);
        assert_eq!(agg.iter().next().unwrap().message, "from helper");
    }

    // === New v1 resource-registry tests ===

    #[test]
    fn plugin_context_starts_with_empty_resources() {
        let mut agg = DiagnosticAggregator::new();
        let ctx = PluginContext::new(&mut agg);
        assert_eq!(ctx.resource_count(), 0);
        assert!(!ctx.contains::<u32>());
        assert!(!ctx.contains::<String>());
    }

    #[test]
    fn plugin_context_insert_then_get_mut_round_trips() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);

        let prior = ctx.insert(42u32);
        assert!(prior.is_none(), "no prior value");
        assert_eq!(ctx.resource_count(), 1);
        assert!(ctx.contains::<u32>());

        let r = ctx.get_mut::<u32>().expect("present");
        assert_eq!(*r, 42);
        *r = 7;

        let r2 = ctx.get_mut::<u32>().expect("still present");
        assert_eq!(*r2, 7);
    }

    #[test]
    fn plugin_context_take_removes_from_registry() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);

        let _ = ctx.insert(99u32);
        assert!(ctx.contains::<u32>());

        let taken = ctx.take::<u32>();
        assert_eq!(taken, Some(99));
        assert!(!ctx.contains::<u32>(), "slot must be empty after take");
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn plugin_context_insert_replaces_previous_returning_old() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);

        assert_eq!(ctx.insert(1u32), None, "first insert returns None");
        assert_eq!(
            ctx.insert(2u32),
            Some(1u32),
            "second insert returns prior 1"
        );
        assert_eq!(
            ctx.resource_count(),
            1,
            "still exactly one u32 slot occupied"
        );
        let cur = ctx.get_mut::<u32>().expect("present");
        assert_eq!(*cur, 2);
    }

    #[test]
    fn plugin_context_get_mut_for_missing_type_returns_none() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);
        // No panic, just None.
        assert!(ctx.get_mut::<u32>().is_none());
        assert!(ctx.get_mut::<String>().is_none());
    }

    #[test]
    fn plugin_context_take_for_missing_type_returns_none() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);
        assert!(ctx.take::<u32>().is_none());
        assert!(ctx.take::<String>().is_none());
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn plugin_context_with_resource_builder_chains() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg)
            .with_resource(42u32)
            .with_resource(String::from("hello"));

        assert_eq!(ctx.resource_count(), 2);
        assert!(ctx.contains::<u32>());
        assert!(ctx.contains::<String>());

        assert_eq!(ctx.get_mut::<u32>().copied(), Some(42));
        assert_eq!(ctx.get_mut::<String>().map(|s| s.as_str()), Some("hello"));
    }

    #[test]
    fn plugin_context_distinct_types_dont_collide() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);
        assert!(ctx.insert(42u32).is_none());
        assert!(ctx.insert(42i64).is_none());
        assert!(ctx.insert(String::from("x")).is_none());

        assert_eq!(ctx.resource_count(), 3);
        assert_eq!(ctx.get_mut::<u32>().copied(), Some(42u32));
        assert_eq!(ctx.get_mut::<i64>().copied(), Some(42i64));
        assert_eq!(ctx.get_mut::<String>().map(|s| s.as_str()), Some("x"));
    }

    #[test]
    fn plugin_context_resource_count_tracks_inserts_and_takes() {
        let mut agg = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut agg);

        assert_eq!(ctx.resource_count(), 0);
        let _ = ctx.insert(1u32);
        assert_eq!(ctx.resource_count(), 1);
        let _ = ctx.insert(2i64);
        assert_eq!(ctx.resource_count(), 2);
        let _ = ctx.insert(3i32);
        assert_eq!(ctx.resource_count(), 3);

        // Replacing same type doesn't grow the count.
        let _ = ctx.insert(99u32);
        assert_eq!(ctx.resource_count(), 3);

        let _ = ctx.take::<u32>();
        assert_eq!(ctx.resource_count(), 2);
        let _ = ctx.take::<i64>();
        assert_eq!(ctx.resource_count(), 1);
        let _ = ctx.take::<i32>();
        assert_eq!(ctx.resource_count(), 0);

        // Repeated takes of the same type after empty are still no-ops.
        assert!(ctx.take::<u32>().is_none());
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn plugin_context_existing_diagnostic_api_still_works_with_resources_inserted() {
        let mut agg = DiagnosticAggregator::new();
        {
            let mut ctx = PluginContext::new(&mut agg);
            // Resources present alongside diagnostics shouldn't break the
            // diagnostic path.
            assert!(ctx.insert(42u32).is_none());
            assert!(ctx.insert(String::from("payload")).is_none());

            ctx.emit_diagnostic(Diagnostic::error("with-resources"));
            // diagnostics() borrow still functions.
            ctx.diagnostics().emit(Diagnostic::warning("via-borrow"));

            assert_eq!(ctx.resource_count(), 2);
        }
        assert_eq!(agg.len(), 2);
        let messages: Vec<&str> = agg.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, ["with-resources", "via-borrow"]);
    }
}
