//! [`DiagnosticAggregator`] — collect diagnostics without fail-fast.

use crate::{Diagnostic, DiagnosticSink, Severity};

/// Collects diagnostics in insertion order without interrupting the caller.
///
/// Implements [`DiagnosticSink`], so it can be passed as `&mut dyn
/// DiagnosticSink` anywhere the trait is expected. After the operation
/// completes, inspect the collected diagnostics with the query methods, or
/// drain them with [`into_inner`][Self::into_inner].
///
/// # Example
///
/// ```rust
/// use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink, Severity};
///
/// let mut agg = DiagnosticAggregator::new();
/// agg.emit(Diagnostic::warning("almost wrong"));
/// agg.emit(Diagnostic::error("definitely wrong"));
///
/// assert!(agg.has_errors());
/// assert_eq!(agg.highest_severity(), Some(Severity::Error));
/// assert_eq!(agg.len(), 2);
/// ```
#[derive(Debug, Default, Clone)]
pub struct DiagnosticAggregator {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticAggregator {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Iterate over collected diagnostics in emission order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    /// Consume the aggregator, returning the collected diagnostics.
    #[must_use]
    pub fn into_inner(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Returns the number of collected diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns `true` when no diagnostics have been collected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns the highest [`Severity`] seen, or `None` if the aggregator is
    /// empty.
    #[must_use]
    pub fn highest_severity(&self) -> Option<Severity> {
        self.diagnostics.iter().map(|d| d.severity).max()
    }

    /// Returns `true` when at least one [`Severity::Error`] diagnostic has
    /// been collected.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Iterate over diagnostics whose severity is at least `min`.
    ///
    /// Useful for filtering to `Warning`-and-above or `Error`-only views
    /// without allocating a new collection.
    pub fn at_least(&self, min: Severity) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.severity >= min)
    }
}

impl DiagnosticSink for DiagnosticAggregator {
    fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Diagnostic, Severity};

    #[test]
    fn new_is_empty() {
        let agg = DiagnosticAggregator::new();
        assert!(agg.is_empty());
        assert_eq!(agg.len(), 0);
        assert_eq!(agg.highest_severity(), None);
        assert!(!agg.has_errors());
    }

    #[test]
    fn emit_appends_in_order() {
        let mut agg = DiagnosticAggregator::new();
        agg.emit(Diagnostic::info("first"));
        agg.emit(Diagnostic::warning("second"));
        agg.emit(Diagnostic::error("third"));

        let msgs: Vec<&str> = agg.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(msgs, ["first", "second", "third"]);
        assert_eq!(agg.len(), 3);
        assert!(!agg.is_empty());
    }

    #[test]
    fn highest_severity_returns_max() {
        let mut agg = DiagnosticAggregator::new();
        agg.emit(Diagnostic::info("i"));
        agg.emit(Diagnostic::warning("w"));
        assert_eq!(agg.highest_severity(), Some(Severity::Warning));

        agg.emit(Diagnostic::error("e"));
        assert_eq!(agg.highest_severity(), Some(Severity::Error));
    }

    #[test]
    fn has_errors_true_iff_any_error() {
        let mut agg = DiagnosticAggregator::new();
        agg.emit(Diagnostic::warning("w"));
        assert!(!agg.has_errors());

        agg.emit(Diagnostic::error("e"));
        assert!(agg.has_errors());
    }

    #[test]
    fn at_least_filters_correctly() {
        let mut agg = DiagnosticAggregator::new();
        agg.emit(Diagnostic::suggestion("s"));
        agg.emit(Diagnostic::info("i"));
        agg.emit(Diagnostic::warning("w"));
        agg.emit(Diagnostic::error("e"));

        let at_warn: Vec<_> = agg.at_least(Severity::Warning).collect();
        assert_eq!(at_warn.len(), 2);
        assert!(at_warn.iter().all(|d| d.severity >= Severity::Warning));

        let at_error: Vec<_> = agg.at_least(Severity::Error).collect();
        assert_eq!(at_error.len(), 1);
        assert_eq!(at_error[0].severity, Severity::Error);
    }

    #[test]
    fn into_inner_drains() {
        let mut agg = DiagnosticAggregator::new();
        agg.emit(Diagnostic::error("oops"));
        let v = agg.into_inner();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn works_as_trait_object() {
        fn fill(sink: &mut dyn DiagnosticSink) {
            sink.emit(Diagnostic::error("from trait object"));
        }
        let mut agg = DiagnosticAggregator::new();
        fill(&mut agg);
        assert!(agg.has_errors());
    }
}
