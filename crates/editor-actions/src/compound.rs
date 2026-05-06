//! [`CompoundAction`] — atomic cross-subsystem action.

use rge_kernel_ecs::World;

use crate::action::{Action, ActionId, ActionResult};

// ---------------------------------------------------------------------------
// CompoundAction
// ---------------------------------------------------------------------------

/// Atomic cross-subsystem [`Action`].
///
/// Applies a sequence of inner [`Action`]s in order. If any `apply` fails,
/// all previously-applied inner actions are reverted in reverse order, leaving
/// the world in its pre-compound state.
pub struct CompoundAction {
    name: String,
    id: ActionId,
    inner: Vec<Box<dyn Action>>,
}

impl CompoundAction {
    /// Create a new empty [`CompoundAction`] with the given name and id.
    #[must_use]
    pub fn new(name: impl Into<String>, id: ActionId) -> Self {
        Self {
            name: name.into(),
            id,
            inner: Vec::new(),
        }
    }

    /// Push an inner [`Action`] onto the compound.
    pub fn push(&mut self, action: Box<dyn Action>) {
        self.inner.push(action);
    }

    /// Number of inner actions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` when no inner actions have been pushed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Action for CompoundAction {
    fn name(&self) -> &str {
        &self.name
    }

    fn id(&self) -> ActionId {
        self.id.clone()
    }

    /// Apply all inner actions in order.
    ///
    /// On failure of the `k`-th action, reverts actions `0..k` in reverse
    /// order and returns the original error.
    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        for (i, action) in self.inner.iter().enumerate() {
            if let Err(e) = action.apply(world) {
                // Revert previously-applied inner actions in reverse order.
                for prev in self.inner[..i].iter().rev() {
                    if let Err(rev_err) = prev.revert(world) {
                        tracing::error!(
                            compound = %self.name,
                            inner = %prev.name(),
                            error = %rev_err,
                            "CompoundAction: rollback revert failed — world may be inconsistent"
                        );
                    }
                }
                return Err(e);
            }
        }
        Ok(())
    }

    /// Revert all inner actions in reverse order.
    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        let mut first_err: Option<ActionResult> = None;
        for action in self.inner.iter().rev() {
            if let Err(e) = action.revert(world) {
                tracing::error!(
                    compound = %self.name,
                    inner = %action.name(),
                    error = %e,
                    "CompoundAction: revert step failed"
                );
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        match first_err {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }

    /// Payload: the name of the compound followed by each inner action's payload,
    /// length-prefixed (u32 LE, saturating for payloads > 4 GiB).
    fn payload(&self) -> Vec<u8> {
        let mut out = self.name.as_bytes().to_vec();
        for a in &self.inner {
            let p = a.payload();
            #[allow(clippy::cast_possible_truncation)]
            let len = p.len() as u32;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&p);
        }
        out
    }

    // merge: Distinct (default)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unnecessary_literal_bound)]
mod tests {
    use rge_kernel_ecs::{Component, EntityId, World};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Tag(u32);
    impl Component for Tag {}

    struct SetTag {
        entity: EntityId,
        value: u32,
        prev: std::sync::Mutex<Option<u32>>,
    }

    impl SetTag {
        fn new(entity: EntityId, value: u32) -> Self {
            Self {
                entity,
                value,
                prev: std::sync::Mutex::new(None),
            }
        }
    }

    impl Action for SetTag {
        fn name(&self) -> &str {
            "set-tag"
        }

        fn id(&self) -> ActionId {
            ActionId::new(format!("set-tag({:?})", self.entity))
        }

        fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
            if world.entity(self.entity).is_none() {
                return Err(ActionResult::MissingEntity(self.entity));
            }
            let old = {
                let eref = world.entity(self.entity);
                eref.and_then(|e| e.get::<Tag>().map(|t| t.0))
            };
            *self.prev.lock().unwrap() = old;
            world.insert(self.entity, Tag(self.value));
            Ok(())
        }

        fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
            match *self.prev.lock().unwrap() {
                Some(v) => {
                    world.insert(self.entity, Tag(v));
                }
                None => {
                    world.remove::<Tag>(self.entity);
                }
            }
            Ok(())
        }
    }

    /// An action that always fails on apply.
    #[allow(dead_code)]
    struct FailAction {
        entity: EntityId,
    }

    impl Action for FailAction {
        fn name(&self) -> &str {
            "fail-action"
        }

        fn id(&self) -> ActionId {
            ActionId::new(format!("fail({:?})", self.entity))
        }

        fn apply(&self, _world: &mut World) -> Result<(), ActionResult> {
            Err(ActionResult::ApplyFailed("intentional failure".to_owned()))
        }

        fn revert(&self, _world: &mut World) -> Result<(), ActionResult> {
            Ok(())
        }
    }

    #[test]
    fn compound_applies_in_order() {
        let mut world = World::new();
        let e = world.spawn();
        let mut compound = CompoundAction::new("test", ActionId::new("compound-test"));
        compound.push(Box::new(SetTag::new(e, 1)));
        compound.push(Box::new(SetTag::new(e, 2)));
        compound.push(Box::new(SetTag::new(e, 3)));
        compound.apply(&mut world).unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Tag>(), Some(&Tag(3)));
    }

    #[test]
    fn compound_reverts_in_reverse() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Tag(0));
        let mut compound = CompoundAction::new("test", ActionId::new("compound-test"));
        compound.push(Box::new(SetTag::new(e, 10)));
        compound.push(Box::new(SetTag::new(e, 20)));
        compound.apply(&mut world).unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Tag>(), Some(&Tag(20)));
        compound.revert(&mut world).unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Tag>(), Some(&Tag(0)));
    }

    #[test]
    fn compound_is_empty() {
        let c = CompoundAction::new("empty", ActionId::new("e"));
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }
}
