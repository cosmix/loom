use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use loom::commands::{
    attach, clean, graph, init, merge, resume, run, self_update, sessions, stage, status, stop,
    verify, worktree_cmd,
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

        /// Auto-merge completed stages to target branch
        #[arg(long)]
        auto_merge: bool,
    },

    /// Show dashboard with context health
    Status,

    /// Verify a stage is complete
    Verify {
        /// Stage ID to verify (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Skip acceptance criteria checks and go straight to approval
        #[arg(long, short)]
        force: bool,
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

    /// Kill a specific session
    Kill {
        /// Session ID to kill (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        session_id: String,
    },
}

#[derive(Subcommand)]
enum WorktreeCommands {
    /// List all worktrees
    List,

    /// Clean up unused worktrees
    Clean,
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
            auto_merge,
        } => {
            if attach {
                attach::execute_logs()
            } else if foreground {
                run::execute(stage, manual, max_parallel, watch, auto_merge)
            } else {
                run::execute_background(stage, manual, max_parallel, watch, auto_merge)
            }
        }
        Commands::Status => status::execute(),
        Commands::Verify { stage_id, force } => verify::execute(stage_id, force),
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
            SessionsCommands::Kill { session_id } => sessions::kill(session_id),
        },
        Commands::Worktree { command } => match command {
            WorktreeCommands::List => worktree_cmd::list(),
            WorktreeCommands::Clean => worktree_cmd::clean(),
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
        },
        Commands::SelfUpdate => self_update::execute(),
        Commands::Clean {
            all,
            worktrees,
            sessions,
            state,
        } => clean::execute(all, worktrees, sessions, state),
        Commands::Stop => stop::execute(),
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
