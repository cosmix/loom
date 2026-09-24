//! `loom project` subcommands: facts about the checkout itself.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Report each package's language kinds, test runner and language skills
    ///
    /// Scans the checkout that contains PATH and prints its root, whether the
    /// bounded scan was truncated, and one line per package. A package whose
    /// stack has no test-runner adapter reports `runner=unsupported`
    /// (`"runner":null` with --json).
    Detect {
        /// Directory inside the checkout to scan (default: the current directory)
        path: Option<PathBuf>,

        /// Print one line of compact JSON instead of the human-readable report
        #[arg(long)]
        json: bool,
    },
}
