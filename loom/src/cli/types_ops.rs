//! Operational CLI command types: plans, sessions, worktrees, and the
//! deterministic context/hook entry points.
//!
//! Split out of `types.rs` for the same reason as `types_memory.rs` and
//! `types_stage.rs`: the top-level `Commands` enum is large enough on its own,
//! and every new subcommand family that lands beside it costs the file lines it
//! does not have. `types.rs` re-exports everything here, so callers keep naming
//! a single `cli::types` surface.

use std::net::IpAddr;

use super::types::parse_web_host;
use crate::validation::clap_id_validator;
use clap::{Args, Subcommand};

/// Bare `loom plan`: serve the planning app for the project in the current directory.
#[derive(Args, Debug, Clone)]
pub struct PlanServeArgs {
    /// Port to bind (default: 7272, or the next free port after it; 0 picks a free port)
    #[arg(long)]
    pub port: Option<u16>,

    /// Address to bind: an IPv4/IPv6 literal or `localhost`. Binding off loopback serves
    /// plain HTTP and prints a warning
    #[arg(long, value_parser = parse_web_host, default_value = "127.0.0.1")]
    pub host: IpAddr,

    /// Print the URL without opening a browser
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Subcommand)]
pub enum PlanCommands {
    /// Verify a plan file without side effects (no .loom/work/, no git repo required)
    Verify {
        /// Path to the plan file to validate
        path: std::path::PathBuf,

        /// Promote warnings to errors
        #[arg(long)]
        strict: bool,

        /// Machine-readable JSON output (suppresses human text)
        #[arg(long)]
        json: bool,

        /// Disable ANSI color codes
        #[arg(long)]
        no_color: bool,
    },

    /// Print the project checklist and every document convention problem under doc/
    Check {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },

    /// Seal, brief, report and release plan and spec summaries (no server needed)
    Summary {
        #[command(subcommand)]
        command: SummaryCommands,
    },
}

/// `loom plan summary <subcommand>`.
#[derive(Subcommand)]
pub enum SummaryCommands {
    /// Validate the draft of a summary and write it into the store
    Seal {
        /// Summary id, for example `product/section_01` or `plans/my-plan`
        id: String,

        /// Seal even when another session or a terminal claimed the summary
        #[arg(long)]
        force: bool,
    },

    /// Claim summaries and print the brief for rebuilding them
    Brief {
        /// Summary ids to claim and brief
        #[arg(required = true)]
        ids: Vec<String>,

        /// Note to show beside the sentence it names, or at the top
        #[arg(long)]
        note: Option<String>,
    },

    /// Print the state of every summary
    Status {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },

    /// Give up the claims on summaries
    Release {
        /// Summary ids to release
        #[arg(required = true)]
        ids: Vec<String>,
    },
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

    /// Remove a specific worktree and branch after merge conflict resolution
    ///
    /// Use this command after resolving merge conflicts manually or in a resolver session.
    /// It cleans up the worktree and branch WITHOUT attempting another merge.
    Remove {
        /// Stage ID to clean up (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Allow removal when unmerged work is detected
        #[arg(long)]
        force: bool,

        /// Exact confirmation phrase required with --force
        #[arg(long = "confirm", requires = "force", value_name = "PHRASE")]
        confirmation: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ContextCommands {
    /// Record files an agent edited, so the stage's context overlay stays current
    RecordEdit {
        /// Stage whose overlay the edit belongs to
        #[arg(long)]
        stage: String,

        /// Edited path; repeat for several
        #[arg(long = "path", required = true)]
        paths: Vec<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum HookCommands {
    /// UserPromptSubmit entry point: emit a retrieval brief, or nothing
    UserPrompt,

    /// Internal maintenance entry point invoked by the UserPromptSubmit hook
    /// itself as a fire-and-forget nudge when the source graph looks stale.
    /// Not a user-facing command.
    ReconcileGraph {
        /// Stop a running background reconcile instead of starting one
        #[arg(long)]
        cancel: bool,
    },

    /// Internal maintenance entry point invoked by the PreCompact shell hook
    /// and by knowledge-orient.sh on a SessionStart `compact`, to reopen this
    /// session's own delivery suppression once its context window is
    /// compacted away. Not a user-facing command.
    PreCompact,

    /// Print Rust-resolved main/subagent context ceilings for session hooks.
    /// Not a user-facing command.
    #[command(hide = true)]
    ContextCeilings,

    /// Persist authoritative lifecycle observations for one Codex forward;
    /// reads the forward's PostToolUse payload on stdin.
    #[command(hide = true)]
    ForwardReceipt,

    /// Retrieve a scoped brief for a typed worker.
    #[command(hide = true)]
    WorkerBrief,

    /// Discover package-scoped project types for skill recommendations.
    #[command(hide = true)]
    ProjectTypes,

    /// PostToolUse helper behind `loom-relay.sh`: record the requests a
    /// sandboxed loom command left in this session's scratch directory in the
    /// daemon inbox. Not a user-facing command.
    #[command(hide = true)]
    Relay {
        /// Comma-separated request kinds the Bash command may relay, as the
        /// hook derived them from its loom invocations.
        #[arg(long)]
        allowed_kinds: String,
    },

    /// SubagentStop helper behind `subagent-stop.sh`: record a
    /// `loom-code-reviewer`'s final report as a review round. Not a
    /// user-facing command.
    #[command(hide = true)]
    ReviewHarvest,
}

#[derive(Subcommand)]
pub enum RequestCommands {
    /// Show whether the daemon has applied a request relayed via the CLI
    /// output the relay hook watches for (`loom memory note`, `loom stage
    /// merge --resolved`, ...).
    Status {
        /// The request id printed after the originating command's output.
        id: String,

        /// Session to look the request up under. Defaults to
        /// `LOOM_SESSION_ID`; without either, every session's inbox is
        /// scanned.
        #[arg(long)]
        session: Option<String>,

        /// Wait up to SECS seconds for the request to be applied or refused,
        /// failing on a refusal or when it is still pending at the deadline.
        #[arg(long, value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..=600))]
        wait: Option<u64>,
    },
}
