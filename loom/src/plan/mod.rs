//! Plan parsing and schema validation
//!
//! This module handles:
//! - Parsing plan documents (markdown with YAML metadata)
//! - Validating stage definitions
//! - Building execution graphs

pub mod graph;
pub mod parser;
pub mod schema;

// Re-export commonly used types
pub use graph::{ExecutionGraph, NodeStatus, StageNode};
pub use parser::{parse_plan, parse_plan_content, ParsedPlan};
pub use schema::{validate, LoomConfig, LoomMetadata, StageDefinition, ValidationError};
