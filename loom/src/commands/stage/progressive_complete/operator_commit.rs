//! The commit an operator authorizes for landing at an explicit command
//! boundary: `complete --no-verify`, `complete --force-unsafe` and
//! `human-review --force-complete`. A recorded commit here means the operator
//! named the content to land, not that any check ran. Automatic recovery must
//! never call this.

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::branch::{branch_name_for_stage, commits_ahead_of};
use crate::git::cleanup::branch_exists_strict;
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::stage::Stage;

/// What the stage branch holds beyond the merge target.
pub(in crate::commands::stage) enum BranchWork {
    /// No `loom/<stage-id>` branch exists.
    Missing { branch: String },
    /// The branch has no commit beyond `target`: landing it would merge
    /// nothing while reporting success.
    Empty { branch: String, target: String },
    /// The committed branch tip, captured from the stage worktree, with at
    /// least one commit beyond the target. Persist it with `validate_locked`
    /// inside the `update_stage` that records it.
    Committed(VerificationSnapshot),
    /// The tip was captured, but counting its commits beyond the target
    /// failed, so the branch may hold no work. A path whose merge skips the
    /// zero-commit guard must refuse this.
    Unconfirmed {
        snapshot: VerificationSnapshot,
        error: anyhow::Error,
    },
}

/// Classify the stage branch and, when it may carry work, capture its tip.
/// A failed branch-existence probe is an error, never a missing branch.
pub(in crate::commands::stage) fn branch_work(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<BranchWork> {
    let target = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    let branch = branch_name_for_stage(&stage.id);
    if !branch_exists_strict(&branch, repo_root)? {
        return Ok(BranchWork::Missing { branch });
    }
    let ahead = commits_ahead_of(&branch, &target, repo_root);
    if matches!(ahead, Ok(0)) {
        return Ok(BranchWork::Empty { branch, target });
    }
    let snapshot = VerificationSnapshot::capture(stage, work_dir)
        .with_context(|| format!("cannot record the tip of '{branch}' for landing"))?;
    Ok(match ahead {
        Ok(_) => BranchWork::Committed(snapshot),
        Err(error) => BranchWork::Unconfirmed {
            snapshot,
            error: error.context(format!(
                "failed to count commits ahead of '{target}' on '{branch}'"
            )),
        },
    })
}

/// Report an unconfirmed ahead-count on a path that records the tip anyway.
/// Only paths that land through the daemon's merge gate (`MergeGate::Enforce`)
/// may do so: that gate counts the commits again and routes a zero-commit
/// branch to human review instead of merging it.
pub(in crate::commands::stage) fn warn_unconfirmed(error: &anyhow::Error) {
    eprintln!(
        "Warning: {error:#}. Recording the branch tip anyway; the daemon's merge \
         sends the stage to review if the branch has no commits."
    );
}

/// Report the recorded commit and the one it replaced, if any.
pub(in crate::commands::stage) fn report_recorded_commit(
    stage_id: &str,
    previous: Option<&str>,
    recorded: &str,
) {
    match previous {
        Some(old) if old != recorded => {
            println!("Stage '{stage_id}': recorded commit {recorded} (replaces {old}).")
        }
        _ => println!("Stage '{stage_id}': recorded commit {recorded}."),
    }
}
