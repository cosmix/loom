//! Git merge operations for integrating worktree branches

mod checkout_apply;
mod checkout_files;
mod checkout_state;
pub mod control_paths;
mod fast_forward;
pub mod in_progress;
mod inputs;
pub mod lock;
mod operation;
mod resolved;
mod status;
mod tree;

use anyhow::{bail, Result};
use std::path::Path;
use std::time::Duration;

use super::branch::{branch_exists, branch_name_for_stage, branch_ref, is_ancestor_of};
use crate::git::runner::run_git_checked;
use crate::git::target_guard::{self, GuardState};
use lock::MergeLock;
use operation::operator_operation;

// Re-export status types for use by other modules
pub use control_paths::{
    control_path_violation, control_path_violation_as, tree_change_violation, MergeGate,
};
pub use in_progress::{
    detect_in_progress_merge_at, detect_in_progress_merge_at_worktree, detect_in_progress_merges,
    git_dir_for_repo_path, merge_head_exists, ActiveMergeState, InProgressMerge, MergeLocation,
};
pub use inputs::blocked_merge_inputs;
pub use resolved::check_resolved_worktree;
pub use status::{build_merge_report, check_merge_state, MergeState, MergeStatusReport};
pub(crate) use tree::rev_parse;
pub use tree::{
    advance_target, commit_merge, merge_tree, Advance, MergeBlock, PendingMerge, StashReapply,
    TreeMerge,
};

/// Result of a merge operation
#[derive(Debug, Clone)]
pub enum MergeResult {
    /// The target branch now points at a two-parent merge commit.
    Success {
        /// Number of files changed
        files_changed: u32,
        /// Number of insertions
        insertions: u32,
        /// Number of deletions
        deletions: u32,
        /// The operator's stashed changes, when they were stashed around
        /// the merge.
        stash: Option<StashReapply>,
    },
    /// Merge has conflicts that need resolution; nothing was changed.
    Conflict {
        /// List of files with conflicts
        conflicting_files: Vec<String>,
    },
    /// Nothing to merge (the stage branch is already in the target)
    AlreadyUpToDate,
    /// The merge was computed but the target was not advanced.
    Blocked(MergeBlock),
    /// The control-path gate refused the branch ([`MergeGate::Enforce`]);
    /// nothing was changed. `reason` is the human-review reason.
    Held { reason: String },
}

/// Merge a stage branch into the target branch (typically main) without
/// changing the main checkout beyond a final fast-forward.
///
/// Steps, under the merge lock:
/// 1. Refuse while the operator has a merge, cherry-pick, revert or rebase
///    in progress in the checkout.
/// 2. Refuse while the target guard holds the target
///    ([`MergeBlock::TargetHeld`]), for every gate mode: the bypass covers the
///    stage's own control paths, never a target that moved outside loom.
/// 3. With [`MergeGate::Enforce`], hold a branch whose diff touches a control
///    path. The gate reads the same two commits that are merged, under the
///    lock, so a commit made to the branch while the lock was awaited cannot
///    slip past it.
/// 4. Compute the merge with `git merge-tree`; conflicts leave everything
///    untouched. With the gate enforced, the diff from the target to the
///    merged tree is gated as well.
/// 5. Advance the target with [`advance_target`], which writes the merge
///    commit with `git commit-tree` only when the advance can happen, and
///    move the guard's accepted tip along with it.
pub fn merge_stage(
    stage_id: &str,
    target_branch: &str,
    repo_root: &Path,
    work_dir: &Path,
    gate: MergeGate,
) -> Result<MergeResult> {
    let _lock = MergeLock::acquire(work_dir, Duration::from_secs(30)).map_err(|e| {
        anyhow::anyhow!(
            "Could not acquire merge lock: {}. Another merge may be in progress.",
            e
        )
    })?;

    if let Some(marker) = operator_operation(repo_root)? {
        return Ok(MergeResult::Blocked(MergeBlock::OperatorOperation {
            marker,
        }));
    }

    let (old, branch_tip) = merge_tips(stage_id, target_branch, repo_root)?;
    verify_completed_tip(stage_id, repo_root, work_dir, &branch_tip)?;
    if let Some(block) = target_hold(repo_root, work_dir, target_branch, &old)? {
        return Ok(MergeResult::Blocked(block));
    }
    if is_ancestor_of(&branch_tip, &old, repo_root)? {
        return Ok(MergeResult::AlreadyUpToDate);
    }

    if gate == MergeGate::Enforce {
        // Fails closed: a diff that cannot be computed is an error, not a pass.
        let label = branch_name_for_stage(stage_id);
        if let Some(reason) = control_path_violation_as(repo_root, &old, &branch_tip, &label)? {
            return Ok(MergeResult::Held { reason });
        }
    }

    merge_and_advance(
        (repo_root, work_dir),
        stage_id,
        target_branch,
        (&old, &branch_tip),
        gate,
    )
}

