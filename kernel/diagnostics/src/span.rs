//! Source-location types that anchor a diagnostic to its origin.

use serde::{Deserialize, Serialize};

/// A source location within a file (file name, 1-based line, 1-based column).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLoc {
    /// The file name or path (UTF-8, not required to be absolute).
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number. Use `1` when column precision is unavailable.
    pub column: u32,
}

impl SourceLoc {
    /// Construct a [`SourceLoc`] from its components.
    #[must_use]
    pub fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

/// Where in the input space a diagnostic originates.
///
/// All four fields are optional; at least one should be populated in any
/// concrete `Span` worth emitting. The builder methods (`at_*` / `with_*`)
/// make it easy to populate only the fields that are meaningful.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// File, line, and column — the classic source location.
    pub source: Option<SourceLoc>,
    /// Identifier of a graph node (any graph: CAD, material, anim, script).
    pub graph_node: Option<String>,
    /// 1-based line number within a script buffer.
    pub script_line: Option<u32>,
    /// Content-addressed asset path.
    pub asset_path: Option<String>,
}

impl Span {
    /// Construct an empty span (all fields `None`).
    ///
    /// Use the `with_*` builder methods to populate fields, or start from one
    /// of the convenience constructors (`at_file`, `at_graph_node`, …).
    #[must_use]
    pub fn new() -> Self {
        Self {
            source: None,
            graph_node: None,
            script_line: None,
            asset_path: None,
        }
    }

    /// Construct a span pinned to a file location.
    #[must_use]
    pub fn at_file(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self::new().with_source(file, line, column)
    }

    /// Construct a span pinned to a graph node ID.
    #[must_use]
    pub fn at_graph_node(node: impl Into<String>) -> Self {
        Self::new().with_graph_node(node)
    }

    /// Construct a span pinned to a 1-based script line number.
    #[must_use]
    pub fn at_script_line(line: u32) -> Self {
        Self::new().with_script_line(line)
    }

    /// Construct a span pinned to a content-addressed asset path.
    #[must_use]
    pub fn at_asset(path: impl Into<String>) -> Self {
        Self::new().with_asset_path(path)
    }

    /// Set the file/line/column source location, consuming and returning `self`.
    #[must_use]
    pub fn with_source(mut self, file: impl Into<String>, line: u32, column: u32) -> Self {
        self.source = Some(SourceLoc::new(file, line, column));
        self
    }

    /// Set the graph-node ID, consuming and returning `self`.
    #[must_use]
    pub fn with_graph_node(mut self, node: impl Into<String>) -> Self {
        self.graph_node = Some(node.into());
        self
    }

    /// Set the script-line number, consuming and returning `self`.
    #[must_use]
    pub fn with_script_line(mut self, line: u32) -> Self {
        self.script_line = Some(line);
        self
    }

    /// Set the asset path, consuming and returning `self`.
    #[must_use]
    pub fn with_asset_path(mut self, path: impl Into<String>) -> Self {
        self.asset_path = Some(path.into());
        self
    }

    /// Returns `true` when all four fields are `None` (the span carries no location).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.source.is_none()
            && self.graph_node.is_none()
            && self.script_line.is_none()
            && self.asset_path.is_none()
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        assert!(Span::new().is_empty());
    }

    #[test]
    fn at_file_populates_source() {
        let s = Span::at_file("foo.rs", 10, 3);
        assert!(!s.is_empty());
        let loc = s.source.unwrap();
        assert_eq!(loc.file, "foo.rs");
        assert_eq!(loc.line, 10);
        assert_eq!(loc.column, 3);
    }

    #[test]
    fn at_graph_node_populates_graph_node() {
        let s = Span::at_graph_node("mat::albedo");
        assert_eq!(s.graph_node.as_deref(), Some("mat::albedo"));
        assert!(s.source.is_none());
    }

    #[test]
    fn at_script_line_populates_script_line() {
        let s = Span::at_script_line(42);
        assert_eq!(s.script_line, Some(42));
    }

    #[test]
    fn at_asset_populates_asset_path() {
        let s = Span::at_asset("assets/textures/rock.png");
        assert_eq!(s.asset_path.as_deref(), Some("assets/textures/rock.png"));
    }

    #[test]
    fn with_builders_chain_correctly() {
        let s = Span::new()
            .with_source("bar.rs", 1, 1)
            .with_graph_node("node-7")
            .with_script_line(3)
            .with_asset_path("a/b/c");
        assert!(s.source.is_some());
        assert!(s.graph_node.is_some());
        assert!(s.script_line.is_some());
        assert!(s.asset_path.is_some());
        assert!(!s.is_empty());
    }
}
