//! Post-merge base-reconcile + cleanup tail shared by `merge`'s success
//! paths (`merge_resolved` and `merge_retry`'s three success arms).

use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::CleanupConfig;
use crate::orchestrator::merge_lifecycle::{self, CleanupOutcome};

/// Run the primitive's post-merge base-reconcile + cleanup for a stage whose
/// merge was already verified, and print what cleanup actually did. Returns
/// the outcome so callers can decide whether to add their own hint for the
/// non-`Done` cases.
///
/// `result.warnings` is not surfaced here: `MergeLifecycle::cleanup` reaches
/// `CleanupOutcome::Done` only via `cleanup_after_merge`, which always
/// builds an empty `warnings` vec on its `Ok` path (only
/// `cleanup_multiple_stages` ever populates it) — dead on this path.
pub(super) fn finish_merge_and_report(
    stage_id: &str,
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
) -> CleanupOutcome {
    let outcome = merge_lifecycle::finish_verified_merge(
        stage_id,
        repo_root,
        work_dir,
        target_branch,
        &CleanupConfig::quiet(),
    );
    if let CleanupOutcome::Done(result) = &outcome {
        if result.worktree_removed {
            println!("Removed worktree: .worktrees/{stage_id}");
        }
        if result.branch_deleted {
            println!("Deleted branch: {}", branch_name_for_stage(stage_id));
        }
    }
    outcome
}
