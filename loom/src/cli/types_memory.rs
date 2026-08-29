//! Memory and knowledge CLI command types

use crate::validation::{clap_id_validator, clap_knowledge_content_validator};
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum KnowledgeCommands {
    /// Update (append to) a knowledge file
    Update {
        /// Tier-1 file (entry-points, patterns, conventions, ...) or tier-2 topic (<category>/<slug>)
        file: String,

        /// Content to append (markdown format). Omit or use "-" to read from stdin.
        #[arg(value_parser = clap_knowledge_content_validator)]
        content: Option<String>,
    },

    /// Replace a `#{2,6} <heading>` section in place, at whatever level it's found (corrects stale knowledge; appends if absent)
    ReplaceSection {
        /// Tier-1 file (entry-points, patterns, conventions, ...) or tier-2 topic (<category>/<slug>)
        file: String,

        /// Heading of the section to overwrite, with or without the leading `## `
        heading: String,

        /// Replacement body WITHOUT the heading line. Omit or use "-" to read from stdin.
        #[arg(value_parser = clap_knowledge_content_validator)]
        content: Option<String>,
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

    /// Score retrieval against a checked-in case file (precision@5 / MRR)
    Eval {
        /// Cases file; defaults to loom/eval/retrieval-cases.yaml under the project root
        #[arg(long)]
        cases: Option<PathBuf>,
        /// Override the per-case token budget
        #[arg(long)]
        budget_tokens: Option<usize>,
        /// Machine-readable JSON output (suppresses human text)
        #[arg(long)]
        json: bool,
    },

    /// Rebuild derived context artifacts, and upgrade a flat knowledge dir (creates INDEX.md)
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
