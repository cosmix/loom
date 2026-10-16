//! `merge_retry`'s conflict outcome: record the conflict on the stage, then
//! tell the operator what resolves it next.

use anyhow::Result;
use std::path::Path;

use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::update_stage;

use super::next_step::{print_daemon_next_step, print_fix_limit_options};

/// Persist a conflict `loom stage merge` hit and print the next step.
///
/// `stage` carries the fix-attempt count the caller already persisted. A
/// `MergeBlocked`/`MergeConflict` stage moves to `MergeConflict`, and the
/// daemon's next step for it prints before the manual options. A `Completed`
/// stage that was never merged stays as it is, so only the manual options
/// print. The conflicting files print first, so a failed status write still
/// shows them.
pub(super) fn record_conflict_and_report(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    conflicting_files: &[String],
) -> Result<()> {
    let stage_id = stage.id.as_str();
    println!();
    println!("Merge conflict persists.");
    println!();
    println!("Conflicting files:");
    for file in conflicting_files {
        println!("  - {file}");
    }
    println!();

    let recorded = update_stage(stage_id, work_dir, mark_conflict)?;
    if recorded.status == StageStatus::MergeConflict {
        println!("Stage '{stage_id}' is now MergeConflict.");
        print_daemon_next_step(&recorded, work_dir, repo_root);
        println!();
    }
    print_manual_options(stage);
    Ok(())
}

/// Record the conflict on the fresh on-disk stage: `MergeConflict` with
/// `merge_conflict` set, and `failure_info` and `merge_block` cleared (no
/// conflict path records one, and the error an earlier `MergeBlocked` stored
/// would misdescribe the stage). Completion proof is preserved as recorded.
///
/// A `Completed` stage that was never merged (its plan disables auto-merge)
/// is left untouched: no edge leads from `Completed` to `MergeConflict`, and
/// a `MergeConflict` stage gets the daemon resolver the plan opted out of.
fn mark_conflict(stage: &mut Stage) -> Result<()> {
    if stage.status == StageStatus::Completed && !stage.merged {
        return Ok(());
    }
    stage.try_mark_merge_conflict()?;
    stage.clear_merge_block();
    stage.failure_info = None;
    Ok(())
}

/// The manual way to resolve the conflict, by whether fix attempts remain.
fn print_manual_options(stage: &Stage) {
    let stage_id = stage.id.as_str();
    let (attempts, max_attempts) = (stage.fix_attempts, stage.get_effective_max_fix_attempts());
    if stage.is_at_fix_limit() {
        println!("Fix attempt limit reached ({attempts}/{max_attempts}).");
        println!();
        print_fix_limit_options(stage_id);
    } else {
        println!("To resolve by hand, in .worktrees/{stage_id}: merge the target branch into it,");
        println!("resolve the conflicts, commit, then run:");
        println!("  loom stage merge {stage_id} --resolved");
        println!();
        println!(
            "Remaining attempts: {}/{max_attempts}",
            max_attempts - attempts
        );
    }
}

#[cfg(test)]
mod tests {
    use super::mark_conflict;
    use crate::git::MergeBlock;
    use crate::models::failure::{FailureInfo, FailureType};
    use crate::models::stage::{Stage, StageStatus};

    fn stage_with(status: StageStatus) -> Stage {
        Stage {
            id: "s".to_string(),
            status,
            ..Stage::default()
        }
    }

    fn infrastructure_failure() -> FailureInfo {
        FailureInfo {
            failure_type: FailureType::InfrastructureError,
            detected_at: chrono::Utc::now(),
            evidence: vec!["uncommitted changes on main".to_string()],
        }
    }

    #[test]
    fn a_merge_blocked_stage_becomes_merge_conflict_without_its_old_error() {
        let mut stage = stage_with(StageStatus::MergeBlocked);
        stage.failure_info = Some(infrastructure_failure());
        stage.merge.block = Some(MergeBlock::TargetMoved);
        mark_conflict(&mut stage).unwrap();
        assert_eq!(stage.status, StageStatus::MergeConflict);
        assert!(stage.merge_conflict);
        assert!(stage.failure_info.is_none());
        assert!(stage.merge.block.is_none());
    }

    #[test]
    fn conflict_recording_preserves_existing_or_missing_completion_proof() {
        let mut stage = stage_with(StageStatus::MergeBlocked);
        mark_conflict(&mut stage).unwrap();
        assert_eq!(stage.completed_commit, None);
        stage.completed_commit = Some("aaa".into());
        mark_conflict(&mut stage).unwrap();
        assert_eq!(stage.completed_commit.as_deref(), Some("aaa"));
    }

    #[test]
    fn a_merge_conflict_stage_stays_with_its_flag_set() {
        let mut stage = stage_with(StageStatus::MergeConflict);
        mark_conflict(&mut stage).unwrap();
        assert_eq!(stage.status, StageStatus::MergeConflict);
        assert!(stage.merge_conflict);
    }

    #[test]
    fn an_unmerged_completed_stage_is_left_untouched() {
        let mut stage = stage_with(StageStatus::Completed);
        stage.failure_info = Some(infrastructure_failure());
        mark_conflict(&mut stage).unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(!stage.merge_conflict);
        assert!(stage.failure_info.is_some());
    }

    #[test]
    fn a_stage_merged_meanwhile_is_left_alone() {
        let mut stage = stage_with(StageStatus::Completed);
        stage.merged = true;
        assert!(mark_conflict(&mut stage).is_err());
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(!stage.merge_conflict);
    }
}
