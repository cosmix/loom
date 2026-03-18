//! Stage-related CLI command types

use clap::Subcommand;
use loom::validation::{clap_description_validator, clap_id_validator};

#[derive(Subcommand)]
pub enum StageCommands {
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

        /// UNSAFE: Force completion from any state, bypassing state machine validation.
        /// WARNING: This can corrupt dependency tracking. Use only for recovery.
        #[arg(long = "force-unsafe")]
        force_unsafe: bool,

        /// When using --force-unsafe, also mark stage as merged (assumes manual merge was done).
        /// Without this, dependent stages will NOT be triggered.
        #[arg(long = "assume-merged", requires = "force_unsafe")]
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

    /// Re-run acceptance criteria and complete a stage that previously failed
    ///
    /// Use this to re-verify a stage in CompletedWithFailures or Executing state.
    /// Reloads acceptance criteria from the plan file (unless --no-reload),
    /// runs them, and if they pass, completes the stage with merge.
    ///
    /// Use --dry-run to check criteria without changing stage status.
    Verify {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Skip reloading acceptance criteria from plan file
        #[arg(long)]
        no_reload: bool,

        /// Check criteria without changing stage status (shows detailed results)
        #[arg(long)]
        dry_run: bool,
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

    /// Dispute acceptance criteria and request human review
    ///
    /// Use this when acceptance criteria are incorrect or inappropriate.
    /// Transitions the stage to NeedsHumanReview so a human can decide
    /// whether to approve, force-complete, or reject.
    DisputeCriteria {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Reason why the acceptance criteria are wrong (max 500 characters)
        #[arg(value_parser = clap_description_validator)]
        reason: String,
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
