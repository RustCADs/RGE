//! Component trait and [`ComponentId`] newtype.

use std::any::{Any, TypeId};

// ---------------------------------------------------------------------------
// Component trait
// ---------------------------------------------------------------------------

/// Marker trait for ECS components.
///
/// Every component must be `'static + Send + Sync + Any`.  No derive macro is
/// available yet (Phase-1.x `macros-reflect` integration is deferred).
/// Implement manually:
///
/// ```rust
/// # use rge_kernel_ecs::Component;
/// struct Health(f32);
/// impl Component for Health {}
/// ```
pub trait Component: Any + Send + Sync + 'static {}

// ---------------------------------------------------------------------------
// ComponentId
// ---------------------------------------------------------------------------

/// A stable, type-erased identifier for a component type.
///
/// Backed by [`std::any::TypeId`], which is unique per type for the lifetime
/// of the process.  Two components of different Rust types always have
/// different [`ComponentId`]s even if their memory layouts are identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(TypeId);

impl ComponentId {
    /// Create the [`ComponentId`] for component type `C`.
    #[must_use]
    pub fn of<C: Component>() -> Self {
        Self(TypeId::of::<C>())
    }

    /// Return the underlying [`TypeId`].
    #[must_use]
    pub fn type_id(self) -> TypeId {
        self.0
    }
}

impl From<TypeId> for ComponentId {
    fn from(id: TypeId) -> Self {
        Self(id)
    }
}
