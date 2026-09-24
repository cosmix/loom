//! `loom stage contracts` subcommands: the contract phase's freeze and the
//! frozen record it leaves behind.

use crate::validation::clap_id_validator;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ContractsCommands {
    /// Run every contract test, confirm each one fails, and ask the daemon to
    /// freeze the contract files. The contract session's last step.
    Freeze {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Print the freeze record: contracts with their outcomes, frozen files
    /// with their hashes, and where the frozen copies are kept
    Show {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,
    },

    /// Copy frozen contract files back into the stage worktree
    Restore {
        /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
        #[arg(value_parser = clap_id_validator)]
        stage_id: String,

        /// Restore only this contract's test file
        #[arg(long, value_parser = clap_id_validator)]
        contract: Option<String>,
    },
}
