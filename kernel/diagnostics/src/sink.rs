//! The [`DiagnosticSink`] trait — object-safe emission target.

use crate::Diagnostic;

/// Object-safe sink for diagnostic emission.
///
/// Subsystems accept `&mut dyn DiagnosticSink` so the caller decides whether
/// to aggregate, stream to `tracing`, or discard diagnostics — without forcing
/// generics on every API.
///
/// The blanket implementation for `()` makes it convenient to pass a no-op
/// sink in tests or contexts where diagnostics are irrelevant.
pub trait DiagnosticSink {
    /// Emit one diagnostic. The sink owns what happens next: buffering, logging,
    /// printing, or silently dropping.
    fn emit(&mut self, diagnostic: Diagnostic);
}

/// `()` is a no-op sink — diagnostics are silently discarded.
///
/// Useful as a default in unit tests or stubs that do not care about output.
impl DiagnosticSink for () {
    fn emit(&mut self, _diagnostic: Diagnostic) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Diagnostic;

    #[test]
    fn unit_sink_accepts_any_diagnostic() {
        let mut sink: () = ();
        sink.emit(Diagnostic::error("ignored"));
        sink.emit(Diagnostic::info("also ignored"));
    }

    #[test]
    fn trait_object_is_object_safe() {
        // If DiagnosticSink were not object-safe this would fail to compile.
        fn takes_sink(sink: &mut dyn DiagnosticSink) {
            sink.emit(Diagnostic::warning("test"));
        }
        let mut sink = ();
        takes_sink(&mut sink);
    }
}
