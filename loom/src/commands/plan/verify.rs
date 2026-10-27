//! loom plan verify — validate a plan file without side effects.

use anyhow::{bail, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

mod checks;
mod human;
mod inputs;
pub mod report;
#[cfg(test)]
mod tests;

use report::VerifyReport;

fn should_fail(errors: usize, warnings: usize, strict: bool) -> bool {
    errors > 0 || (strict && warnings > 0)
}

/// Walk up from plan_path.parent() looking for a directory containing `.git`.
/// Handles both `.git` directories and `.git` files (worktrees).
fn find_repo_root(plan_path: &Path) -> Option<PathBuf> {
    plan_path
        .parent()?
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

fn emit_json(output: &VerifyReport) {
    match serde_json::to_string_pretty(output) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("JSON serialization error: {e}"),
    }
    let _ = std::io::stdout().flush();
}

pub fn execute(path: &Path, strict: bool, json: bool, no_color: bool) -> Result<()> {
    if no_color {
        colored::control::set_override(false);
    }
    let report = report::verify_path(path).unwrap_or_else(VerifyReport::from);
    let total_errors = report.errors.len();
    let total_warnings = report.warnings.total();
    let failed = should_fail(total_errors, total_warnings, strict);
    if json {
        emit_json(&report);
        std::process::exit(i32::from(failed));
    }
    if report.stopped {
        // Validation never ran: the human output is only the reason it stopped.
        let message = report.errors.first().map_or("", |e| e.message.as_str());
        bail!("{message}");
    }
    human::print_human(&report, strict);
    if failed {
        bail!("Plan validation failed ({total_errors} error(s), {total_warnings} warning(s))");
    }
    Ok(())
}
