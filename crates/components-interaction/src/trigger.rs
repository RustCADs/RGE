//! [`Trigger`] — "this collider is a volume that emits events on entry/exit".
//!
//! The actual handler runs in `crates/physics` (W11) — this component just
//! flips the `is_sensor` bit on the underlying rapier collider and records
//! which event channels to fire. The PLAN's §1.5.1 trigger-volume role
//! requires `Trigger` plus `Collider` plus `Transform`.

use serde::{Deserialize, Serialize};

/// Trigger-volume configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trigger {
    /// Emit a `TriggerEntered` event when an entity enters this volume.
    pub on_enter: bool,
    /// Emit a `TriggerExited` event when an entity leaves this volume.
    pub on_exit: bool,
}

impl Trigger {
    /// Both entry and exit events enabled (the typical case).
    pub const BOTH: Trigger = Trigger {
        on_enter: true,
        on_exit: true,
    };

    /// Entry-only trigger (one-shot pickup, etc.).
    pub const ENTER_ONLY: Trigger = Trigger {
        on_enter: true,
        on_exit: false,
    };
}

impl Default for Trigger {
    fn default() -> Self {
        Self::BOTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_both() {
        let t = Trigger::BOTH;
        let s = ron::to_string(&t).expect("serialize");
        let back: Trigger = ron::from_str(&s).expect("deserialize");
        assert_eq!(t, back);
    }

    #[test]
    fn round_trip_ron_enter_only() {
        let t = Trigger::ENTER_ONLY;
        let s = ron::to_string(&t).expect("serialize");
        let back: Trigger = ron::from_str(&s).expect("deserialize");
        assert_eq!(t, back);
    }
}
