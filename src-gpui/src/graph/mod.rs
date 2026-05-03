//! Core domain model for the ThoughtTree DAG.
//!
//! Mirrors `src/lib/graph/` from the React frontend. This module is pure Rust
//! with no GPUI dependencies — the same model could back any front end.

pub mod model;
pub mod mutations;
pub mod types;

pub use model::GraphModel;
pub use mutations::GraphMutations;
pub use types::{Graph, GraphEdge, GraphNode, NodeId, NodeRole, Position};
