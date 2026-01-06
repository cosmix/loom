use anyhow::Result;
use clap::{Parser, Subcommand};
use flux::commands::{
    attach, graph, init, merge, resume, run, self_update, sessions, stage, status, verify,
    worktree_cmd,
};
use flux::validation::{clap_description_validator, clap_id_validator};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "flux")]
#[command(about = "Self-propelling agent orchestration CLI", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize .work/ directory with optional plan
    Init {
        /// Path to the plan file
        plan_path: Option<String>,
    },

    /// Run stages from a plan
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
    },

    /// Show dashboard with context health
    Status,

    /// Verify a stage is complete
    Verify {
        /// Stage ID to verify (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Resume work on a stage
    Resume {
        /// Stage ID to resume (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Merge a completed stage
    Merge {
        /// Stage ID to merge (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Attach to a running stage or session
    Attach {
        /// Stage ID or session ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_or_session_id: String,
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

    /// Update flux and configuration files
    SelfUpdate,
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
    /// Mark a stage as complete
    Complete {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
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

    /// Reset a stage to not-started
    Reset {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { plan_path } => init::execute(plan_path.map(PathBuf::from)),
        Commands::Run {
            stage,
            manual,
            max_parallel,
        } => run::execute(stage, manual, max_parallel),
        Commands::Status => status::execute(),
        Commands::Verify { stage_id } => verify::execute(stage_id),
        Commands::Resume { stage_id } => resume::execute(stage_id),
        Commands::Merge { stage_id } => merge::execute(stage_id),
        Commands::Attach {
            stage_or_session_id,
        } => attach::execute(stage_or_session_id),
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
            StageCommands::Complete { stage_id } => stage::complete(stage_id),
            StageCommands::Block { stage_id, reason } => stage::block(stage_id, reason),
            StageCommands::Reset { stage_id } => stage::reset(stage_id),
        },
        Commands::SelfUpdate => self_update::execute(),
    }
}
