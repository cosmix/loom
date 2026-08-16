//! Types for the derived source graph — the second retrieval channel.
//!
//! The source-graph stage populates these; `crate::context` only ever reads
//! them. They live here rather than in [`crate::context::schema`] because the
//! graph is a distinct domain from the knowledge corpus, and the stage that
//! builds it will grow this module rather than the shared contract.
//!
//! [`crate::context::schema`] re-exports all three names, so callers may reach
//! them through either path.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One node of the derived source graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceNode {
    /// Stable id, `<relative-path>#<symbol>` for a symbol node and the bare path
    /// for a file node.
    pub id: String,
    /// Path relative to the project root.
    pub path: PathBuf,
    /// Symbol name, absent for a whole-file node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

/// The relationship a [`SourceEdge`] encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceEdgeKind {
    Calls,
    Imports,
    Defines,
    References,
}

/// One directed edge of the derived source graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEdge {
    /// [`SourceNode::id`] of the origin.
    pub from: String,
    /// [`SourceNode::id`] of the target.
    pub to: String,
    pub kind: SourceEdgeKind,
}