/// Ordinary completion pins the stage tip. Explicit conflict resolution keeps
/// its separate clean-worktree and original-work ancestry requirements.
fn verify_completed_tip(
    stage_id: &str,
    repo_root: &Path,
    work_dir: &Path,
    branch_tip: &str,
) -> Result<()> {
    // Standalone merge callers have no stage store. Once initialized, missing
    // or unreadable stage state must not silently disable the completion guard.
    if !work_dir.join("stages").try_exists()? {
        return Ok(());
    }
    let stage = crate::verify::transitions::load_stage(stage_id, work_dir)?;
    if stage.status == crate::models::stage::StageStatus::MergeConflict {
        return check_resolved_worktree(repo_root, stage_id, stage.completed_commit.as_deref())
            .map_err(anyhow::Error::msg);
    }
    if stage.status == crate::models::stage::StageStatus::Completed || stage.merged {
        anyhow::ensure!(
            stage.completed_commit.is_some(),
            "completed stage has no verified commit; rerun completion before landing"
        );
    }
    if let Some(verified) = stage.completed_commit.as_deref() {
        anyhow::ensure!(
            verified == branch_tip,
            "stage branch changed after verification; rerun completion before landing"
        );
    }
    Ok(())
}

/// The target's tip and the stage branch's tip, resolved by full ref name so
/// a tag named like either branch cannot win. Fails when the stage branch does
/// not exist.
fn merge_tips(stage_id: &str, target_branch: &str, repo_root: &Path) -> Result<(String, String)> {
    let branch_name = branch_name_for_stage(stage_id);
    if !branch_exists(&branch_name, repo_root)? {
        bail!("Branch '{branch_name}' does not exist");
    }
    let old = tree::rev_parse(repo_root, &branch_ref(target_branch))?;
    let branch_tip = tree::rev_parse(repo_root, &branch_ref(&branch_name))?;
    Ok((old, branch_tip))
}

/// [`MergeBlock::TargetHeld`] when the target guard holds `target_branch` at
/// `old`. The caller holds the merge lock.
fn target_hold(
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
    old: &str,
) -> Result<Option<MergeBlock>> {
    Ok(
        match target_guard::check_locked(repo_root, work_dir, target_branch, old)? {
            GuardState::Held(hold) => Some(MergeBlock::TargetHeld {
                target: target_guard::target_key(target_branch).to_string(),
                accepted: hold.accepted,
                observed: hold.observed,
            }),
            GuardState::Clear { .. } => None,
        },
    )
}

