//! `loom stage verify-host <stage-id> --job <job-id>`: the host child the
//! daemon's job runner starts for one verification job. Hidden from help.
//!
//! Exit codes: 0 the job passed, 1 it failed (its result says why), 2 the
//! child refused to run or could not write a result.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::host_verify::child::run_job;
use crate::host_verify::JobResult;

const EXIT_FAILED: i32 = 1;
const EXIT_REFUSED: i32 = 2;

pub fn execute(stage_id: String, job: String) -> Result<()> {
    if ["LOOM_SESSION_ID", "LOOM_STAGE_ID"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        eprintln!("verify-host runs only from the loom daemon");
        std::process::exit(EXIT_REFUSED);
    }
    let result = state_root().and_then(|work_dir| run_job(&work_dir, &stage_id, &job));
    match result {
        Ok(result) => {
            print_summary(&result);
            if !result.passed {
                std::process::exit(EXIT_FAILED);
            }
            Ok(())
        }
        Err(err) => {
            eprintln!("verify-host: {err:#}");
            std::process::exit(EXIT_REFUSED);
        }
    }
}

/// The state root from `LOOM_WORK_DIR`, canonicalized. Never resolved from
/// the current directory: in a worktree that names the live plan's state.
fn state_root() -> Result<PathBuf> {
    let raw = std::env::var_os("LOOM_WORK_DIR").context("LOOM_WORK_DIR is not set")?;
    let path = PathBuf::from(raw)
        .canonicalize()
        .context("LOOM_WORK_DIR does not resolve")?;
    anyhow::ensure!(
        path.is_dir(),
        "LOOM_WORK_DIR is not a directory: {}",
        path.display()
    );
    Ok(path)
}

/// One line per outcome, then the error; the runner keeps it in `child.log`.
fn print_summary(result: &JobResult) {
    for outcome in &result.criteria {
        let mark = if outcome.passed { "✓" } else { "✗" };
        println!("{mark} {}", outcome.label);
    }
    if let Some(error) = &result.error {
        println!("error: {error}");
    }
}
