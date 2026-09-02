use crate::validation::clap_id_validator;
use clap::{Parser, Subcommand};

pub use super::types_config::ConfigArgs;
pub use super::types_memory::{KnowledgeCommands, MemoryCommands};
pub use super::types_ops::{
    ContextCommands, HookCommands, PlanCommands, SessionsCommands, WorktreeCommands,
};
pub use super::types_stage::{OutputCommands, StageCommands};

/// Rendered by `-v`/`--version`: version, commit, build date, target triple.
const VERSION_STRING: &str = concat!(
    env!("LOOM_VERSION"),
    " (",
    env!("LOOM_COMMIT"),
    ", ",
    env!("LOOM_BUILD_DATE"),
    ", ",
    env!("LOOM_TARGET"),
    ")"
);

const HELP_TEMPLATE: &str = "
   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴

{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}";

fn positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("'{value}' is not a valid positive integer"))?;
    if parsed == 0 {
        return Err("value must be at least 1".to_string());
    }
    Ok(parsed)
}

#[derive(Parser)]
#[command(name = "loom")]
#[command(about = "Agent orchestration CLI", long_about = None)]
#[command(version = VERSION_STRING)]
#[command(disable_version_flag = true)]
#[command(help_template = HELP_TEMPLATE)]
#[command(subcommand_help_heading = "Commands")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Print version information
    #[arg(short = 'v', long = "version", short_alias = 'V', action = clap::ArgAction::Version)]
    pub version: Option<bool>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize .loom/work/ directory from a plan
    Init {
        /// Path to the plan file
        plan_path: String,

        /// Clean up stale resources before initialization
        /// (removes old .loom/work/, prunes worktrees, kills orphaned sessions)
        #[arg(long)]
        clean: bool,

        /// Terminal backend for sessions (native|tmux); skips the interactive prompt
        #[arg(long, value_parser = ["native", "tmux"])]
        backend: Option<String>,

        /// Acknowledge and allow a plan that expands the default sandbox policy
        #[arg(long)]
        allow_unsafe_plan: bool,
    },

    /// Run stages from a plan (starts orchestrator in background)
    Run {
        /// Enable manual approval for each stage
        #[arg(short, long)]
        manual: bool,

        /// Maximum number of parallel sessions (default: 4)
        #[arg(short = 'p', long, value_parser = positive_usize)]
        max_parallel: Option<usize>,

        /// Run orchestrator in foreground (not recommended)
        #[arg(long)]
        foreground: bool,

        /// Watch mode: continuously spawn ready stages until all are terminal
        #[arg(short, long, requires = "foreground")]
        watch: bool,

        /// Disable auto-merge of completed stages (merge is enabled by default)
        #[arg(long)]
        no_merge: bool,

        /// Terminal backend for sessions (native|tmux); persisted to the [terminal] section of the loom config
        #[arg(long, value_parser = ["native", "tmux"])]
        backend: Option<String>,
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

    /// Manage active sessions
    Sessions {
        #[command(subcommand)]
        command: SessionsCommands,
    },

    /// Attach to loom sessions (tmux backend only)
    Attach {
        /// Stage id to attach to directly; omit for a tiled overview
        #[arg(value_parser = clap_id_validator)]
        stage_id: Option<String>,
    },

    /// Manage git worktrees
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommands,
    },

    /// Show the execution graph
    Graph,

    /// Manage individual stages
    Stage {
        #[command(subcommand)]
        command: StageCommands,
    },

    /// Create a handoff file capturing current session state
    Handoff {
        /// Stage ID (auto-detected from LOOM_STAGE_ID env var if not provided)
        #[arg(long, value_parser = clap_id_validator)]
        stage: Option<String>,

        /// Session ID (auto-detected from LOOM_SESSION_ID env var if not provided)
        #[arg(long)]
        session: Option<String>,

        /// Trigger type (e.g., precompact, session_end, manual)
        #[arg(long, default_value = "manual")]
        trigger: String,

        /// Optional message to include in the handoff
        #[arg(long)]
        message: Option<String>,
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

    /// Generate code review documents from stage memories
    Review {
        /// Summarize the plan with a headless Claude Haiku call (`claude -p`).
        /// Off by default to avoid headless API charges; without it the plan's
        /// first paragraph is used as the summary.
        #[arg(long)]
        ai_summary: bool,
    },

    /// Update loom and configuration files
    SelfUpdate,

    /// Read or write the user config at ~/.loom/config.toml
    Config(ConfigArgs),

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

        /// Remove only .loom/work/ state directory
        #[arg(long)]
        state: bool,
    },

    /// Repair loom workspace issues (corrupted .loom/work, missing hooks, sandbox settings, etc.)
    ///
    /// By default runs in dry-run mode (reports issues without fixing).
    /// Use --fix to apply repairs.
    Repair {
        /// Apply fixes (default is dry-run)
        #[arg(long)]
        fix: bool,
    },

    /// Query the derived source graph: file outlines, symbol lookup, impact analysis
    Map {
        #[command(flatten)]
        args: crate::commands::map::MapArgs,
    },

    /// Read-only watchdog over Claude Code's per-subagent transcripts:
    /// list liveness state, harvest final reports, or watch until settled
    Subagents {
        #[command(flatten)]
        args: crate::commands::subagents::SubagentsArgs,
    },

    /// Report what agent sessions actually consume, in tokens
    Usage {
        #[command(flatten)]
        args: crate::commands::usage::UsageArgs,
    },

    /// Pressure-test a plan with alternating Claude and Codex review rounds
    Pressure {
        /// Path to the plan file (repo-relative or absolute; a bare filename
        /// resolves under doc/plans/)
        plan: String,

        /// Number of pressure/address rounds to run (must be >= 1)
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..))]
        rounds: u32,

        /// Print the planned steps without spawning Claude or Codex
        #[arg(long)]
        dry_run: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Diagnose a failed stage with Claude Code
    Diagnose {
        /// Stage ID to diagnose (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Manage plan files (validate, inspect)
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
    },

    /// Run goal-backward verification for a stage
    ///
    /// Validates OUTCOMES beyond acceptance criteria:
    /// - TRUTHS: Observable behaviors that must work
    /// - ARTIFACTS: Files that exist with real implementation
    /// - WIRING: Critical connections between components
    Check {
        /// Stage ID to verify (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Generate fix suggestions for gaps
        #[arg(long)]
        suggest: bool,
    },

    /// Build skill keyword index for skill-trigger hook
    SkillIndex,

    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for (bash, zsh, fish).
        /// Auto-detected from $SHELL when using --install without specifying a shell.
        shell: Option<String>,

        /// Install completions to the appropriate system location
        #[arg(long)]
        install: bool,

        /// Check for outdated completions and show migration instructions
        #[arg(long)]
        migrate: bool,
    },

    /// Record and inspect per-stage retrieval context
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },

    /// Deterministic hook entry points invoked by loom's shell hooks
    Hook {
        #[command(subcommand)]
        command: HookCommands,
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn run_rejects_zero_parallelism() {
        let result = Cli::try_parse_from(["loom", "run", "--max-parallel", "0"]);
        assert!(result.is_err());
    }

    #[test]
    fn watch_requires_foreground_mode() {
        let result = Cli::try_parse_from(["loom", "run", "--watch"]);
        assert!(result.is_err());
        assert!(Cli::try_parse_from(["loom", "run", "--foreground", "--watch"]).is_ok());
    }

    #[test]
    fn init_exposes_explicit_unsafe_plan_acknowledgement() {
        let parsed =
            Cli::try_parse_from(["loom", "init", "plan.md", "--allow-unsafe-plan"]).unwrap();

        let Commands::Init {
            allow_unsafe_plan, ..
        } = parsed.command
        else {
            panic!("expected init command");
        };
        assert!(allow_unsafe_plan);
    }

    #[test]
    fn admin_proof_daemon_stop_is_an_exclusive_mint_mode() {
        assert!(Cli::try_parse_from(["loom", "stage", "admin-proof", "--daemon-stop"]).is_ok());
        assert!(
            Cli::try_parse_from(["loom", "stage", "admin-proof", "stage-a", "--daemon-stop"])
                .is_err()
        );
        assert!(Cli::try_parse_from([
            "loom",
            "stage",
            "admin-proof",
            "--daemon-stop",
            "--no-verify"
        ])
        .is_err());
    }

    #[test]
    fn internal_context_ceiling_hook_command_is_hidden() {
        let mut command = Cli::command();
        let hook = command
            .find_subcommand_mut("hook")
            .expect("hook command exists");
        let help = hook.render_long_help().to_string();
        assert!(!help.contains("context-ceilings"), "{help}");
    }
}
