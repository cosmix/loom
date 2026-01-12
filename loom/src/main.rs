use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use loom::checkpoints::CheckpointStatus;
use loom::commands::{
    attach, checkpoint, clean, diagnose, fact, graph, init, knowledge, learn, memory, merge,
    resume, run, self_update, sessions, stage, status, stop, verify, worktree_cmd,
};
use loom::completions::{complete_dynamic, generate_completions, CompletionContext, Shell};
use loom::validation::{clap_description_validator, clap_id_validator};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "loom")]
#[command(about = "Self-propelling agent orchestration CLI", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize .work/ directory from a plan
    Init {
        /// Path to the plan file
        plan_path: String,

        /// Clean up stale resources before initialization
        /// (removes old .work/, prunes worktrees, kills orphaned tmux sessions)
        #[arg(long)]
        clean: bool,
    },

    /// Run stages from a plan (starts orchestrator in background)
    Run {
        /// Specific stage ID to run
        #[arg(short, long, value_parser = clap_id_validator)]
        stage: Option<String>,

        /// Enable manual approval for each stage
        #[arg(short, long)]
        manual: bool,

        /// Maximum number of parallel sessions (default: 4)
        #[arg(short = 'p', long)]
        max_parallel: Option<usize>,

        /// Attach to existing orchestrator session
        #[arg(short, long)]
        attach: bool,

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
    Status,

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

    /// Attach to running sessions
    Attach {
        #[command(subcommand)]
        command: Option<AttachCommands>,

        /// Stage ID or session ID (for direct attach without subcommand)
        #[arg(value_parser = clap_id_validator)]
        target: Option<String>,
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

    /// Manage individual stages
    Stage {
        #[command(subcommand)]
        command: StageCommands,
    },

    /// Manage shared facts across stages
    Fact {
        #[command(subcommand)]
        command: FactCommands,
    },

    /// Manage curated codebase knowledge
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
    },

    /// Record and list learnings (mistakes, patterns, conventions)
    Learn {
        #[command(subcommand)]
        command: LearnCommands,
    },

    /// Manage session memory journal (notes, decisions, questions)
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },

    /// Verify integrity of loom resources
    Verify {
        #[command(subcommand)]
        command: VerifyCommands,
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

        /// Kill only loom tmux sessions
        #[arg(long)]
        sessions: bool,

        /// Remove only .work/ state directory
        #[arg(long)]
        state: bool,
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
enum SessionsCommands {
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
enum WorktreeCommands {
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
enum GraphCommands {
    /// Show the execution graph
    Show,

    /// Edit the execution graph
    Edit,
}

#[derive(Subcommand)]
enum StageCommands {
    /// Mark a stage as complete (runs acceptance criteria by default)
    Complete {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Session ID to also mark as completed
        #[arg(long, value_parser = clap_id_validator)]
        session: Option<String>,

        /// Skip acceptance criteria verification
        #[arg(long)]
        no_verify: bool,
    },

    /// Block a stage with a reason
    Block {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Reason for blocking (max 500 characters)
        #[arg(value_parser = clap_description_validator)]
        reason: String,
    },

    /// Reset a stage to ready state, optionally cleaning up session and worktree
    Reset {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Also reset worktree to clean state (git reset --hard)
        #[arg(long)]
        hard: bool,

        /// Kill associated session if running
        #[arg(long)]
        kill_session: bool,
    },

    /// Mark a stage as waiting for user input (used by hooks)
    Waiting {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Resume a stage from waiting state (used by hooks)
    Resume {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Hold a stage (prevent auto-execution even when ready)
    Hold {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Release a held stage (allow auto-execution)
    Release {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Skip a stage (dependents will remain blocked)
    Skip {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Reason for skipping (max 500 characters)
        #[arg(short, long, value_parser = clap_description_validator)]
        reason: Option<String>,
    },

    /// Retry a blocked stage
    Retry {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Ignore retry limit and reset retry count
        #[arg(long)]
        force: bool,
    },

    /// Manually trigger recovery for a crashed or hung stage
    ///
    /// Creates a recovery signal with context from the last session
    /// and resets the stage to Queued status for a new session.
    Recover {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Force recovery even if stage is not in a failed state
        #[arg(short, long)]
        force: bool,
    },

    /// Complete merge conflict resolution and mark stage as completed
    ///
    /// Use this after resolving merge conflicts for a stage in MergeConflict status.
    /// Verifies there are no remaining unmerged files and the merge is complete,
    /// then transitions the stage to Completed with merged=true.
    MergeComplete {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Manage stage outputs (structured values passed to dependent stages)
    Output {
        #[command(subcommand)]
        command: OutputCommands,
    },
}

#[derive(Subcommand)]
enum OutputCommands {
    /// Set an output value for a stage
    Set {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Output key (alphanumeric, dash, underscore only; max 64 characters)
        #[arg(value_parser = clap_id_validator)]
        key: String,

        /// Output value (JSON or plain string)
        value: String,

        /// Description of the output
        #[arg(short, long, value_parser = clap_description_validator)]
        description: Option<String>,
    },

    /// Get a specific output value
    Get {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Output key to retrieve
        #[arg(value_parser = clap_id_validator)]
        key: String,
    },

    /// List all outputs for a stage
    List {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Remove an output from a stage
    Remove {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Output key to remove
        #[arg(value_parser = clap_id_validator)]
        key: String,
    },
}

#[derive(Subcommand)]
enum FactCommands {
    /// Set a fact (key-value pair shared across stages)
    Set {
        /// Fact key (alphanumeric, dash, underscore only; max 64 characters)
        key: String,

        /// Fact value (max 500 characters)
        value: String,

        /// Stage ID that owns this fact (auto-detected from worktree if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        stage: Option<String>,

        /// Confidence level: low, medium (default), high
        #[arg(short, long)]
        confidence: Option<String>,
    },

    /// Get a fact by key
    Get {
        /// Fact key to retrieve
        key: String,
    },

    /// List facts (optionally filtered by stage)
    List {
        /// Filter by stage ID
        #[arg(short, long, value_parser = clap_id_validator)]
        stage: Option<String>,
    },
}

#[derive(Subcommand)]
enum CheckpointCommands {
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

#[derive(Subcommand)]
enum KnowledgeCommands {
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
}

#[derive(Subcommand)]
enum LearnCommands {
    /// Record a mistake and optional correction
    Mistake {
        /// Description of the mistake
        description: String,

        /// Correction or fix for the mistake
        #[arg(short, long)]
        correction: Option<String>,
    },

    /// Record an architectural pattern discovered
    Pattern {
        /// Description of the pattern
        description: String,
    },

    /// Record a coding convention learned
    Convention {
        /// Description of the convention
        description: String,
    },

    /// Record human guidance (requires --human flag)
    Guidance {
        /// Description of the guidance
        description: String,

        /// Confirm this is from a human operator (required)
        #[arg(long)]
        human: bool,

        /// Source of the guidance (e.g., "code review", "slack")
        #[arg(short, long)]
        source: Option<String>,
    },

    /// List recorded learnings
    List {
        /// Filter by category (mistake, pattern, convention, guidance)
        #[arg(short, long)]
        category: Option<String>,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
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
}

#[derive(Subcommand)]
enum VerifyCommands {
    /// Verify learning files haven't been corrupted
    Learnings {
        /// Session ID to verify against (auto-detected from worktree if not provided)
        #[arg(short, long, value_parser = clap_id_validator)]
        session: Option<String>,
    },
}

#[derive(Subcommand)]
enum AttachCommands {
    /// Attach to all running sessions in a unified tmux view
    All {
        /// Open separate GUI terminal windows instead of tmux session
        #[arg(long)]
        gui: bool,

        /// Detach other clients from sessions before attaching
        #[arg(long, short)]
        detach: bool,

        /// Use legacy window-per-session mode instead of tiled panes
        #[arg(long)]
        windows: bool,

        /// Layout for tiled view: tiled (default), horizontal, vertical
        #[arg(long, value_name = "LAYOUT", default_value = "tiled")]
        layout: String,
    },

    /// List all attachable sessions
    List,

    /// Stream daemon logs in real-time
    Logs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { plan_path, clean } => init::execute(Some(PathBuf::from(plan_path)), clean),
        Commands::Run {
            stage,
            manual,
            max_parallel,
            attach,
            foreground,
            watch,
            no_merge,
        } => {
            let auto_merge = !no_merge;
            if attach {
                attach::execute_logs()
            } else if foreground {
                run::execute(stage, manual, max_parallel, watch, auto_merge)
            } else {
                run::execute_background(stage, manual, max_parallel, watch, auto_merge)
            }
        }
        Commands::Status => status::execute(),
        Commands::Resume { stage_id } => resume::execute(stage_id),
        Commands::Merge { stage_id, force } => merge::execute(stage_id, force),
        Commands::Attach { command, target } => match (command, target) {
            (
                Some(AttachCommands::All {
                    gui,
                    detach,
                    windows,
                    layout,
                }),
                _,
            ) => attach::execute_all(gui, detach, windows, layout),
            (Some(AttachCommands::List), _) => attach::list(),
            (Some(AttachCommands::Logs), _) => attach::execute_logs(),
            (None, Some(target)) => attach::execute(target),
            (None, None) => attach::list(),
        },
        Commands::Sessions { command } => match command {
            SessionsCommands::List => sessions::list(),
            SessionsCommands::Kill { session_ids, stage } => sessions::kill(session_ids, stage),
        },
        Commands::Worktree { command } => match command {
            WorktreeCommands::List => worktree_cmd::list(),
            WorktreeCommands::Clean => worktree_cmd::clean(),
            WorktreeCommands::Remove { stage_id } => worktree_cmd::remove(stage_id),
        },
        Commands::Graph { command } => match command {
            GraphCommands::Show => graph::show(),
            GraphCommands::Edit => graph::edit(),
        },
        Commands::Stage { command } => match command {
            StageCommands::Complete {
                stage_id,
                session,
                no_verify,
            } => stage::complete(stage_id, session, no_verify),
            StageCommands::Block { stage_id, reason } => stage::block(stage_id, reason),
            StageCommands::Reset {
                stage_id,
                hard,
                kill_session,
            } => stage::reset(stage_id, hard, kill_session),
            StageCommands::Waiting { stage_id } => stage::waiting(stage_id),
            StageCommands::Resume { stage_id } => stage::resume_from_waiting(stage_id),
            StageCommands::Hold { stage_id } => stage::hold(stage_id),
            StageCommands::Release { stage_id } => stage::release(stage_id),
            StageCommands::Skip { stage_id, reason } => stage::skip(stage_id, reason),
            StageCommands::Retry { stage_id, force } => stage::retry(stage_id, force),
            StageCommands::Recover { stage_id, force } => stage::recover(stage_id, force),
            StageCommands::MergeComplete { stage_id } => stage::merge_complete(stage_id),
            StageCommands::Output { command } => match command {
                OutputCommands::Set {
                    stage_id,
                    key,
                    value,
                    description,
                } => stage::output_set(stage_id, key, value, description),
                OutputCommands::Get { stage_id, key } => stage::output_get(stage_id, key),
                OutputCommands::List { stage_id } => stage::output_list(stage_id),
                OutputCommands::Remove { stage_id, key } => stage::output_remove(stage_id, key),
            },
        },
        Commands::Fact { command } => match command {
            FactCommands::Set {
                key,
                value,
                stage,
                confidence,
            } => fact::set(key, value, stage, confidence),
            FactCommands::Get { key } => fact::get(key),
            FactCommands::List { stage } => fact::list(stage),
        },
        Commands::Knowledge { command } => match command {
            KnowledgeCommands::Show { file } => knowledge::show(file),
            KnowledgeCommands::Update { file, content } => knowledge::update(file, content),
            KnowledgeCommands::Init => knowledge::init(),
            KnowledgeCommands::List => knowledge::list(),
        },
        Commands::Learn { command } => match command {
            LearnCommands::Mistake {
                description,
                correction,
            } => learn::mistake(description, correction),
            LearnCommands::Pattern { description } => learn::pattern(description),
            LearnCommands::Convention { description } => learn::convention(description),
            LearnCommands::Guidance {
                description,
                human,
                source,
            } => learn::guidance(description, human, source),
            LearnCommands::List { category } => learn::list(category),
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Note { text, session } => memory::note(text, session),
            MemoryCommands::Decision {
                text,
                context,
                session,
            } => memory::decision(text, context, session),
            MemoryCommands::Question { text, session } => memory::question(text, session),
            MemoryCommands::Query { search, session } => memory::query(search, session),
            MemoryCommands::List {
                session,
                entry_type,
            } => memory::list(session, entry_type),
            MemoryCommands::Show { session } => memory::show(session),
            MemoryCommands::Sessions => memory::sessions(),
        },
        Commands::Verify { command } => match command {
            VerifyCommands::Learnings { session } => verify::learnings(session),
        },
        Commands::SelfUpdate => self_update::execute(),
        Commands::Clean {
            all,
            worktrees,
            sessions,
            state,
        } => clean::execute(all, worktrees, sessions, state),
        Commands::Stop => stop::execute(),
        Commands::Checkpoint { command } => match command {
            CheckpointCommands::Create {
                task_id,
                status,
                force,
                outputs,
                notes,
            } => {
                let status = status.parse::<CheckpointStatus>()?;
                checkpoint::execute(task_id, status, force, outputs, notes)
            }
            CheckpointCommands::List { session } => checkpoint::list(session),
        },
        Commands::Diagnose { stage_id } => diagnose::execute(&stage_id),
        Commands::Completions { shell } => {
            let shell = Shell::from_str(&shell)?;
            let mut cmd = Cli::command();
            generate_completions(&mut cmd, shell);
            Ok(())
        }
        Commands::Complete { shell, args } => {
            let ctx = CompletionContext::from_args(&shell, &args);
            complete_dynamic(&ctx)
        }
    }
}
