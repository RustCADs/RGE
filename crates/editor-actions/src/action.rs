//! [`Action`] trait and associated types.

use rge_kernel_ecs::{EntityId, World};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ActionId
// ---------------------------------------------------------------------------

/// Stable identifier for an [`Action`] — used for coalescing target identity.
///
/// For example, `"transform.translate(entity=0x1234)"` — the same id within
/// the 500 ms coalesce window will merge two consecutive actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(pub String);

impl ActionId {
    /// Construct an [`ActionId`] from any string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// ActionResult
// ---------------------------------------------------------------------------

/// Errors returned by [`Action::apply`] and [`Action::revert`].
#[derive(Debug, thiserror::Error)]
pub enum ActionResult {
    /// The apply step failed with a human-readable message.
    #[error("apply failed: {0}")]
    ApplyFailed(String),
    /// The revert step failed with a human-readable message.
    #[error("revert failed: {0}")]
    RevertFailed(String),
    /// The target entity was not found in the world.
    #[error("entity {0:?} not found")]
    MissingEntity(EntityId),
}

// ---------------------------------------------------------------------------
// MergeOutcome
// ---------------------------------------------------------------------------

/// Outcome of attempting to merge two same-target [`Action`]s during coalescing.
#[derive(Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Successfully merged — drop `next`, keep this [`Action`] with merged state.
    Merged,
    /// Cannot merge (different targets / different operations) — keep both.
    Distinct,
}

// ---------------------------------------------------------------------------
// Action trait
// ---------------------------------------------------------------------------

/// One reversible editor mutation.
///
/// Implementors:
/// - encapsulate the source entity/component/handle they mutate
/// - implement [`apply`](Action::apply) to perform the mutation against
///   `&mut World`
/// - implement [`revert`](Action::revert) to undo the mutation byte-identically
/// - implement [`merge`](Action::merge) to coalesce with an adjacent
///   same-target [`Action`]
pub trait Action: Send + Sync + 'static {
    /// Stable name for diagnostics + audit-ledger payload (e.g. `"spawn-entity"`).
    fn name(&self) -> &str;

    /// Stable identifier for coalescing target. Same id within 500 ms coalesces.
    fn id(&self) -> ActionId;

    /// Apply the mutation.
    ///
    /// # Errors
    ///
    /// Returns [`ActionResult::MissingEntity`] when the target entity is absent,
    /// or [`ActionResult::ApplyFailed`] for any other apply-time failure.
    fn apply(&self, world: &mut World) -> Result<(), ActionResult>;

    /// Revert the mutation. After successful `revert`, the world is byte-identical
    /// to its pre-[`apply`](Action::apply) state for the affected components.
    ///
    /// # Errors
    ///
    /// Returns [`ActionResult::RevertFailed`] when the revert cannot be completed,
    /// or [`ActionResult::MissingEntity`] when the target entity is absent.
    fn revert(&self, world: &mut World) -> Result<(), ActionResult>;

    /// Try to merge `next` into self. Default: [`MergeOutcome::Distinct`] (no merging).
    ///
    /// Override to support coalescing. When [`MergeOutcome::Merged`] is returned,
    /// `self` holds the merged state and `next` is dropped.
    fn merge(&mut self, _next: &dyn Action) -> MergeOutcome {
        MergeOutcome::Distinct
    }

    /// Serialize for audit-ledger payload. Default: just the name as bytes.
    ///
    /// Override to capture parameters for richer replay diagnostics.
    fn payload(&self) -> Vec<u8> {
        self.name().as_bytes().to_vec()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unnecessary_literal_bound)]
mod tests {
    use rge_kernel_ecs::{Component, World};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Marker(u32);
    impl Component for Marker {}

    /// A trivial Action that inserts/removes a `Marker` component.
    struct InsertMarker {
        entity: EntityId,
        value: u32,
    }

    impl Action for InsertMarker {
        fn name(&self) -> &str {
            "insert-marker"
        }

        fn id(&self) -> ActionId {
            ActionId::new(format!("insert-marker(entity={:?})", self.entity))
        }

        fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
            if world.entity(self.entity).is_none() {
                return Err(ActionResult::MissingEntity(self.entity));
            }
            world.insert(self.entity, Marker(self.value));
            Ok(())
        }

        fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
            world.remove::<Marker>(self.entity);
            Ok(())
        }
    }

    #[test]
    fn action_id_display() {
        let id = ActionId::new("test.action(entity=42)");
        assert_eq!(id.to_string(), "test.action(entity=42)");
    }

    #[test]
    fn default_merge_is_distinct() {
        let mut w = World::new();
        let e = w.spawn();
        let mut a = InsertMarker {
            entity: e,
            value: 1,
        };
        let b = InsertMarker {
            entity: e,
            value: 2,
        };
        assert_eq!(a.merge(&b), MergeOutcome::Distinct);
    }

    #[test]
    fn default_payload_is_name_bytes() {
        let mut w = World::new();
        let e = w.spawn();
        let a = InsertMarker {
            entity: e,
            value: 1,
        };
        assert_eq!(a.payload(), b"insert-marker");
    }

    #[test]
    fn apply_missing_entity_returns_error() {
        let mut w = World::new();
        // Spawn and immediately despawn to get a now-invalid EntityId.
        let e = w.spawn();
        w.despawn(e);
        let a = InsertMarker {
            entity: e,
            value: 0,
        };
        assert!(matches!(
            a.apply(&mut w),
            Err(ActionResult::MissingEntity(_))
        ));
    }
}
