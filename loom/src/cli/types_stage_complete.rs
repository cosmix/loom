//! Arguments of `loom stage complete` and `loom stage admin-proof`.

use clap::Args;

use crate::validation::clap_id_validator;

#[derive(Args)]
pub struct CompleteArgs {
    /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
    #[arg(value_parser = clap_id_validator)]
    pub stage_id: String,

    /// Session ID to also mark as completed
    #[arg(long, value_parser = clap_id_validator)]
    pub session: Option<String>,

    /// Skip acceptance criteria verification; the committed branch tip is
    /// recorded for landing. Refuses a branch with zero commits.
    /// Redundant with --force-unsafe, which runs no checks either.
    #[arg(long)]
    pub no_verify: bool,

    /// UNSAFE: Force completion from any state, bypassing state machine validation.
    /// Records the committed branch tip, if any, so the daemon's auto-merge
    /// can land it; a zero-commit branch records nothing.
    /// WARNING: This can corrupt dependency tracking. Use only for recovery.
    #[arg(long = "force-unsafe")]
    pub force_unsafe: bool,

    /// When using --force-unsafe, also mark stage as merged (assumes manual merge was done;
    /// the stage's commit must be in the target). Without this, dependents are triggered
    /// only once the recorded commit lands.
    #[arg(long = "assume-merged", requires = "force_unsafe")]
    pub assume_merged: bool,

    /// Re-run every acceptance criterion even when a pass is cached for this tree
    #[arg(long)]
    pub no_cache: bool,
}

#[derive(Args)]
pub struct AdminProofArgs {
    /// Stage ID the proof authorizes
    #[arg(required_unless_present = "daemon_stop", value_parser = clap_id_validator)]
    pub stage_id: Option<String>,

    /// Authorize one daemon shutdown instead of a stage completion
    #[arg(
        long = "daemon-stop",
        conflicts_with_all = ["stage_id", "no_verify", "force_unsafe", "assume_merged"]
    )]
    pub daemon_stop: bool,

    /// Authorize --no-verify for this stage completion
    #[arg(long, conflicts_with = "daemon_stop")]
    pub no_verify: bool,

    /// Authorize --force-unsafe for this stage completion
    #[arg(long = "force-unsafe", conflicts_with = "daemon_stop")]
    pub force_unsafe: bool,

    /// Authorize --assume-merged together with --force-unsafe
    #[arg(
        long = "assume-merged",
        requires = "force_unsafe",
        conflicts_with = "daemon_stop"
    )]
    pub assume_merged: bool,
}
