//! Integration tests for `rge-kernel-diagnostics`.
//!
//! Covers: span builders, severity ordering, diagnostic builders, aggregator
//! behaviour, trait-object safety, no-op sink, display format, and serde
//! round-trip via RON.

use rge_kernel_diagnostics::{
    Diagnostic, DiagnosticAggregator, DiagnosticSink, FailureClass, Severity, Span, Suggestion,
};

// ---------------------------------------------------------------------------
// 1. Span builders
// ---------------------------------------------------------------------------

#[test]
fn span_new_is_empty() {
    assert!(Span::new().is_empty());
}

#[test]
fn span_at_file_sets_source_only() {
    let s = Span::at_file("main.rs", 7, 3);
    assert!(!s.is_empty());
    let loc = s.source.as_ref().unwrap();
    assert_eq!(loc.file, "main.rs");
    assert_eq!(loc.line, 7);
    assert_eq!(loc.column, 3);
    assert!(s.graph_node.is_none());
    assert!(s.script_line.is_none());
    assert!(s.asset_path.is_none());
}

#[test]
fn span_at_graph_node_sets_graph_node_only() {
    let s = Span::at_graph_node("anim::root");
    assert_eq!(s.graph_node.as_deref(), Some("anim::root"));
    assert!(s.source.is_none());
}

#[test]
fn span_at_script_line_sets_script_line_only() {
    let s = Span::at_script_line(99);
    assert_eq!(s.script_line, Some(99));
    assert!(s.source.is_none());
}

#[test]
fn span_at_asset_sets_asset_path_only() {
    let s = Span::at_asset("pkg://textures/rock.png");
    assert_eq!(s.asset_path.as_deref(), Some("pkg://textures/rock.png"));
}

#[test]
fn span_with_builders_chain_and_is_not_empty() {
    let s = Span::new()
        .with_source("lib.rs", 1, 1)
        .with_graph_node("g1")
        .with_script_line(5)
        .with_asset_path("a/b");
    assert!(!s.is_empty());
    assert!(s.source.is_some());
    assert!(s.graph_node.is_some());
    assert!(s.script_line.is_some());
    assert!(s.asset_path.is_some());
}

// ---------------------------------------------------------------------------
// 2. Severity ordering
// ---------------------------------------------------------------------------

#[test]
fn severity_ordering_is_escalating() {
    assert!(Severity::Suggestion < Severity::Info);
    assert!(Severity::Info < Severity::Warning);
    assert!(Severity::Warning < Severity::Error);
}

