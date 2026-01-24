//! Execution graph loading from .work/stages/ or plan file.
//!
//! This module re-exports the shared graph loading implementation from plan::graph::loader.

// Re-export the shared implementation
pub use crate::plan::graph::build_execution_graph;
