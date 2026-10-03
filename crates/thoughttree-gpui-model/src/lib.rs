//! Desktop-independent Graph semantics and the shared `.thoughttree` format.
//!
//! File IO belongs to the guarded Project service in `thoughttree-core`.
//! This crate only produces snapshots; it never writes a Project file.

mod editor;
mod graph;
mod import;
mod layout;
mod project;
mod provenance;
mod search;
mod types;

pub use editor::*;
pub use graph::*;
pub use import::*;
pub use layout::*;
pub use project::*;
pub use provenance::*;
pub use search::*;
pub use types::*;

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or_default()
}
