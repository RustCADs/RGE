//! The [`Diagnostic`] struct and its [`Suggestion`] companion.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{FailureClass, Severity, Span};

/// An optional fix-it suggestion attached to a [`Diagnostic`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Human-readable description of the suggested fix.
    pub message: String,
    /// Optional replacement text that an editor can apply automatically.
    pub replacement: Option<String>,
}

impl Suggestion {
    /// Construct a suggestion with only a message (no automatic replacement).
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            replacement: None,
        }
    }

    /// Construct a suggestion with a message and an automatic replacement.
    #[must_use]
    pub fn with_replacement(message: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            replacement: Some(replacement.into()),
        }
    }
}

/// One structured diagnostic record.
///
/// Construct via the severity-named constructors (`error`, `warning`, `info`,
/// `suggestion`) and refine with the `with_*` builder methods.
///
/// # Example
///
/// ```rust
/// use rge_kernel_diagnostics::{Diagnostic, FailureClass, Span};
///
/// let d = Diagnostic::error("shader compilation failed")
///     .with_span(Span::at_file("pbr.wgsl", 42, 1))
///     .with_failure_class(FailureClass::Recoverable);
///
/// assert!(d.failure_class.is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// How severe this diagnostic is.
    pub severity: Severity,
    /// Optional failure class (typically `None` for `Info` / `Suggestion`).
    pub failure_class: Option<FailureClass>,
    /// Where in the input space this diagnostic originates.
    pub span: Span,
    /// Human-readable description of what went wrong (or what to note).
    pub message: String,
    /// Optional suggested fix.
    pub suggestion: Option<Suggestion>,
}

impl Diagnostic {
    fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            failure_class: None,
            span: Span::new(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Construct an `Error`-severity diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Construct a `Warning`-severity diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// Construct an `Info`-severity diagnostic.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Severity::Info, message)
    }

    /// Construct a `Suggestion`-severity diagnostic.
    #[must_use]
    pub fn suggestion(message: impl Into<String>) -> Self {
        Self::new(Severity::Suggestion, message)
    }

    /// Attach a [`Span`], consuming and returning `self`.
    #[must_use]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    /// Attach a [`FailureClass`], consuming and returning `self`.
    #[must_use]
    pub fn with_failure_class(mut self, class: FailureClass) -> Self {
        self.failure_class = Some(class);
        self
    }

    /// Attach a [`Suggestion`], consuming and returning `self`.
    #[must_use]
    pub fn with_suggestion(mut self, s: Suggestion) -> Self {
        self.suggestion = Some(s);
        self
    }
}

impl fmt::Display for Diagnostic {
    /// Formats as `[<severity>] <location>: <message>`.
    ///
    /// Location is derived from the `span`: if a [`crate::span::SourceLoc`] is
    /// present it renders as `file:line:col`; otherwise `<no location>` is
    /// used. Additional span fields (graph node, script line, asset path) are
    /// appended as `(key=value)` pairs when present.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] ", self.severity.label())?;

        // Primary location.
        if let Some(ref loc) = self.span.source {
            write!(f, "{}:{}:{}", loc.file, loc.line, loc.column)?;
        } else {
            write!(f, "<no location>")?;
        }

        // Secondary span fields.
        if let Some(ref node) = self.span.graph_node {
            write!(f, " (node={node})")?;
        }
        if let Some(line) = self.span.script_line {
            write!(f, " (script_line={line})")?;
        }
        if let Some(ref path) = self.span.asset_path {
            write!(f, " (asset={path})")?;
        }

        write!(f, ": {}", self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_severity() {
        assert_eq!(Diagnostic::error("e").severity, Severity::Error);
        assert_eq!(Diagnostic::warning("w").severity, Severity::Warning);
        assert_eq!(Diagnostic::info("i").severity, Severity::Info);
        assert_eq!(Diagnostic::suggestion("s").severity, Severity::Suggestion);
    }

    #[test]
    fn with_span_is_chainable() {
        let d = Diagnostic::error("x").with_span(Span::at_file("a.rs", 1, 1));
        assert!(d.span.source.is_some());
    }

    #[test]
    fn with_failure_class_is_chainable() {
        let d = Diagnostic::error("x").with_failure_class(FailureClass::Recoverable);
        assert_eq!(d.failure_class, Some(FailureClass::Recoverable));
    }

    #[test]
    fn with_suggestion_is_chainable() {
        let s = Suggestion::new("try this");
        let d = Diagnostic::warning("w").with_suggestion(s.clone());
        assert_eq!(d.suggestion.as_ref().unwrap().message, "try this");
    }

    #[test]
    fn display_with_source_loc() {
        let d = Diagnostic::error("bad thing").with_span(Span::at_file("foo.rs", 12, 5));
        assert_eq!(d.to_string(), "[error] foo.rs:12:5: bad thing");
    }

    #[test]
    fn display_no_location() {
        let d = Diagnostic::info("hello");
        assert_eq!(d.to_string(), "[info] <no location>: hello");
    }
}
