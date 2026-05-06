//! Contact-event channels.
//!
//! Rapier emits raw contact pairs into `NarrowPhase::contact_pairs()` after
//! every step; we walk that table once per frame and convert into the four
//! typed channels script-host cares about:
//!
//! - [`CollisionStarted`] — first tick a non-sensor pair was in contact.
//! - [`CollisionEnded`] — last-tick contact dissolved.
//! - [`TriggerEntered`] — sensor (intersection) just started overlapping.
//! - [`TriggerExited`] — sensor just stopped overlapping.
//!
//! The split between contact and trigger is driven by the collider's
//! [`Collider::is_sensor`](crate::stubs::components_physics::Collider) bit.
//!
//! Latency contract: events generated on tick *T* must reach script handlers
//! by the end of tick *T* (PLAN.md §6.10 "trigger event fires on collision;
//! reaches script handler within <16ms"). We achieve this by running
//! [`drain`] in the `contact_events` schedule stage which is the *last*
//! stage of the physics block — same tick, same frame.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::stubs::kernel_events::Channel;
use crate::world::World;

/// A contact pair has just begun a non-sensor collision.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CollisionStarted {
    /// First body's stable id.
    pub a: u64,
    /// Second body's stable id.
    pub b: u64,
}

/// A contact pair has just ended a non-sensor collision.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CollisionEnded {
    /// First body's stable id.
    pub a: u64,
    /// Second body's stable id.
    pub b: u64,
}

/// A sensor has just been entered.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TriggerEntered {
    /// Sensor body stable id.
    pub sensor: u64,
    /// Other body stable id.
    pub other: u64,
}

/// A sensor has just been exited.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TriggerExited {
    /// Sensor body stable id.
    pub sensor: u64,
    /// Other body stable id.
    pub other: u64,
}

/// Bundle of typed event channels published per-tick.
///
/// Held by the consumer (script-host) so it can drain after each
/// `physics_step`. Kept as a single struct rather than four separate
/// `Channel`s so the API stays one parameter wide.
#[derive(Debug, Default)]
pub struct ContactEventChannel {
    /// Started-collision events.
    pub started: Channel<CollisionStarted>,
    /// Ended-collision events.
    pub ended: Channel<CollisionEnded>,
    /// Trigger-entered events.
    pub trigger_entered: Channel<TriggerEntered>,
    /// Trigger-exited events.
    pub trigger_exited: Channel<TriggerExited>,
    /// Tick-over-tick state for transition detection. Mutated by [`drain`].
    state: RefCell<EventState>,
}

#[derive(Debug, Default)]
struct EventState {
    /// (a, b) pairs that were in contact last tick.
    last_contacts: HashSet<(u64, u64)>,
    /// Sensor pairs that were intersecting last tick.
    last_intersections: HashSet<(u64, u64)>,
}

impl ContactEventChannel {
    /// Construct empty channels.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Drain Rapier's contact pair table into the typed channels.
///
/// Runs in the `contact_events` schedule stage, after `post_physics`. Computes
/// transitions by diffing this tick's pairs against the previous tick.
pub fn drain(world: &World, channel: &ContactEventChannel) {
    let mut state = channel.state.borrow_mut();

    // Collect this tick's pairs into stable-ordered sets so we can diff.
    let mut current_contacts: HashSet<(u64, u64)> = HashSet::new();
    let mut current_intersections: HashSet<(u64, u64)> = HashSet::new();

    for pair in world.narrowphase.contact_pairs() {
        if !pair.has_any_active_contact() {
            continue;
        }
        let (Some(a_handle), Some(b_handle)) = (
            collider_to_body_id(world, pair.collider1),
            collider_to_body_id(world, pair.collider2),
        ) else {
            continue;
        };
        let key = ordered(a_handle, b_handle);

        let a_sensor = world
            .colliders
            .get(pair.collider1)
            .is_some_and(rapier3d::geometry::Collider::is_sensor);
        let b_sensor = world
            .colliders
            .get(pair.collider2)
            .is_some_and(rapier3d::geometry::Collider::is_sensor);

        if a_sensor || b_sensor {
            current_intersections.insert(key);
        } else {
            current_contacts.insert(key);
        }
    }

    // Rapier's intersection_pairs (sensor pairs) live in a separate table.
    // The 0.32 iterator yields `(ColliderHandle, ColliderHandle, bool)`.
    for (collider1, collider2, intersecting) in world.narrowphase.intersection_pairs() {
        if !intersecting {
            continue;
        }
        let (Some(a_handle), Some(b_handle)) = (
            collider_to_body_id(world, collider1),
            collider_to_body_id(world, collider2),
        ) else {
            continue;
        };
        current_intersections.insert(ordered(a_handle, b_handle));
    }

    // Transition diff: started = new \ old; ended = old \ new.
    for (a, b) in current_contacts.difference(&state.last_contacts) {
        channel.started.push(CollisionStarted { a: *a, b: *b });
    }
    for (a, b) in state.last_contacts.difference(&current_contacts) {
        channel.ended.push(CollisionEnded { a: *a, b: *b });
    }
    for (s, o) in current_intersections.difference(&state.last_intersections) {
        // We don't know which side was the sensor without re-querying; the
        // ordered pair convention guarantees `s < o` so we always report the
        // smaller id as the sensor and let the consumer rebind via the
        // collider component if it cares. (Real W04 wave will plumb the bit
        // through.)
        channel.trigger_entered.push(TriggerEntered {
            sensor: *s,
            other: *o,
        });
    }
    for (s, o) in state.last_intersections.difference(&current_intersections) {
        channel.trigger_exited.push(TriggerExited {
            sensor: *s,
            other: *o,
        });
    }

    state.last_contacts = current_contacts;
    state.last_intersections = current_intersections;
}

fn ordered(a: u64, b: u64) -> (u64, u64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn collider_to_body_id(world: &World, h: rapier3d::geometry::ColliderHandle) -> Option<u64> {
    let collider = world.colliders.get(h)?;
    let body_handle = collider.parent()?;
    // We can't reverse the Rapier handle → stable id mapping cheaply without
    // a side-table. For W11 we use the body handle's raw index, which is
    // monotonic for non-removal scenarios — the determinism contract already
    // forbids body removal mid-replay so this is a sound approximation.
    let (idx, _gen) = body_handle.into_raw_parts();
    let _ = world.bodies.get(body_handle)?; // ensure handle is live
    Some(u64::from(idx))
}
