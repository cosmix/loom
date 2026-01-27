use clap::{Parser, Subcommand};
use loom::validation::clap_id_validator;

pub use super::types_memory::{KnowledgeCommands, MemoryCommands};
pub use super::types_stage::{OutputCommands, StageCommands};

const HELP_TEMPLATE: &str = "
   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴

{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}";

#[derive(Parser)]
#[command(name = "loom")]
#[command(about = "Agent orchestration CLI", long_about = None)]
#[command(version)]
#[command(help_template = HELP_TEMPLATE)]
#[command(subcommand_help_heading = "Commands")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize .work/ directory from a plan
    Init {
        /// Path to the plan file
        plan_path: String,

        /// Clean up stale resources before initialization
        /// (removes old .work/, prunes worktrees, kills orphaned sessions)
        #[arg(long)]
        clean: bool,
    },

    /// Run stages from a plan (starts orchestrator in background)
    Run {
        /// Enable manual approval for each stage
        #[arg(short, long)]
        manual: bool,

        /// Maximum number of parallel sessions (default: 4)
        #[arg(short = 'p', long)]
        max_parallel: Option<usize>,

        /// Run orchestrator in foreground (not recommended)
        #[arg(long)]
        foreground: bool,

        /// Watch mode: continuously spawn ready stages until all are terminal
        #[arg(short, long)]
        watch: bool,

        /// Disable auto-merge of completed stages (merge is enabled by default)
        #[arg(long)]
        no_merge: bool,
    },

    /// Show dashboard with context health
    Status {
        /// Live mode: subscribe to daemon for real-time updates
        #[arg(short, long)]
        live: bool,

        /// Compact mode: single-line output for scripting
        #[arg(short, long)]
        compact: bool,

        /// Verbose mode: show detailed failure information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Resume work on a stage
    Resume {
        /// Stage ID to resume (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Merge or recover a stage (restart conflict resolution if interrupted)
    ///
    /// Primary use: recovery from failed/interrupted merge sessions.
    /// When a merge conflict occurs, loom spawns a Claude Code session to resolve it.
    /// If that session terminates before completion, use this command to restart it.
    Merge {
        /// Stage ID to merge (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Force merge even if stage is not Completed/Verified or has active sessions
        #[arg(short, long)]
        force: bool,
    },

    /// Manage active sessions
    Sessions {
        #[command(subcommand)]
        command: SessionsCommands,
    },

    /// Manage git worktrees
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommands,
    },

    /// Manage the execution graph
    Graph {
        #[command(subcommand)]
        command: GraphCommands,
    },

    /// Manage loom hooks (install/configure without a plan)
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },

    /// Manage individual stages
    Stage {
        #[command(subcommand)]
        command: StageCommands,
    },

    /// Manage curated codebase knowledge
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
    },

    /// Manage session memory journal (notes, decisions, questions)
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },

    /// Manage sandbox configuration
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommands,
    },

    /// Update loom and configuration files
    SelfUpdate,

    /// Clean up loom resources (worktrees, sessions, state)
    Clean {
        /// Remove all loom resources
        #[arg(long)]
        all: bool,

        /// Remove only worktrees and their branches
        #[arg(long)]
        worktrees: bool,

        /// Kill only loom sessions
        #[arg(long)]
        sessions: bool,

        /// Remove only .work/ state directory
        #[arg(long)]
        state: bool,
    },

    /// Repair loom workspace issues (corrupted .work, missing hooks, etc.)
    ///
    /// By default runs in dry-run mode (reports issues without fixing).
    /// Use --fix to apply repairs.
    Repair {
        /// Apply fixes (default is dry-run)
        #[arg(long)]
        fix: bool,
    },

    /// Map codebase structure to knowledge files
    Map {
        /// Deep analysis (more thorough, slower)
        #[arg(short, long)]
        deep: bool,

        /// Focus on specific area (e.g., "auth", "api", "db")
        #[arg(short, long)]
        focus: Option<String>,

        /// Overwrite existing knowledge (default: append)
        #[arg(long)]
        overwrite: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Signal task completion with a checkpoint
    Checkpoint {
        #[command(subcommand)]
        command: CheckpointCommands,
    },

    /// Diagnose a failed stage with Claude Code
    Diagnose {
        /// Stage ID to diagnose (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Run goal-backward verification for a stage
    ///
    /// Validates OUTCOMES beyond acceptance criteria:
    /// - TRUTHS: Observable behaviors that must work
    /// - ARTIFACTS: Files that exist with real implementation
    /// - WIRING: Critical connections between components
    Verify {
        /// Stage ID to verify (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Generate fix suggestions for gaps
        #[arg(long)]
        suggest: bool,
    },

    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for (bash, zsh, fish)
        shell: String,
    },

    /// Internal: Dynamic completion helper (invoked by shell)
    #[command(hide = true)]
    Complete {
        /// Shell type
        shell: String,
        /// Command line arguments being completed
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum SandboxCommands {
    /// Suggest sandbox network domains based on project dependencies
    Suggest,
}

#[derive(Subcommand)]
pub enum SessionsCommands {
    /// List all active sessions
    List,

    /// Kill one or more sessions
    Kill {
        /// Session IDs to kill (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(num_args = 1.., required_unless_present = "stage", value_parser = clap_id_validator)]
        session_ids: Vec<String>,

        /// Kill all sessions for a stage
        #[arg(long, conflicts_with = "session_ids", value_parser = clap_id_validator)]
        stage: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum WorktreeCommands {
    /// List all worktrees
    List,

    /// Clean up unused worktrees
    Clean,

    /// Remove a specific worktree and branch after merge conflict resolution
    ///
    /// Use this command after resolving merge conflicts (manually or via Claude Code).
    /// It cleans up the worktree and branch WITHOUT attempting another merge.
    Remove {
        /// Stage ID to clean up (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },
}

#[derive(Subcommand)]
pub enum GraphCommands {
    /// Show the execution graph
    Show,

    /// Edit the execution graph
    Edit,
}

#[derive(Subcommand)]
pub enum HooksCommands {
    /// Install loom hooks to the current project
    ///
    /// Installs hook scripts to ~/.claude/hooks/loom/ and configures
    /// .claude/settings.local.json with permissions and hooks.
    ///
    /// This allows using loom hooks (like prefer-modern-tools and commit-guard)
    /// in any Claude Code session without running `loom init` with a plan.
    Install,

    /// List available loom hooks and their status
    List,
}

#[derive(Subcommand)]
pub enum CheckpointCommands {
    /// Create a checkpoint to signal task completion
    Create {
        /// Task ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        task_id: String,

        /// Status of the task (completed, blocked, needs_help)
        #[arg(short, long, default_value = "completed")]
        status: String,

        /// Force checkpoint even if verification fails or checkpoint exists
        #[arg(short, long)]
        force: bool,

        /// Output key=value pairs (can be repeated)
        #[arg(short, long = "output", value_name = "KEY=VALUE")]
        outputs: Vec<String>,

        /// Optional notes about the task
        #[arg(short, long)]
        notes: Option<String>,
    },

    /// List checkpoints for the current session
    List {
        /// Session ID (defaults to current session)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },
}