/// Compute and land the merge of `branch_tip` into `old`.
fn merge_and_advance(
    (repo_root, work_dir): (&Path, &Path),
    stage_id: &str,
    target_branch: &str,
    (old, branch_tip): (&str, &str),
    gate: MergeGate,
) -> Result<MergeResult> {
    let merged_tree = match merge_tree(repo_root, old, branch_tip)? {
        TreeMerge::Clean { tree } => tree,
        TreeMerge::Conflict { paths, .. } => {
            return Ok(MergeResult::Conflict {
                conflicting_files: paths,
            })
        }
    };
    if gate == MergeGate::Enforce {
        // The exact change that lands, not only the merge-base diff.
        let label = branch_name_for_stage(stage_id);
        if let Some(reason) = tree_change_violation(repo_root, old, &merged_tree, &label)? {
            return Ok(MergeResult::Held { reason });
        }
    }
    let msg = format!(
        "Merge {} into {target_branch}",
        branch_name_for_stage(stage_id)
    );
    let shortstat = run_git_checked(&["diff", "--shortstat", old, &merged_tree], repo_root)?;
    let (files_changed, insertions, deletions) = parse_merge_stats(&shortstat);
    let mut pending = PendingMerge::new(repo_root, &merged_tree, [old, branch_tip], &msg);

    let stash = match advance_target(repo_root, target_branch, stage_id, &mut pending)? {
        Advance::Advanced { stash } => stash,
        Advance::Blocked(block) => return Ok(MergeResult::Blocked(block)),
    };
    record_merge(work_dir, target_branch, old, &mut pending);
    Ok(MergeResult::Success {
        files_changed,
        insertions,
        deletions,
        stash,
    })
}

/// Move the target guard's accepted tip from `old` to the merge commit loom
/// just landed. A failure only warns: the merge landed, and the next
/// evaluation judges the move.
fn record_merge(work_dir: &Path, target_branch: &str, old: &str, pending: &mut PendingMerge) {
    let recorded = pending
        .commit()
        .and_then(|merge| target_guard::record_advance(work_dir, target_branch, old, &merge));
    if let Err(error) = recorded {
        tracing::warn!("target guard: could not record the merge into {target_branch}: {error:#}");
    }
}

/// Parse merge statistics from git output
fn parse_merge_stats(output: &str) -> (u32, u32, u32) {
    let mut files_changed = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;

    for line in output.lines() {
        // Look for line like: "3 files changed, 10 insertions(+), 5 deletions(-)"
        if line.contains("files changed") || line.contains("file changed") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "files" || *part == "file") && i > 0 {
                    files_changed = parts[i - 1].parse().unwrap_or(0);
                }
                if part.contains("insertion") && i > 0 {
                    insertions = parts[i - 1].parse().unwrap_or(0);
                }
                if part.contains("deletion") && i > 0 {
                    deletions = parts[i - 1].parse().unwrap_or(0);
                }
            }
        }
    }

    (files_changed, insertions, deletions)
}

/// Get list of files with conflicts (during an active merge)
pub fn get_conflicting_files(repo_root: &Path) -> Result<Vec<String>> {
    let stdout = run_git_checked(&["diff", "--name-only", "--diff-filter=U"], repo_root)?;
    let files: Vec<String> = stdout
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}

/// Verify that a merge actually succeeded by checking git ancestry.
///
/// This prevents "phantom merges" where the merged flag is set but the code
/// was never actually integrated. We verify by checking if the stage's
/// completed commit is reachable from the target branch.
///
/// # Arguments
/// * `completed_commit` - The commit SHA from when the stage completed
/// * `target_branch` - The branch we should have merged into (e.g., "main")
/// * `repo_root` - Path to the git repository root
///
/// # Returns
/// * `Ok(true)` if the commit is in the target branch's history (merge verified)
/// * `Ok(false)` if the commit is NOT in the target branch's history (merge not verified)
/// * `Err` if git command fails
pub fn verify_merge_succeeded(
    completed_commit: &str,
    target_branch: &str,
    repo_root: &Path,
) -> Result<bool> {
    is_ancestor_of(completed_commit, &branch_ref(target_branch), repo_root)
}

#[cfg(test)]
mod stage_overlap_tests;
#[cfg(test)]
mod stage_tests;
#[cfg(test)]
mod tag_shadow_tests;
#[cfg(test)]
mod target_guard_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod completion_tests;
