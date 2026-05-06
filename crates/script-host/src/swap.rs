//! State-preserving module swap — Phase 3.2 prototype.
//!
//! [`capture_state`] serializes [`Counter`] components from every live entity
//! to a RON blob. [`restore_state`] deserializes and re-inserts them after a
//! new module instance is loaded.
//!
//! # Prototype limitations
//!
//! Only [`Counter`] components are captured. Generalising the snapshot to
//! every reflected type requires type-erased archetype iteration in
//! `kernel/ecs` — a **Phase 4-Foundation** extension. The swap measurement
//! (steps 4-7) is what matters for the p95 < 100 ms gate; the protocol is
//! correct and minimal.
//!
//! # RON format
//!
//! ```ron
//! // component_snapshot field:
//! (counters: {"<i64-handle-as-string>": <counter-value-i64>, ...})
//! ```

use std::collections::HashMap;

use rge_kernel_ecs::World;
use rge_kernel_events::SubscriptionId;
use serde::{Deserialize, Serialize};

use crate::ecs_bridge::{entity_id_to_i64, Counter};

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors that can occur during a module swap.
#[derive(Debug, thiserror::Error)]
pub enum SwapError {
    /// Serializing component state to RON failed.
    #[error("capture failed: {0}")]
    Capture(String),
    /// Deserializing / restoring component state from RON failed.
    #[error("restore failed: {0}")]
    Restore(String),
}

// ---------------------------------------------------------------------------
// SwapPlan
// ---------------------------------------------------------------------------

/// State captured before a module swap; opaque blob that the new instance
/// restores after instantiation.
///
/// # Serialization
///
/// [`component_snapshot`] is a RON-encoded [`CounterSnapshot`] mapping
/// entity handle (i64 as string) to counter value. Stored as `Vec<u8>` to
/// keep the type opaque to callers.
///
/// [`component_snapshot`]: Self::component_snapshot
#[derive(Debug, Clone)]
pub struct SwapPlan {
    /// World tick at the moment of capture.
    pub captured_at_tick: u64,
    /// RON-serialized counter snapshot (opaque to callers).
    pub component_snapshot: Vec<u8>,
    /// Advisory: subscription IDs held by the old instance.
    pub event_subscriptions: Vec<SubscriptionId>,
}

// ---------------------------------------------------------------------------
// SwapResult
// ---------------------------------------------------------------------------

/// Outcome of a completed swap.
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// World tick when capture happened.
    pub captured_at_tick: u64,
    /// Number of Counter components successfully restored.
    pub restored_components: usize,
    /// Wall-clock duration of the swap window (capture + re-instantiate +
    /// restore) in milliseconds.
    pub swap_duration_ms: f64,
}

// ---------------------------------------------------------------------------
// Snapshot payload (internal, serde-able)
// ---------------------------------------------------------------------------

/// Internal serialization format for the component snapshot.
#[derive(Serialize, Deserialize)]
struct CounterSnapshot {
    /// Maps entity handle (i64 encoded as a String key) → counter value.
    counters: HashMap<String, i64>,
}

// ---------------------------------------------------------------------------
// capture_state
// ---------------------------------------------------------------------------

/// Capture the current [`Counter`] state of every live entity into a
/// [`SwapPlan`].
///
/// Uses [`World::query`] to walk Counter-bearing entities (O(n)). The result
/// is a RON blob in [`SwapPlan::component_snapshot`].
///
/// # Errors
///
/// Returns [`SwapError::Capture`] if RON serialization fails.
pub fn capture_state(world: &World) -> Result<SwapPlan, SwapError> {
    let tick = world.current_tick();

    let counters: HashMap<String, i64> = world
        .query::<Counter>()
        .map(|(id, c)| (entity_id_to_i64(id).to_string(), c.value))
        .collect();

    let snapshot = CounterSnapshot { counters };
    let ron_str = ron::to_string(&snapshot).map_err(|e| SwapError::Capture(e.to_string()))?;

    Ok(SwapPlan {
        captured_at_tick: tick,
        component_snapshot: ron_str.into_bytes(),
        event_subscriptions: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// restore_state
// ---------------------------------------------------------------------------

/// Restore [`Counter`] components from a [`SwapPlan`] into `world`.
///
/// For each `(handle, value)` pair in the snapshot, finds the matching entity
/// (by truncated i64 handle scan via `query::<Counter>()`) and re-inserts the
/// [`Counter`]. Entities that no longer exist are skipped silently.
///
/// Returns the number of components successfully restored.
///
/// # Errors
///
/// Returns [`SwapError::Restore`] if the RON blob cannot be parsed.
pub fn restore_state(world: &mut World, plan: &SwapPlan) -> Result<usize, SwapError> {
    let ron_str = std::str::from_utf8(&plan.component_snapshot)
        .map_err(|e| SwapError::Restore(format!("snapshot not utf-8: {e}")))?;

    let snapshot: CounterSnapshot =
        ron::from_str(ron_str).map_err(|e| SwapError::Restore(format!("ron parse: {e}")))?;

    // Collect all (handle, entity_id) pairs from the current world first,
    // to avoid borrow conflicts between query iteration and insert.
    let handle_map: HashMap<i64, rge_kernel_ecs::EntityId> = world
        .query::<Counter>()
        .map(|(id, _)| (entity_id_to_i64(id), id))
        .collect();

    let mut restored = 0usize;
    for (handle_str, value) in &snapshot.counters {
        let handle: i64 = handle_str
            .parse()
            .map_err(|e| SwapError::Restore(format!("bad handle key `{handle_str}`: {e}")))?;

        if let Some(&id) = handle_map.get(&handle) {
            world.insert(id, Counter { value: *value });
            restored += 1;
        }
        // Silently skip entities not found (despawned before restore).
    }

    Ok(restored)
}
