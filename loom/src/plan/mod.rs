//! Plan parsing and schema validation
//!
//! This module handles:
//! - Parsing plan documents (markdown with YAML metadata)
//! - Validating stage definitions
//! - Building execution graphs

pub mod amendment;
pub mod graph;
pub mod parser;
pub mod schema;

#[cfg(test)]
pub mod tests;

// Re-export commonly used types
pub use amendment::{
    apply_amendment, verify_plan_versions_consistency, AmendmentField, AmendmentPatch,
    AmendmentRequest, AmendmentResult,
};
pub use graph::{ExecutionGraph, StageNode};
pub use parser::{load_stage_definition_from_plan, parse_plan, parse_plan_content, ParsedPlan};
pub use schema::{validate, LoomConfig, LoomMetadata, StageDefinition, ValidationError};
