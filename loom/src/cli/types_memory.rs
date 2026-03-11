//! Memory and knowledge CLI command types

use clap::Subcommand;
use loom::fs::knowledge::{DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES};
use loom::validation::{clap_id_validator, clap_knowledge_content_validator};

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

    /// Initialize the knowledge directory
    Init,

    /// List all knowledge files
    List,

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
    Gc {
        /// Max lines per file before GC is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_FILE_LINES)]
        max_file_lines: usize,

        /// Max total lines before GC is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_TOTAL_LINES)]
        max_total_lines: usize,

        /// Only show metrics, skip compaction instructions
        #[arg(short, long)]
        quiet: bool,
    },

    /// Run interactive Claude session to explore and populate knowledge files
    Bootstrap {
        /// Model to use for the Claude session (e.g., "sonnet", "opus")
        #[arg(long)]
        model: Option<String>,

        /// Skip running codebase map before bootstrapping
        #[arg(long)]
        skip_map: bool,

        /// Run in non-interactive mode (pass -p flag to Claude)
        #[arg(short, long)]
        quick: bool,
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

    /// List memory entries from a stage
    List {
        /// Stage ID (auto-detected if not provided)
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
