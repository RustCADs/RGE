//! Diagnostic severity levels, ordered by escalation.

use serde::{Deserialize, Serialize};

/// Diagnostic severity ordered by escalation.
///
/// The discriminant values are stable: `Suggestion = 0`, `Info = 1`,
/// `Warning = 2`, `Error = 3`. `PartialOrd` / `Ord` reflect this ordering so
/// that `Severity::Suggestion < Severity::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// A non-actionable hint that a better approach may be available.
    Suggestion = 0,
    /// Informational message; no action required.
    Info = 1,
    /// Something is likely wrong but the operation can continue.
    Warning = 2,
    /// A hard error; the operation cannot produce valid output.
    Error = 3,
}

impl Severity {
    /// Returns the lower-case label for this severity level, suitable for
    /// display in terminal output and log lines.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Suggestion => "suggestion",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_escalating() {
        assert!(Severity::Suggestion < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn ord_is_total() {
        let mut v = vec![
            Severity::Error,
            Severity::Suggestion,
            Severity::Info,
            Severity::Warning,
        ];
        v.sort();
        assert_eq!(
            v,
            [
                Severity::Suggestion,
                Severity::Info,
                Severity::Warning,
                Severity::Error
            ]
        );
    }

    #[test]
    fn labels_are_correct() {
        assert_eq!(Severity::Suggestion.label(), "suggestion");
        assert_eq!(Severity::Info.label(), "info");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Error.label(), "error");
    }
}
