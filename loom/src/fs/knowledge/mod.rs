//! Knowledge map operations for curated codebase knowledge.
//!
//! Design principle: Claude Code already has Glob, Grep, Read, LSP tools.
//! We curate high-level knowledge that helps agents know WHERE to look,
//! not raw indexing.
//!
//! Knowledge is tiered: a generated `INDEX.md` (tier 0) points at the seven
//! curated summary files (tier 1), which link to per-category topic files
//! (tier 2, e.g. `architecture/merge-flow.md`). Directories created before the
//! hierarchy existed stay flat and keep working — see [`types::KnowledgeLayout`].

pub mod dir;
pub mod gc;
pub mod index;
pub mod summary;
pub mod templates;
pub mod types;

// Re-export commonly used types
pub use dir::KnowledgeDir;
pub use gc::{
    analyze_gc_metrics, GcMetrics, Tier1Metrics, TopicMetrics, DEFAULT_MAX_PROMOTED_BLOCKS,
    DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES, SECTION_EXTRACT_THRESHOLD,
};
pub use index::TopicEntry;
pub use types::{KnowledgeFile, KnowledgeLayout, KnowledgeTarget, INDEX_FILENAME};
