//! Stage-related CLI command types

use crate::plan::{AmendmentField, AmendmentPatch};
use crate::validation::{clap_description_validator, clap_id_validator};
use anyhow::{bail, Result};
use clap::{Subcommand, ValueEnum};

/// Which array on a stage `loom stage amend` mutates.
///
/// Mirrors `crate::plan::AmendmentField` one-for-one; kept as a separate,
/// clap-level mirror of `crate::plan::AmendmentField`; [`AmendField::to_field`]
/// maps it across so `cli::dispatch` stays a thin routing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AmendField {
    /// Mutate the `acceptance` array.
    Acceptance,
    /// Mutate the `wiring` array.
    Wiring,
}

/// What to do at `--index` within the field targeted by `loom stage amend`.
///
/// Mirrors `crate::plan::AmendmentPatch`'s variants (minus their payloads),
/// which [`AmendOp::to_patch`] reconstructs from `op` + `index` + `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AmendOp {
    /// Replace the element at `--index` with `--value`.
    Replace,
    /// Insert `--value` at `--index`, shifting existing elements right.
    Insert,
    /// Remove the element at `--index`.
    Delete,
}

impl AmendField {
    /// Map to the plan-level field selector.
    pub fn to_field(self) -> AmendmentField {
        match self {
            AmendField::Acceptance => AmendmentField::Acceptance,
            AmendField::Wiring => AmendmentField::Wiring,
        }
    }
}

impl AmendOp {
    /// Build the plan-level patch, enforcing the value/op pairing that clap
    /// cannot express: `replace`/`insert` need `--value`, `delete` refuses it.
    pub fn to_patch(self, index: usize, value: Option<String>) -> Result<AmendmentPatch> {
        match (self, value) {
            (AmendOp::Replace, Some(value)) => Ok(AmendmentPatch::Replace { index, value }),
            (AmendOp::Insert, Some(value)) => Ok(AmendmentPatch::Insert { index, value }),
            (AmendOp::Delete, None) => Ok(AmendmentPatch::Delete { index }),
            (AmendOp::Replace, None) => bail!("--value is required for --op replace"),
            (AmendOp::Insert, None) => bail!("--value is required for --op insert"),
            (AmendOp::Delete, Some(_)) => bail!("--value is not accepted with --op delete"),
        }
    }
}

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

        /// No longer changes what gets cleared: the stage's session assignment
        /// is always cleared on reset, in both soft and hard mode. Only
        /// affects the printed "(hard reset)" vs "(soft reset)" label. Does
        /// NOT run git reset --hard in the worktree — clean the worktree
        /// manually if needed.
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

    /// Amend a stage's acceptance or wiring array in place (operator repair).
    ///
    /// Routes through the audited plan-amendment path: writes a numbered
    /// snapshot under .work/plan_versions/, appends an audit row, and
    /// rewrites both the plan file and the stage file. Use when a criterion
    /// is impossible rather than merely failing -- an agent inside a stage
    /// should file `loom stage dispute-criteria` instead.
    Amend {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Which array to mutate.
        #[arg(long)]
        field: AmendField,

        /// What to do at `--index`.
        #[arg(long)]
        op: AmendOp,

        /// 0-based index into the array.
        #[arg(long)]
        index: usize,

        /// YAML body for the new element. Required for replace/insert,
        /// rejected for delete.
        #[arg(long)]
        value: Option<String>,

        /// Reason recorded in the audit log.
        #[arg(long, value_parser = clap_description_validator)]
        reason: Option<String>,
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
