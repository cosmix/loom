//! Knowledge map operations for curated codebase knowledge.
//!
//! Design principle: Claude Code already has Glob, Grep, Read, LSP tools.
//! We curate high-level knowledge that helps agents know WHERE to look,
//! not raw indexing.

pub mod dir;
pub mod gc;
pub mod types;

// Re-export commonly used types
pub use dir::KnowledgeDir;
pub use gc::{
    analyze_gc_metrics, FileGcMetrics, GcMetrics, DEFAULT_MAX_FILE_LINES,
    DEFAULT_MAX_PROMOTED_BLOCKS, DEFAULT_MAX_TOTAL_LINES,
};
pub use types::KnowledgeFile;
