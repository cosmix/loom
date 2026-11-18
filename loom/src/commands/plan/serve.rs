//! Bare `loom plan`: serve the planning app for the project in the current directory.

use anyhow::Result;

use crate::cli::PlanServeArgs;
use crate::plan_server::{self, AlreadyRunning};

/// Run the planning server until stopped. A server already running for this project has
/// printed its URL to stderr; that exits 1 without a second error line.
pub fn execute(args: &PlanServeArgs) -> Result<()> {
    match plan_server::run(args) {
        Err(error) if error.is::<AlreadyRunning>() => std::process::exit(1),
        other => other,
    }
}
