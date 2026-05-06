//! `cad_core::graph` — operator DAG built on `kernel/graph-foundation`.
//!
//! Failure class: snapshot-recoverable

pub mod operator_graph;

pub use operator_graph::{EvalError, GraphBuildError, OperatorGraph};