#[test]
fn severity_cmp_is_total() {
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

// ---------------------------------------------------------------------------
// 3. Diagnostic builders
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_error_sets_severity_error() {
    assert_eq!(Diagnostic::error("e").severity, Severity::Error);
}

#[test]
fn diagnostic_warning_sets_severity_warning() {
    assert_eq!(Diagnostic::warning("w").severity, Severity::Warning);
}

#[test]
fn diagnostic_info_sets_severity_info() {
    assert_eq!(Diagnostic::info("i").severity, Severity::Info);
}

#[test]
fn diagnostic_suggestion_sets_severity_suggestion() {
    assert_eq!(Diagnostic::suggestion("s").severity, Severity::Suggestion);
}

#[test]
fn diagnostic_with_span_sets_span() {
    let span = Span::at_file("a.rs", 3, 2);
    let d = Diagnostic::error("e").with_span(span.clone());
    assert_eq!(d.span, span);
}

#[test]
fn diagnostic_with_failure_class_sets_class() {
    let d = Diagnostic::error("e").with_failure_class(FailureClass::SnapshotRecoverable);
    assert_eq!(d.failure_class, Some(FailureClass::SnapshotRecoverable));
}

#[test]
fn diagnostic_with_suggestion_sets_suggestion() {
    let sugg = Suggestion::new("use `foo` instead");
    let d = Diagnostic::warning("w").with_suggestion(sugg);
    assert_eq!(d.suggestion.as_ref().unwrap().message, "use `foo` instead");
    assert!(d.suggestion.as_ref().unwrap().replacement.is_none());
}

#[test]
fn diagnostic_builders_chain_fully() {
    let d = Diagnostic::error("full chain")
        .with_span(Span::at_file("x.rs", 10, 1))
        .with_failure_class(FailureClass::SessionFatal)
        .with_suggestion(Suggestion::with_replacement("fix it", "fixed_value"));
    assert_eq!(d.severity, Severity::Error);
    assert!(d.span.source.is_some());
    assert_eq!(d.failure_class, Some(FailureClass::SessionFatal));
    let sugg = d.suggestion.unwrap();
    assert_eq!(sugg.message, "fix it");
    assert_eq!(sugg.replacement.as_deref(), Some("fixed_value"));
}

// ---------------------------------------------------------------------------
// 4. Aggregator
// ---------------------------------------------------------------------------

#[test]
fn aggregator_starts_empty() {
    let agg = DiagnosticAggregator::new();
    assert!(agg.is_empty());
    assert_eq!(agg.len(), 0);
    assert_eq!(agg.highest_severity(), None);
    assert!(!agg.has_errors());
}

#[test]
fn aggregator_emits_in_order() {
    let mut agg = DiagnosticAggregator::new();
    agg.emit(Diagnostic::info("first"));
    agg.emit(Diagnostic::error("second"));
    let msgs: Vec<&str> = agg.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(msgs, ["first", "second"]);
}

#[test]
fn aggregator_highest_severity_is_max() {
    let mut agg = DiagnosticAggregator::new();
    agg.emit(Diagnostic::warning("w"));
    assert_eq!(agg.highest_severity(), Some(Severity::Warning));
    agg.emit(Diagnostic::error("e"));
    assert_eq!(agg.highest_severity(), Some(Severity::Error));
}

#[test]
fn aggregator_has_errors_true_iff_any_error() {
    let mut agg = DiagnosticAggregator::new();
    agg.emit(Diagnostic::warning("w"));
    assert!(!agg.has_errors());
    agg.emit(Diagnostic::error("e"));
    assert!(agg.has_errors());
}

#[test]
fn aggregator_at_least_filters_warning_and_above() {
    let mut agg = DiagnosticAggregator::new();
    agg.emit(Diagnostic::suggestion("s"));
    agg.emit(Diagnostic::info("i"));
    agg.emit(Diagnostic::warning("w"));
    agg.emit(Diagnostic::error("e"));

    let results: Vec<_> = agg.at_least(Severity::Warning).collect();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|d| d.severity >= Severity::Warning));
}

#[test]
fn aggregator_len_and_is_empty_track_state() {
    let mut agg = DiagnosticAggregator::new();
    assert!(agg.is_empty());
    agg.emit(Diagnostic::info("x"));
    assert!(!agg.is_empty());
    assert_eq!(agg.len(), 1);
    agg.emit(Diagnostic::info("y"));
    assert_eq!(agg.len(), 2);
}

// ---------------------------------------------------------------------------
// 5. Sink trait-object safety
// ---------------------------------------------------------------------------

#[test]
fn aggregator_works_as_dyn_diagnostic_sink() {
    fn fill(sink: &mut dyn DiagnosticSink) {
        sink.emit(Diagnostic::error("via dyn"));
    }
    let mut agg = DiagnosticAggregator::new();
    fill(&mut agg);
    assert!(agg.has_errors());
}

// ---------------------------------------------------------------------------
// 6. () no-op sink
// ---------------------------------------------------------------------------

#[test]
fn unit_no_op_sink_emits_silently() {
    let mut sink = ();
    sink.emit(Diagnostic::error("dropped silently"));
    // Test passes if there is no panic or compilation error.
}

// ---------------------------------------------------------------------------
// 7. Display format
// ---------------------------------------------------------------------------

#[test]
fn display_with_source_loc_format() {
    let d = Diagnostic::error("bad shader").with_span(Span::at_file("foo.rs", 12, 5));
    assert_eq!(d.to_string(), "[error] foo.rs:12:5: bad shader");
}

#[test]
fn display_without_location_uses_placeholder() {
    let d = Diagnostic::warning("possible issue");
    assert_eq!(d.to_string(), "[warning] <no location>: possible issue");
}

// ---------------------------------------------------------------------------
// 8. Serde round-trip via RON
// ---------------------------------------------------------------------------

#[test]
fn round_trip_via_ron() {
    let original = Diagnostic::error("serde check")
        .with_span(
            Span::new()
                .with_source("test.rs", 5, 10)
                .with_graph_node("node-42")
                .with_script_line(7)
                .with_asset_path("pkg://a/b/c"),
        )
        .with_failure_class(FailureClass::Recoverable)
        .with_suggestion(Suggestion::with_replacement("use bar", "bar"));

    let serialized = ron::to_string(&original).expect("serialize");
    let deserialized: Diagnostic = ron::from_str(&serialized).expect("deserialize");
    assert_eq!(original, deserialized);
}
