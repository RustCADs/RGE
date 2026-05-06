//! [`SubscriptionId`] — opaque subscriber token issued by [`EventBus`].
//!
//! [`EventBus`]: crate::EventBus

use serde::{Deserialize, Serialize};

/// Opaque token identifying a single subscription registered with [`EventBus`].
///
/// IDs are increment-only and globally unique within a bus instance. They are
/// used for subscription tracking and diagnostics only — the bus does not
/// invoke callbacks. Consumers iterate channels directly.
///
/// [`EventBus`]: crate::EventBus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    /// Construct a [`SubscriptionId`] from a raw counter value.
    ///
    /// Intended for internal use by [`EventBus`]; external callers should
    /// prefer to obtain IDs via [`EventBus::subscribe`].
    ///
    /// [`EventBus`]: crate::EventBus
    /// [`EventBus::subscribe`]: crate::EventBus::subscribe
    #[must_use]
    pub(crate) fn from_raw(n: u64) -> Self {
        Self(n)
    }

    /// Returns the underlying `u64` counter value.
    ///
    /// Useful for logging, debugging, or serialization. The value is
    /// monotonically increasing within a bus instance.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trips() {
        let id = SubscriptionId::from_raw(7);
        assert_eq!(id.raw(), 7);
    }

    #[test]
    fn ids_are_value_equal() {
        let a = SubscriptionId::from_raw(1);
        let b = SubscriptionId::from_raw(1);
        assert_eq!(a, b);
    }

    #[test]
    fn ids_are_copy() {
        let a = SubscriptionId::from_raw(3);
        let b = a; // copy
        assert_eq!(a, b);
    }
}
