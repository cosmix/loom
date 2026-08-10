//! Stage-related CLI command types

use crate::validation::{clap_description_validator, clap_id_validator};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum StageCommands {
    /// Mark a stage as complete (runs acceptance criteria by default).
    /// Privileged flags authorize themselves from the daemon token, which
    /// only an operator shell can read.
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

        /// UNSAFE: Force completion from any state, bypassing state machine validation.
        /// WARNING: This can corrupt dependency tracking. Use only for recovery.
        #[arg(long = "force-unsafe")]
        force_unsafe: bool,

        /// When using --force-unsafe, also mark stage as merged (assumes manual merge was done).
        /// Without this, dependent stages will NOT be triggered.
        #[arg(long = "assume-merged", requires = "force_unsafe")]
        assume_merged: bool,
    },

    /// Mint a one-time proof for a trusted broker to hand to another process.
    /// Operators do not need this: privileged commands authorize themselves.
    /// Supply the daemon token through LOOM_ADMIN_TOKEN; only the proof is printed.
    AdminProof {
        /// Stage ID the proof authorizes
        #[arg(required_unless_present = "daemon_stop", value_parser = clap_id_validator)]
        stage_id: Option<String>,

        /// Authorize one daemon shutdown instead of a stage completion
        #[arg(
            long = "daemon-stop",
            conflicts_with_all = ["stage_id", "no_verify", "force_unsafe", "assume_merged"]
        )]
        daemon_stop: bool,

        /// Authorize --no-verify for this stage completion
        #[arg(long, conflicts_with = "daemon_stop")]
        no_verify: bool,

        /// Authorize --force-unsafe for this stage completion
        #[arg(long = "force-unsafe", conflicts_with = "daemon_stop")]
        force_unsafe: bool,

        /// Authorize --assume-merged together with --force-unsafe
        #[arg(
            long = "assume-merged",
            requires = "force_unsafe",
            conflicts_with = "daemon_stop"
        )]
        assume_merged: bool,
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

        /// Also clear the stage's session assignment (does NOT run git reset --hard in the
        /// worktree — clean the worktree manually if needed)
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

    /// Retry a failed, crashed, or hung stage
    ///
    /// Generates a recovery signal with context when the stage was crashed or
    /// hung, or when --context is provided. Replaces the old `recover` command.
    Retry {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Ignore retry limit and reset retry count
        #[arg(long)]
        force: bool,

        /// Recovery context message (triggers recovery signal generation)
        #[arg(long)]
        context: Option<String>,
    },

    /// Merge a stage's worktree branch into main
    ///
    /// Re-attempts the merge for a stage in MergeConflict or MergeBlocked status.
    /// Must be run from within the stage worktree.
    /// Use --resolved after manually resolving conflicts to mark the merge complete.
    Merge {
        /// Stage ID (auto-detected from branch if omitted)
        #[arg(value_parser = clap_id_validator)]
        stage_id: Option<String>,

        /// Mark manually resolved merge conflicts as complete
        /// (validates clean git state before completing)
        #[arg(long)]
        resolved: bool,
    },

    /// Respond to a stage flagged for human review
    ///
    /// Use this to approve, force-complete, or reject a stage in NeedsHumanReview state.
    /// Without flags, shows the current review reason and available actions.
    HumanReview {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Approve: resume execution with fresh fix attempts
        #[arg(long, group = "action")]
        approve: bool,

        /// Force-complete: skip acceptance criteria and mark as completed
        #[arg(long, group = "action")]
        force_complete: bool,

        /// Reject: block the stage with the given reason (max 500 characters)
        #[arg(long, group = "action", value_parser = clap_description_validator)]
        reject: Option<String>,
    },

    /// Dispute an acceptance criterion. Files a structured dispute via the
    /// daemon RPC; the daemon writes request.md and transitions the stage
    /// to NeedsAdjudication. Verdict.md is written by the adjudicator
    /// (daemon-only), never by the agent.
    DisputeCriteria {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Index (0-based) of the acceptance criterion being disputed.
        #[arg(long = "criterion-index")]
        criterion_index: usize,

        /// Reason the criterion is wrong or impossible (max 500 chars).
        #[arg(long, value_parser = clap_description_validator)]
        reason: String,

        /// Optional commit SHA cited as evidence.
        #[arg(long = "evidence-commit")]
        evidence_commit: Option<String>,

        /// Optional path to a captured failure-output file (truncated to 4KB).
        #[arg(long = "failure-output")]
        failure_output: Option<std::path::PathBuf>,
    },

    /// Manage stage outputs (structured values passed to dependent stages)
    Output {
        #[command(subcommand)]
        command: OutputCommands,
    },
}

#[derive(Subcommand)]
pub enum OutputCommands {
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
