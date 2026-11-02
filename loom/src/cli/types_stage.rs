//! Stage-related CLI command types

use crate::validation::{clap_description_validator, clap_id_validator};
use clap::Subcommand;

#[path = "types_stage_amend.rs"]
mod amend;
#[path = "types_stage_complete.rs"]
mod complete;
#[path = "types_stage_contracts.rs"]
mod contracts;
#[path = "types_stage_disputes.rs"]
mod disputes;
#[path = "types_stage_review.rs"]
mod review;

pub use amend::AmendArgs;
pub use complete::{AdminProofArgs, CompleteArgs};
pub use contracts::ContractsCommands;
pub use disputes::{
    CriteriaDispute, DisputeContractArgs, DisputeCriteriaArgs, DisputeFindingsArgs,
    DisputeIntegrityArgs,
};
pub use review::ReviewCommands;

#[derive(Subcommand)]
pub enum StageCommands {
    /// Mark a stage as complete (runs acceptance criteria by default).
    /// Privileged flags authorize themselves from the daemon token, which
    /// only an operator shell can read.
    Complete(CompleteArgs),

    /// Mint a one-time proof to hand to another process.
    /// Operators do not need this: privileged commands authorize themselves.
    /// Supply the daemon token through LOOM_ADMIN_TOKEN; only the proof is printed.
    AdminProof(AdminProofArgs),

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

        /// Recovery context message (triggers recovery signal generation). How an
        /// operator answers a stage that blocked itself: the next session reads it first.
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

    /// Commit the staged changes through the daemon
    ///
    /// Runs the pre-commit and commit-msg hooks here, then relays the staged
    /// tree; wait for it with `loom request status <id> --wait 90`.
    Commit {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Commit message (type(scope): description); repeat -m for further
        /// paragraphs, joined with a blank line as git does
        #[arg(short = 'm', long = "message", required = true)]
        message: Vec<String>,
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

        /// Force-complete: skip acceptance criteria, record the committed branch tip
        /// and merge it. Refuses a missing or zero-commit branch.
        #[arg(long, group = "action")]
        force_complete: bool,

        /// Reject: block the stage with the given reason (max 500 characters)
        #[arg(long, group = "action", value_parser = clap_description_validator)]
        reject: Option<String>,
    },

    /// Dispute an acceptance criterion, a wiring check, a wiring test, or
    /// failing impact-selected tests the stage did not cause.
    /// Files a structured dispute via the daemon RPC; the daemon writes
    /// request.md and transitions the stage to NeedsAdjudication. Verdict.md
    /// is written by the adjudicator (daemon-only), never by the agent.
    DisputeCriteria(DisputeCriteriaArgs),

    /// Dispute open review findings of a plan v2 stage, in one request.
    DisputeFindings(DisputeFindingsArgs),

    /// Dispute a frozen contract of a plan v2 stage.
    DisputeContract(DisputeContractArgs),

    /// Dispute test-integrity events of a plan v2 stage, in one request.
    DisputeIntegrity(DisputeIntegrityArgs),

    /// Record an adjudication session's verdict on a dispute.
    ///
    /// Run by the adjudication session the orchestrator spawns for a dispute,
    /// from the main repository, once it has written its JSON verdict to a
    /// file. The command validates the JSON and writes
    /// `.loom/work/disputes/<stage>/<n>/verdict.md`; the orchestrator applies the
    /// verdict on its next poll. A stage worktree session is refused: a stage
    /// may not judge its own disputed criterion.
    Adjudicate {
        /// Stage the dispute belongs to (alphanumeric, dash, underscore only;
        /// max 128 characters).
        #[arg(long, value_parser = clap_id_validator)]
        stage: String,

        /// Dispute id, as allocated when the dispute was filed.
        #[arg(long)]
        dispute: u32,

        /// Path to the JSON verdict the session wrote.
        #[arg(long = "verdict-file")]
        verdict_file: std::path::PathBuf,
    },

    /// Amend a stage's acceptance, wiring, wiring_tests, contracts, harness or reachable array in place
    /// (operator repair).
    ///
    /// Routes through the audited plan-amendment path: writes a numbered
    /// snapshot under .loom/work/plan_versions/, appends an audit row, and
    /// rewrites both the plan file and the stage file. Use when a criterion
    /// is impossible rather than merely failing -- an agent inside a stage
    /// should file `loom stage dispute-criteria` instead.
    Amend(AmendArgs),

    /// Manage stage outputs (structured values passed to dependent stages)
    Output {
        #[command(subcommand)]
        command: OutputCommands,
    },

    /// Freeze, show, or restore a stage's contract tests
    Contracts {
        #[command(subcommand)]
        command: ContractsCommands,
    },

    /// Show a stage's recorded code reviews
    Review {
        #[command(subcommand)]
        command: ReviewCommands,
    },

    /// Run one host verification job; the daemon starts it, never a session
    #[command(hide = true)]
    VerifyHost {
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// The job to run: the id of the stage's current running job
        #[arg(long)]
        job: String,
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
