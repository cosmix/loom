//! The ordered steps shared by pressure pipeline drivers.

use std::path::{Path, PathBuf};

/// The record-writing step shown after the pressure rounds.
pub const RECORD_STEP_LABEL: &str = "write the ## Pressure tests record";

/// One step in the pressure pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Delete the codex report file if it exists.
    DeleteReport(PathBuf),
    /// Run the two independent pressure-tests concurrently: Claude `/pressure`
    /// in the foreground and Codex `$pressure` in the background.
    Pressure {
        /// Full positional slash invocation, e.g. `/pressure doc/plans/PLAN-foo.md`.
        claude: String,
        /// Full positional skill invocation, e.g. `$pressure doc/plans/PLAN-foo.md`.
        codex: String,
    },
    /// Run Claude `/address` in the foreground to fold the review into the plan.
    Address(String),
}

/// Build the ordered list of steps for `rounds` rounds.
///
/// Each round: delete the report (so a failed Codex write can't leave
/// `/address` reading the previous round's report) → run Claude `/pressure` and
/// Codex `$pressure` concurrently → Claude `/address`. After all rounds, one
/// final report deletion as cleanup.
pub fn plan_steps(rounds: u32, invocation: &str, report: &Path) -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..rounds {
        steps.push(Step::DeleteReport(report.to_path_buf()));
        steps.push(Step::Pressure {
            claude: format!("/pressure {invocation}"),
            codex: format!("$pressure {invocation}"),
        });
        steps.push(Step::Address(format!("/address {invocation}")));
    }
    steps.push(Step::DeleteReport(report.to_path_buf()));
    steps
}
