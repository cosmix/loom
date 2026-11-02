//! Post-merge base-reconcile + cleanup tail shared by `merge`'s success
//! paths (`merge_resolved` and `merge_retry`).

use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::CleanupConfig;
use crate::orchestrator::merge_lifecycle::{self, CleanupOutcome};

/// Run the primitive's post-merge base-reconcile + cleanup for a stage whose
/// merge was already verified, and print what cleanup actually did. Every case
/// where cleanup did not finish prints its own reason.
pub(super) fn finish_merge_and_report(
    stage_id: &str,
    repo_root: &Path,
    work_dir: &Path,
    target_branch: &str,
) {
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
    if let Some(message) = unfinished_cleanup_message(stage_id, &outcome) {
        println!("{message}");
    }
}

/// What to tell the operator when cleanup did not finish, so a deferred or
/// failed cleanup is never silent. `None` when cleanup ran or had nothing to do.
fn unfinished_cleanup_message(stage_id: &str, outcome: &CleanupOutcome) -> Option<String> {
    match outcome {
        CleanupOutcome::Refused { reason } => Some(format!("Worktree cleanup refused: {reason}")),
        CleanupOutcome::Failed(error) => Some(format!("Worktree cleanup failed: {error}")),
        CleanupOutcome::Deferred { reason } => Some(format!(
            "Worktree cleanup deferred: {reason}. The daemon removes .worktrees/{stage_id} \
             once nothing uses it (or run `loom worktree remove {stage_id}`)"
        )),
        CleanupOutcome::Done(result) if !result.warnings.is_empty() => {
            Some(format!("Worktree cleanup: {}", result.warnings.join("; ")))
        }
        CleanupOutcome::Done(_) | CleanupOutcome::NothingToDo => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_and_failed_cleanup_name_their_reason() {
        let refused = CleanupOutcome::Refused {
            reason: "unmerged commits".to_string(),
        };
        assert_eq!(
            unfinished_cleanup_message("s", &refused).unwrap(),
            "Worktree cleanup refused: unmerged commits"
        );
        let failed = CleanupOutcome::Failed("disk full".to_string());
        assert_eq!(
            unfinished_cleanup_message("s", &failed).unwrap(),
            "Worktree cleanup failed: disk full"
        );
    }

    #[test]
    fn deferred_cleanup_names_each_reason_the_daemon_and_the_manual_command() {
        for reason in [
            "the command runs inside .worktrees/s",
            "session session-1 (merge) still runs for the stage",
        ] {
            let outcome = CleanupOutcome::Deferred {
                reason: reason.to_string(),
            };
            let message = unfinished_cleanup_message("s", &outcome).unwrap();
            assert!(message.contains(reason), "{message}");
            assert!(
                message.contains("The daemon removes .worktrees/s"),
                "{message}"
            );
            assert!(message.contains("loom worktree remove s"), "{message}");
        }
    }

    #[test]
    fn finished_cleanup_prints_nothing_extra() {
        assert!(unfinished_cleanup_message("s", &CleanupOutcome::NothingToDo).is_none());
    }

    #[test]
    fn retained_paths_are_reported_after_successful_git_reconciliation() {
        let outcome = CleanupOutcome::Done(crate::git::cleanup::CleanupResult {
            worktree_removed: false,
            branch_deleted: true,
            warnings: vec!["retained .worktrees/s".to_string()],
            ..Default::default()
        });
        assert_eq!(
            unfinished_cleanup_message("s", &outcome).unwrap(),
            "Worktree cleanup: retained .worktrees/s"
        );
    }
}
