use crate::models::stage::{Stage, StageStatus};

fn create_test_stage(status: StageStatus) -> Stage {
    let mut stage = Stage::new(
        "Test Stage".to_string(),
        Some("Test description".to_string()),
    );
    stage.status = status;
    stage
}

mod can_transition_to;
mod completed_with_failures;
mod merge_blocked;
mod merge_conflict;
mod skipped;
mod try_transition;
mod valid_transitions;
mod workflows;
