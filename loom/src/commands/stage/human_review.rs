//! Human review response for a stage
//!
//! Allows a human to respond to a stage flagged for review via dispute-criteria.
//! Supports three actions: approve (resume), force-complete (skip acceptance), reject (block).

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

/// Handle human review response for a stage.
///
/// One of `approve`, `force_complete`, or `reject_reason` must be provided.
/// If none are provided, shows the current review status and available actions.
pub fn human_review(
    stage_id: String,
    approve: bool,
    force_complete: bool,
    reject_reason: Option<String>,
) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // If no action flag is provided, show current status
    if !approve && !force_complete && reject_reason.is_none() {
        return show_review_status(&stage_id, &stage);
    }

    // Verify the stage is in NeedsHumanReview
    if stage.status != StageStatus::NeedsHumanReview {
        bail!(
            "Stage '{}' is in '{}' state. human-review requires NeedsHumanReview.",
            stage_id,
            stage.status
        );
    }

    if approve {
        handle_approve(&mut stage, &stage_id, work_dir)
    } else if force_complete {
        handle_force_complete(&mut stage, &stage_id, work_dir)
    } else if let Some(reason) = reject_reason {
        handle_reject(&mut stage, &stage_id, &reason, work_dir)
    } else {
        unreachable!()
    }
}

/// Show current review status and available actions.
fn show_review_status(stage_id: &str, stage: &crate::models::stage::Stage) -> Result<()> {
    if stage.status != StageStatus::NeedsHumanReview {
        bail!(
            "Stage '{}' is in '{}' state, not awaiting human review.",
            stage_id,
            stage.status
        );
    }

    println!("Stage '{stage_id}' is awaiting human review.");
    println!();
    if let Some(ref reason) = stage.review_reason {
        println!("Review reason: {reason}");
    } else {
        println!("Review reason: (none recorded)");
    }
    println!();
    println!("Available actions:");
    println!("  loom stage human-review {stage_id} --approve         Resume execution with fresh fix attempts");
    println!("  loom stage human-review {stage_id} --force-complete  Skip acceptance and mark as completed");
    println!(
        "  loom stage human-review {stage_id} --reject <reason> Block the stage with a reason"
    );

    Ok(())
}

/// Approve the review: resume execution with fresh fix attempts.
fn handle_approve(
    stage: &mut crate::models::stage::Stage,
    stage_id: &str,
    work_dir: &Path,
) -> Result<()> {
    stage.try_approve_review()?;
    stage.fix_attempts = 0;
    save_stage(stage, work_dir)?;

    println!("Stage '{stage_id}' approved. Agent can continue with fresh fix attempts.");

    Ok(())
}

/// Force-complete the review: skip acceptance criteria and mark as completed.
fn handle_force_complete(
    stage: &mut crate::models::stage::Stage,
    stage_id: &str,
    work_dir: &Path,
) -> Result<()> {
    eprintln!(
        "WARNING: Force-completing stage '{stage_id}' without acceptance criteria verification."
    );

    stage.try_force_complete_review()?;

    // Attempt progressive merge
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());

    // Reset status to allow complete_with_merge to work
    // complete_with_merge calls try_complete which expects a non-Completed status.
    // Since try_force_complete_review already moved to Completed, we need to
    // handle merge separately.
    stage.merged = false;

    // Try to merge the stage branch
    match super::progressive_complete::attempt_progressive_merge(stage, &repo_root, work_dir)? {
        super::progressive_complete::MergeOutcome::Success => {
            save_stage(stage, work_dir)?;
            println!("Stage '{stage_id}' force-completed. Review the merge result.");

            let triggered = trigger_dependents(stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;
            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  -> {dep_id}");
                }
            }
        }
        super::progressive_complete::MergeOutcome::Conflict
        | super::progressive_complete::MergeOutcome::Blocked => {
            // Stage already saved in conflict/blocked state by attempt_progressive_merge
            println!(
                "Stage '{stage_id}' force-completed but merge had issues. Review the merge result."
            );
        }
    }

    Ok(())
}

/// Reject the review: block the stage with a reason.
fn handle_reject(
    stage: &mut crate::models::stage::Stage,
    stage_id: &str,
    reason: &str,
    work_dir: &Path,
) -> Result<()> {
    stage.try_reject_review(reason.to_string())?;
    stage.close_reason = Some(reason.to_string());
    save_stage(stage, work_dir)?;

    println!("Stage '{stage_id}' rejected and blocked.");
    println!("Reason: {reason}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use tempfile::TempDir;

    fn setup_stage(temp: &TempDir, status: StageStatus, review_reason: Option<&str>) -> Stage {
        let stages_dir = temp.path().join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let stage = Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            status,
            review_reason: review_reason.map(|s| s.to_string()),
            fix_attempts: 5,
            ..Default::default()
        };

        crate::verify::transitions::save_stage(&stage, temp.path()).unwrap();
        stage
    }

    #[test]
    fn test_human_review_approve() {
        let temp = TempDir::new().unwrap();
        setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));

        let work_dir = temp.path();
        let mut stage = load_stage("test-stage", work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);
        assert_eq!(stage.fix_attempts, 5);

        stage.try_approve_review().unwrap();
        stage.fix_attempts = 0;

        assert_eq!(stage.status, StageStatus::Executing);
        assert_eq!(stage.fix_attempts, 0);
        assert_eq!(stage.review_reason, None);
    }

    #[test]
    fn test_human_review_force_complete() {
        let temp = TempDir::new().unwrap();
        setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));

        let work_dir = temp.path();
        let mut stage = load_stage("test-stage", work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);

        stage.try_force_complete_review().unwrap();

        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.completed_at.is_some());
    }

    #[test]
    fn test_human_review_reject() {
        let temp = TempDir::new().unwrap();
        setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));

        let work_dir = temp.path();
        let mut stage = load_stage("test-stage", work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);

        stage
            .try_reject_review("Not needed anymore".to_string())
            .unwrap();

        assert_eq!(stage.status, StageStatus::Blocked);
        assert_eq!(stage.review_reason, Some("Not needed anymore".to_string()));
    }

    #[test]
    fn test_human_review_wrong_state() {
        // Queued -> Executing is valid via try_approve_review's inner transition,
        // but the command-level check for NeedsHumanReview status should catch it.
        // Test the transition method directly from a truly invalid state.
        let mut stage = Stage {
            status: StageStatus::Completed,
            ..Default::default()
        };
        let result = stage.try_approve_review();
        assert!(result.is_err());
    }
}
