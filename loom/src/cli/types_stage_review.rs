//! `loom stage review` subcommands: a stage's recorded code reviews
//! (DESIGN D12).

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
}
