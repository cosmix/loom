//! Dispute acceptance criteria for a stage
//!
//! Allows an agent to flag acceptance criteria as incorrect,
//! transitioning the stage to NeedsHumanReview for human judgment.

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};

/// Dispute acceptance criteria for a stage, requesting human review.
///
/// The stage must be in Executing or CompletedWithFailures state.
/// Transitions the stage to NeedsHumanReview with the given reason.
pub fn dispute_criteria(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    match stage.status {
        StageStatus::Executing => {
            stage.try_request_human_review(reason.clone())?;
        }
        StageStatus::CompletedWithFailures => {
            // CompletedWithFailures -> Executing -> NeedsHumanReview
            stage.try_mark_executing()?;
            stage.try_request_human_review(reason.clone())?;
        }
        _ => {
            bail!(
                "Stage '{}' is in '{}' state. dispute-criteria requires Executing or CompletedWithFailures.",
                stage_id,
                stage.status
            );
        }
    }

    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' flagged for human review.");
    println!("Reason: {reason}");
    println!();
    println!("The stage is now awaiting human review.");
    println!("A human should run one of:");
    println!("  loom stage human-review {stage_id} --approve         Resume execution");
    println!("  loom stage human-review {stage_id} --force-complete  Mark as completed");
    println!("  loom stage human-review {stage_id} --reject          Block the stage");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use tempfile::TempDir;

    fn setup_stage(temp: &TempDir, status: StageStatus) -> Stage {
        let stages_dir = temp.path().join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        let stage = Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            status,
            ..Default::default()
        };

        let work_dir = temp.path();
        crate::verify::transitions::save_stage(&stage, work_dir).unwrap();
        stage
    }

    #[test]
    fn test_dispute_from_executing() {
        let temp = TempDir::new().unwrap();
        setup_stage(&temp, StageStatus::Executing);

        let work_dir = temp.path();
        let mut stage = load_stage("test-stage", work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::Executing);

        stage
            .try_request_human_review("Bad criteria".to_string())
            .unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);
        assert_eq!(stage.review_reason, Some("Bad criteria".to_string()));
    }

    #[test]
    fn test_dispute_from_completed_with_failures() {
        let temp = TempDir::new().unwrap();
        setup_stage(&temp, StageStatus::CompletedWithFailures);

        let work_dir = temp.path();
        let mut stage = load_stage("test-stage", work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::CompletedWithFailures);

        // Two-step transition: CompletedWithFailures -> Executing -> NeedsHumanReview
        stage.try_mark_executing().unwrap();
        stage
            .try_request_human_review("Criteria are wrong".to_string())
            .unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);
        assert_eq!(stage.review_reason, Some("Criteria are wrong".to_string()));
    }

    #[test]
    fn test_dispute_from_invalid_state() {
        let mut stage = Stage {
            status: StageStatus::Queued,
            ..Default::default()
        };

        let result = stage.try_request_human_review("reason".to_string());
        assert!(result.is_err());
    }
}
