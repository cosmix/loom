//! Computes the execution time the status surfaces display.

use chrono::{DateTime, Utc};

use crate::models::stage::{Stage, StageStatus};

/// Execution time to display: the seconds banked by finished attempts plus the
/// attempt still in flight. `Stage::execution_secs` alone is frozen for the
/// whole of an attempt - `accumulate_attempt_time` banks it only when the
/// attempt ends - so an executing stage would otherwise read `0s` throughout
/// its first attempt.
pub(super) fn execution_secs_live(stage: &Stage, now: DateTime<Utc>) -> Option<i64> {
    if stage.status != StageStatus::Executing {
        return stage.execution_secs;
    }
    let start = stage.attempt_started_at.or(stage.started_at)?;
    let in_flight = now.signed_duration_since(start).num_seconds().max(0);
    Some(stage.execution_secs.unwrap_or(0).saturating_add(in_flight))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn stage_with_status(status: StageStatus) -> Stage {
        let mut stage = Stage::new("timing".to_string(), None);
        stage.status = status;
        stage
    }

    #[test]
    fn executing_stage_with_frozen_zero_counts_the_in_flight_attempt() {
        let now = Utc::now();
        let mut stage = stage_with_status(StageStatus::Executing);
        stage.execution_secs = Some(0);
        stage.attempt_started_at = Some(now - Duration::seconds(90));

        assert_eq!(execution_secs_live(&stage, now), Some(90));
    }

    #[test]
    fn executing_stage_adds_in_flight_attempt_to_banked_prior_attempts() {
        let now = Utc::now();
        let mut stage = stage_with_status(StageStatus::Executing);
        stage.execution_secs = Some(100);
        stage.attempt_started_at = Some(now - Duration::seconds(50));

        assert_eq!(execution_secs_live(&stage, now), Some(150));
    }

    #[test]
    fn executing_stage_without_attempt_started_at_falls_back_to_started_at() {
        let now = Utc::now();
        let mut stage = stage_with_status(StageStatus::Executing);
        stage.attempt_started_at = None;
        stage.started_at = Some(now - Duration::seconds(30));

        assert_eq!(execution_secs_live(&stage, now), Some(30));
    }

    #[test]
    fn non_executing_stage_shows_banked_total_and_never_ticks() {
        let now = Utc::now();
        let mut stage = stage_with_status(StageStatus::Completed);
        stage.execution_secs = Some(42);
        stage.attempt_started_at = Some(now - Duration::hours(1));

        assert_eq!(execution_secs_live(&stage, now), Some(42));
    }
}
