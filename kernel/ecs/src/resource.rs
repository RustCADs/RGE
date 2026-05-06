//! [`Res<T>`] — a handle for non-component shared world state.

use std::ops::Deref;

// ---------------------------------------------------------------------------
// Res<T>
// ---------------------------------------------------------------------------

/// An immutable handle to a world resource of type `T`.
///
/// Resources are non-component global values inserted via
/// [`World::insert_resource`](crate::world::World::insert_resource) and
/// accessed via [`World::resource`](crate::world::World::resource).
///
/// Derefs transparently to `T`.
pub struct Res<'w, T: 'static> {
    value: &'w T,
}

impl<'w, T: 'static> Res<'w, T> {
    /// Construct a `Res` handle wrapping a reference.
    #[must_use]
    pub(crate) fn new(value: &'w T) -> Self {
        Self { value }
    }

    /// Get the inner reference.
    #[must_use]
    pub fn inner(&self) -> &T {
        self.value
    }
}

impl<T: 'static> Deref for Res<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::world::World;

    struct Config {
        max_fps: u32,
    }

    #[test]
    fn resource_insert_get() {
        let mut world = World::new();
        world.insert_resource(Config { max_fps: 60 });
        let res = world.resource::<Config>().unwrap();
        assert_eq!(res.max_fps, 60);
    }

    #[test]
    fn resource_remove() {
        let mut world = World::new();
        world.insert_resource(Config { max_fps: 30 });
        let removed = world.remove_resource::<Config>().unwrap();
        assert_eq!(removed.max_fps, 30);
        assert!(world.resource::<Config>().is_none());
    }
}
