//! Non-destructive merge-conflict probing with explicit failure categories.

use super::{abort_merge, checkout_branch, get_conflicting_files, lock::MergeLock};
use crate::git::runner::{run_git, run_git_checked};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

/// Successful result of probing a prospective merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeProbeOutcome {
    /// The source can be merged without conflicts.
    Clean,
    /// Git created genuine unmerged index entries for these paths.
    Conflicts(Vec<String>),
}

/// Failure to execute a merge probe or restore the repository afterwards.
#[derive(Debug)]
pub enum MergeProbeError {
    /// Git or the repository could not perform a probe operation.
    Infrastructure {
        /// Operation that failed.
        operation: &'static str,
        /// Context-rich failure detail.
        details: String,
    },
    /// The probe ran, but aborting it or restoring the original checkout failed.
    Restoration {
        /// One or more restoration failures.
        details: String,
    },
}

impl std::fmt::Display for MergeProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Infrastructure { operation, details } => write!(
                formatter,
                "merge probe infrastructure failure during {operation}: {details}"
            ),
            Self::Restoration { details } => write!(
                formatter,
                "merge probe repository restoration failed: {details}"
            ),
        }
    }
}

impl std::error::Error for MergeProbeError {}

/// Typed result of a merge-conflict probe.
pub type MergeProbeResult = std::result::Result<MergeProbeOutcome, MergeProbeError>;

/// Probe whether `source_branch` can merge into `target_branch`.
///
/// The repository must be clean. The original checkout is restored on every
/// path, and abort/checkout restoration failures are returned as
/// [`MergeProbeError::Restoration`] rather than being discarded.
pub fn get_conflicting_files_from_status(
    source_branch: &str,
    target_branch: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> MergeProbeResult {
    let _lock = MergeLock::acquire(work_dir, Duration::from_secs(30))
        .map_err(|error| infrastructure("lock acquisition", error))?;
    require_clean_repository(repo_root)?;
    require_no_merge(repo_root)?;

    let original_ref = checkout_reference(repo_root)?;
    let mut guard = RepositoryStateGuard::new(repo_root, original_ref);
    let probe_result = run_probe(source_branch, target_branch, repo_root);
    let restoration = guard.restore();

    match restoration {
        Ok(()) => probe_result,
        Err(MergeProbeError::Restoration { mut details }) => {
            if let Err(probe_error) = probe_result {
                details.push_str(&format!("; probe also failed: {probe_error}"));
            }
            Err(MergeProbeError::Restoration { details })
        }
        Err(error) => Err(error),
    }
}

fn run_probe(source_branch: &str, target_branch: &str, repo_root: &Path) -> MergeProbeResult {
    checkout_branch(target_branch, repo_root)
        .map_err(|error| infrastructure("target checkout", error))?;
    let args = ["merge", "--no-commit", "--no-ff", source_branch];
    let output = run_git(&args, repo_root).map_err(|error| infrastructure("merge", error))?;

    if output.status.success() {
        return Ok(MergeProbeOutcome::Clean);
    }
    if !merge_head_exists_strict(repo_root)? {
        return Err(MergeProbeError::Infrastructure {
            operation: "merge",
            details: format_git_failure(&args, repo_root, &output),
        });
    }

    let conflicts = get_conflicting_files(repo_root)
        .map_err(|error| infrastructure("conflict inspection", error))?;
    if conflicts.is_empty() {
        Err(MergeProbeError::Infrastructure {
            operation: "merge",
            details: format!(
                "git failed with MERGE_HEAD present but produced no unmerged paths; {}",
                format_git_failure(&args, repo_root, &output)
            ),
        })
    } else {
        Ok(MergeProbeOutcome::Conflicts(conflicts))
    }
}

fn require_clean_repository(repo_root: &Path) -> std::result::Result<(), MergeProbeError> {
    let status = run_git_checked(
        &["status", "--porcelain=v1", "--untracked-files=all"],
        repo_root,
    )
    .map_err(|error| infrastructure("cleanliness check", error))?;
    if status.is_empty() {
        Ok(())
    } else {
        Err(MergeProbeError::Infrastructure {
            operation: "cleanliness check",
            details: format!("repository has uncommitted changes:\n{status}"),
        })
    }
}

fn require_no_merge(repo_root: &Path) -> std::result::Result<(), MergeProbeError> {
    if merge_head_exists_strict(repo_root)? {
        Err(MergeProbeError::Infrastructure {
            operation: "preflight",
            details: "a merge is already in progress; refusing to disturb it".to_string(),
        })
    } else {
        Ok(())
    }
}

fn checkout_reference(repo_root: &Path) -> std::result::Result<String, MergeProbeError> {
    let output = run_git(&["symbolic-ref", "--quiet", "--short", "HEAD"], repo_root)
        .map_err(|error| infrastructure("original checkout inspection", error))?;
    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).trim().to_string()),
        Some(1) => run_git_checked(&["rev-parse", "--verify", "HEAD"], repo_root)
            .map_err(|error| infrastructure("detached HEAD inspection", error)),
        _ => Err(MergeProbeError::Infrastructure {
            operation: "original checkout inspection",
            details: format_git_failure(
                &["symbolic-ref", "--quiet", "--short", "HEAD"],
                repo_root,
                &output,
            ),
        }),
    }
}

fn merge_head_exists_strict(repo_root: &Path) -> std::result::Result<bool, MergeProbeError> {
    let args = ["rev-parse", "--verify", "--quiet", "MERGE_HEAD"];
    let output = run_git(&args, repo_root)
        .map_err(|error| infrastructure("MERGE_HEAD inspection", error))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(MergeProbeError::Infrastructure {
            operation: "MERGE_HEAD inspection",
            details: format_git_failure(&args, repo_root, &output),
        }),
    }
}

fn infrastructure(operation: &'static str, error: impl std::fmt::Display) -> MergeProbeError {
    MergeProbeError::Infrastructure {
        operation,
        details: error.to_string(),
    }
}

fn format_git_failure(args: &[&str], repo_root: &Path, output: &Output) -> String {
    let exit = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "signal".to_string());
    format!(
        "git {} failed (exit {exit}); directory: {}; stdout: {}; stderr: {}",
        args.join(" "),
        repo_root.display(),
        display_output(&output.stdout),
        display_output(&output.stderr)
    )
}

fn display_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        text.trim().to_string()
    }
}

struct RepositoryStateGuard {
    repo_root: PathBuf,
    original_ref: String,
    armed: bool,
}

impl RepositoryStateGuard {
    fn new(repo_root: &Path, original_ref: String) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            original_ref,
            armed: true,
        }
    }

    fn restore(&mut self) -> std::result::Result<(), MergeProbeError> {
        if !self.armed {
            return Ok(());
        }
        self.armed = false;
        let mut failures = Vec::new();

        match merge_head_exists_strict(&self.repo_root) {
            Ok(true) => {
                if let Err(error) = abort_merge(&self.repo_root) {
                    failures.push(format!("merge abort failed: {error}"));
                }
            }
            Ok(false) => {}
            Err(error) => failures.push(format!("MERGE_HEAD inspection failed: {error}")),
        }
        if let Err(error) = checkout_branch(&self.original_ref, &self.repo_root) {
            failures.push(format!("checkout '{}' failed: {error}", self.original_ref));
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(MergeProbeError::Restoration {
                details: failures.join("; "),
            })
        }
    }
}

impl Drop for RepositoryStateGuard {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            tracing::error!(%error, "Failed to restore repository during merge-probe drop");
        }
    }
}

#[cfg(test)]
mod tests;
