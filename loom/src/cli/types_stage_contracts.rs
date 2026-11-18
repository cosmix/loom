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

        /// Name the test of one contract: `<contract-id>=<test identifier>`.
        /// Repeat for every contract whose plan entry has no `test` hint. A
        /// later freeze keeps the flags it does not repeat.
        #[arg(long = "test", value_name = "ID=TEST")]
        test: Vec<String>,

        /// Name the runner adapter of one contract whose plan entry sets no
        /// `runner`: `<contract-id>=<adapter>`. Repeatable; a later freeze
        /// keeps the flags it does not repeat.
        #[arg(long, value_name = "ID=ADAPTER")]
        runner: Vec<String>,

        /// Name the run directory of one contract whose plan entry sets no
        /// `cwd`: `<contract-id>=<dir>`. Repeatable; a later freeze keeps the
        /// flags it does not repeat.
        #[arg(long, value_name = "ID=DIR")]
        cwd: Vec<String>,
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
