//! Memory and knowledge CLI command types

use clap::Subcommand;
use loom::fs::knowledge::{DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES};
use loom::validation::{clap_description_validator, clap_id_validator};

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

        /// Content to append (markdown format)
        #[arg(value_parser = clap_description_validator)]
        content: String,
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

    /// Analyze knowledge files for size, duplicates, and promoted blocks
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
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Record a note in the session memory
    Note {
        /// The note text
        text: String,

        /// Session ID (auto-detected from worktree if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },

    /// Record a decision with optional rationale
    Decision {
        /// The decision text
        text: String,

        /// Context or rationale for the decision
        #[arg(short, long)]
        context: Option<String>,

        /// Session ID (auto-detected from worktree if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },

    /// Record an open question
    Question {
        /// The question text
        text: String,

        /// Session ID (auto-detected from worktree if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },

    /// Search memory entries
    Query {
        /// Search term
        search: String,

        /// Session ID to search (searches all if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },

    /// List memory entries from a session
    List {
        /// Session ID (auto-detected if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,

        /// Filter by entry type (note, decision, question)
        #[arg(short = 't', long)]
        entry_type: Option<String>,
    },

    /// Show full memory journal
    Show {
        /// Session ID (auto-detected if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },

    /// List all memory journals
    Sessions,

    /// Promote memory entries to knowledge files
    Promote {
        /// Entry type to promote: note, decision, question, or all
        entry_type: String,

        /// Target knowledge file: entry-points, patterns, conventions, mistakes
        target: String,

        /// Session ID (auto-detected if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },
}
