//! Structured handoff schema for validated YAML handoffs.
//!
//! This module defines the V2 handoff format which uses typed YAML fields
//! instead of unstructured prose. This enables machine-readable handoffs
//! that can be validated and parsed reliably.

mod parsing;
mod types;
mod v2;

// Re-export all public types
pub use parsing::ParsedHandoff;
pub use types::{CommitRef, CompletedTask, FileRef, KeyDecision};
pub use v2::{HandoffV2, HANDOFF_SCHEMA_VERSION};
