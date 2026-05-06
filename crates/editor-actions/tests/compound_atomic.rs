//! `CompoundAction` atomicity test: if the 3rd inner action fails, the first two
//! are reverted.
#![allow(clippy::unnecessary_literal_bound)]

use rge_editor_actions::action::{Action, ActionId, ActionResult};
use rge_editor_actions::{CommandBus, CompoundAction};
use rge_kernel_ecs::{Component, EntityId, World};

#[derive(Debug, Clone, PartialEq)]
struct Counter(u32);
impl Component for Counter {}

/// Increment a `Counter` component on apply; decrement on revert.
struct IncrementCounter {
    entity: EntityId,
}

impl Action for IncrementCounter {
    fn name(&self) -> &str {
        "increment-counter"
    }

    fn id(&self) -> ActionId {
        ActionId::new(format!("increment-counter({:?})", self.entity))
    }

    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        let current = {
            let eref = world.entity(self.entity);
            eref.and_then(|e| e.get::<Counter>().map(|c| c.0))
                .unwrap_or(0)
        };
        world.insert(self.entity, Counter(current + 1));
        Ok(())
    }

    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        let current = {
            let eref = world.entity(self.entity);
            eref.and_then(|e| e.get::<Counter>().map(|c| c.0))
                .unwrap_or(0)
        };
        world.insert(self.entity, Counter(current.saturating_sub(1)));
        Ok(())
    }
}

/// An action that always fails on apply.
struct AlwaysFail;

impl Action for AlwaysFail {
    fn name(&self) -> &str {
        "always-fail"
    }

    fn id(&self) -> ActionId {
        ActionId::new("always-fail")
    }

    fn apply(&self, _world: &mut World) -> Result<(), ActionResult> {
        Err(ActionResult::ApplyFailed(
            "intentional failure for atomicity test".to_owned(),
        ))
    }

    fn revert(&self, _world: &mut World) -> Result<(), ActionResult> {
        Ok(())
    }
}

#[test]
fn compound_third_fails_first_two_reverted() {
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Counter(0));

    let mut compound = CompoundAction::new("three-increments", ActionId::new("three-increments"));
    compound.push(Box::new(IncrementCounter { entity })); // inner[0]
    compound.push(Box::new(IncrementCounter { entity })); // inner[1]
    compound.push(Box::new(AlwaysFail)); // inner[2] — fails

    let result = compound.apply(&mut world);
    assert!(
        result.is_err(),
        "CompoundAction must propagate the failure from the 3rd inner action"
    );

    // inner[0] and inner[1] were applied (Counter went 0→1→2), then the 3rd
    // apply failed → rollback: revert inner[1] (2→1) then inner[0] (1→0).
    assert_eq!(
        world.entity(entity).unwrap().get::<Counter>(),
        Some(&Counter(0)),
        "after failed compound apply, world must be back to pre-compound state"
    );
}

#[test]
fn compound_success_applies_all_three() {
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Counter(0));

    let mut compound = CompoundAction::new("three-increments", ActionId::new("three-increments"));
    compound.push(Box::new(IncrementCounter { entity }));
    compound.push(Box::new(IncrementCounter { entity }));
    compound.push(Box::new(IncrementCounter { entity }));

    compound.apply(&mut world).unwrap();
    assert_eq!(
        world.entity(entity).unwrap().get::<Counter>(),
        Some(&Counter(3))
    );
}

#[test]
fn compound_via_bus_undo_reverts_all() {
    let mut bus = CommandBus::new();
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Counter(0));

    let mut compound = CompoundAction::new("two-increments", ActionId::new("two-increments"));
    compound.push(Box::new(IncrementCounter { entity }));
    compound.push(Box::new(IncrementCounter { entity }));

    bus.submit(Box::new(compound), &mut world).unwrap();
    assert_eq!(
        world.entity(entity).unwrap().get::<Counter>(),
        Some(&Counter(2))
    );

    bus.undo(&mut world).unwrap();
    assert_eq!(
        world.entity(entity).unwrap().get::<Counter>(),
        Some(&Counter(0)),
        "undo of compound must revert all inner actions"
    );
}
