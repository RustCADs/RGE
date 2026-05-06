//! Failure-class taxonomy per PLAN.md §1.13.

use serde::{Deserialize, Serialize};

/// The five failure classes defined in PLAN.md §1.13.
///
/// A failure class is a *tag* carried by a [`Diagnostic`][crate::Diagnostic]
/// (via the `failure_class` field) that tells consumers how to respond. The
/// substrate itself does not enforce recovery semantics — callers decide what
/// to do with `SessionFatal` vs `Recoverable`.
///
/// Not every diagnostic carries a failure class; informational and suggestion
/// diagnostics typically leave it as `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureClass {
    /// The failure is local; the operation can retry or skip and continue.
    Recoverable,
    /// The current in-memory state is corrupt but a saved snapshot is valid.
    /// Session continues from the last known-good snapshot.
    SnapshotRecoverable,
    /// The plugin that raised this error must be unloaded; the host session
    /// continues without it.
    PluginFatal,
    /// The entire editor/runtime session must be torn down and restarted.
    SessionFatal,
    /// A fundamental kernel invariant was violated; the process must exit.
    KernelFatal,
}

impl FailureClass {
    /// Returns the kebab-case label for this failure class, matching the
    /// `//! Failure class: <label>` format required by the architecture lint.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Recoverable => "recoverable",
            Self::SnapshotRecoverable => "snapshot-recoverable",
            Self::PluginFatal => "plugin-fatal",
            Self::SessionFatal => "session-fatal",
            Self::KernelFatal => "kernel-fatal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip_to_expected_strings() {
        assert_eq!(FailureClass::Recoverable.label(), "recoverable");
        assert_eq!(
            FailureClass::SnapshotRecoverable.label(),
            "snapshot-recoverable"
        );
        assert_eq!(FailureClass::PluginFatal.label(), "plugin-fatal");
        assert_eq!(FailureClass::SessionFatal.label(), "session-fatal");
        assert_eq!(FailureClass::KernelFatal.label(), "kernel-fatal");
    }
}
