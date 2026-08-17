//! Memory and knowledge CLI command types

use crate::fs::knowledge::{DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES};
use crate::validation::{clap_id_validator, clap_knowledge_content_validator};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum KnowledgeCommands {
    /// Show knowledge summary or a specific file
    Show {
        /// File to show (entry-points, patterns, conventions)
        #[arg(value_name = "FILE")]
        file: Option<String>,
    },

    /// Update (append to) a knowledge file
    Update {
        /// File to update (entry-points, patterns, conventions)
        file: String,

        /// Content to append (markdown format). Omit or use "-" to read from stdin.
        #[arg(value_parser = clap_knowledge_content_validator)]
        content: Option<String>,
    },

    /// Replace a section in a knowledge file by heading
    ReplaceSection {
        /// File to update (entry-points, patterns, conventions, mistakes, etc.)
        file: String,

        /// Section heading to find and replace (e.g., "Merge Recovery Flow")
        heading: String,

        /// New content for the section. Omit or use "-" to read from stdin.
        #[arg(value_parser = clap_knowledge_content_validator)]
        content: Option<String>,
    },

    /// Initialize the knowledge directory
    Init,

    /// List all knowledge files
    List,

    /// Regenerate the knowledge INDEX.md (creates it on a flat knowledge dir)
    Index,

    /// Check knowledge completeness and src/ coverage
    Check {
        /// Minimum coverage percentage required (default: 50)
        #[arg(long, default_value = "50")]
        min_coverage: u8,

        /// Path to src/ directory to check (default: auto-detect)
        #[arg(long)]
        src_path: Option<String>,

        /// Quiet mode - only output errors
        #[arg(short, long)]
        quiet: bool,
    },

    /// Analyze knowledge files for size, duplicates, and curated blocks
    Audit {
        /// Max lines per tier-1 summary file before compaction is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_TIER1_LINES)]
        max_file_lines: usize,

        /// Max lines per tier-2 topic file before compaction is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_TOPIC_LINES)]
        max_topic_lines: usize,

        /// Only show metrics, skip compaction instructions
        #[arg(short, long)]
        quiet: bool,
    },

    /// Spawn Claude session to compact knowledge files (dedupe, summarize, drop stale)
    Gc {
        /// Model for the Claude session (default: "opus" — GC is judgement-heavy)
        #[arg(long)]
        model: Option<String>,

        /// Preview proposed changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Run in non-interactive mode (no terminal UI)
        #[arg(long)]
        quick: bool,
    },

    /// Spawn Claude session to explore and populate knowledge files
    Bootstrap {
        /// Model for the Claude session (default: "opus" — bootstrap is judgement-heavy)
        #[arg(long)]
        model: Option<String>,

        /// Skip running codebase map before bootstrapping
        #[arg(long)]
        skip_map: bool,

        /// Run in non-interactive mode (no terminal UI)
        #[arg(long)]
        quick: bool,
    },

    /// Retrieve a token-budgeted context pack for a query (deterministic, offline)
    Context {
        /// Seed the query from this stage's dependencies, and name it in output
        #[arg(long)]
        stage: Option<String>,
        /// Query text to retrieve context for
        #[arg(long)]
        query: String,
        /// Maximum estimated tokens the pack may contain
        #[arg(long, default_value_t = 2000)]
        budget_tokens: usize,
        /// Retrieval channels to search: knowledge, source, or all
        #[arg(long, default_value = "all")]
        scope: String,
        /// Chunk id that must be included; repeatable
        #[arg(long = "require-id")]
        require_id: Vec<String>,
        /// Show per-item scores and selection reasons
        #[arg(long)]
        explain: bool,
        /// Machine-readable JSON output (suppresses human text)
        #[arg(long)]
        json: bool,
    },

    /// Show knowledge catalog freshness, size, and reported issues
    Status {
        /// Machine-readable JSON output (suppresses human text)
        #[arg(long)]
        json: bool,
    },

    /// Rebuild derived context artifacts when the knowledge tree has changed
    Sync {
        /// Rebuild only the structural (catalog) layer
        #[arg(long)]
        structural_only: bool,
        /// Machine-readable JSON output (suppresses human text)
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Record a note in the stage memory
    Note {
        /// The note text
        text: String,

        /// Stage ID (auto-detected from LOOM_STAGE_ID if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },

    /// Record a decision with optional rationale
    Decision {
        /// The decision text
        text: String,

        /// Context or rationale for the decision
        #[arg(short, long)]
        context: Option<String>,

        /// Stage ID (auto-detected from LOOM_STAGE_ID if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },

    /// Record an open question
    Question {
        /// The question text
        text: String,

        /// Stage ID (auto-detected from LOOM_STAGE_ID if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },

    /// Record a file change
    Change {
        /// Description of what changed (e.g., "src/foo.rs - Added bar() function")
        text: String,

        /// Stage ID (auto-detected from LOOM_STAGE_ID if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },

    /// Search memory entries
    Query {
        /// Search term
        search: String,

        /// Stage ID to search (searches all if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },

    /// List memory entries (all journals in the plan, or one stage with --stage)
    List {
        /// Stage ID to scope to (lists every journal in the plan if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,

        /// Filter by entry type (note, decision, question)
        #[arg(short = 't', long)]
        entry_type: Option<String>,
    },

    /// Show full memory journal
    Show {
        /// Stage ID (auto-detected if not provided)
        #[arg(short = 'S', long, value_parser = clap_id_validator)]
        stage: Option<String>,

        /// Show ALL stage memories
        #[arg(short, long)]
        all: bool,
    },
}
