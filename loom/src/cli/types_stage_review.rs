//! `loom stage review` subcommands: a stage's recorded code reviews
//! (DESIGN D12) and its test-integrity events (DESIGN D13).

use crate::validation::clap_id_validator;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ReviewCommands {
    /// Print the recorded review rounds, the open findings, whether the latest
    /// round matches the worktree's current changes, and the files changed
    /// since it: the brief for the next reviewer
    Status {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },
    /// Print the stage's current test-integrity events (fallen declaration or
    /// assertion totals, lost assertion lines, changed ratchet files) with
    /// their detail, and which of them are accepted
    Integrity {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },
}
