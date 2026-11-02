//! Terminal outcomes, completion summaries and their protocol frames.

use super::*;
use crate::daemon::protocol::Response;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::serialize_stage_to_markdown;

fn write_stage_file(stages_dir: &Path, stage: &Stage) {
    let content = serialize_stage_to_markdown(stage).unwrap();
    std::fs::write(stages_dir.join(format!("{}.md", stage.id)), content).unwrap();
}

#[test]
fn completion_durations_require_real_starts() {
    let temp = tempfile::tempdir().unwrap();
    let stages_dir = temp.path().join("stages");
    std::fs::create_dir_all(&stages_dir).unwrap();
    let time = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);

    let mut normal = Stage::new("Normal".to_string(), None);
    normal.id = "normal".to_string();
    normal.status = StageStatus::Completed;
    normal.started_at = Some(time("2026-01-01T00:00:00Z"));
    normal.completed_at = Some(time("2026-01-01T00:01:00Z"));
    write_stage_file(&stages_dir, &normal);

    let mut skipped = Stage::new("Skipped".to_string(), None);
    skipped.id = "skipped".to_string();
    skipped.status = StageStatus::Skipped;
    skipped.completed_at = Some(time("2026-01-01T00:02:00Z"));
    write_stage_file(&stages_dir, &skipped);

    let summary = collect_completion_summary(temp.path()).unwrap();
    assert_eq!(summary.stages[0].duration_secs, Some(60));
    assert_eq!(summary.stages[1].duration_secs, None);
    assert_eq!(summary.total_duration_secs, 120);

    std::fs::remove_file(stages_dir.join("normal.md")).unwrap();
    let summary = collect_completion_summary(temp.path()).unwrap();
    assert_eq!(summary.stages[0].duration_secs, None);
    assert_eq!(summary.total_duration_secs, 0);
}

#[test]
fn failed_outcomes_and_unreadable_or_inflight_stages_are_not_success() {
    let temp = tempfile::tempdir().unwrap();
    let stages_dir = temp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();
    let mut stage = Stage::new("Running".into(), None);
    stage.id = "running".into();
    stage.status = StageStatus::Executing;
    write_stage_file(&stages_dir, &stage);
    fs::write(stages_dir.join("bad.md"), "invalid stage").unwrap();
    for outcome in [
        TerminalOutcome::SchedulerFailure("panic".into()),
        TerminalOutcome::FinalizationFailure("archive".into()),
        TerminalOutcome::Success,
    ] {
        fs::write(
            temp.path().join("orchestrator.complete"),
            serde_json::to_string(&outcome).unwrap(),
        )
        .unwrap();
        let (summary, details) = collect_completion_with_details(temp.path()).unwrap();
        assert_eq!(summary.failure_count, 2);
        assert_eq!(
            details.len(),
            if outcome == TerminalOutcome::Success {
                1
            } else {
                2
            }
        );
    }
    fs::remove_file(stages_dir.join("bad.md")).unwrap();
    stage.status = StageStatus::Completed;
    write_stage_file(&stages_dir, &stage);
    for outcome in [
        TerminalOutcome::SchedulerFailure("panic".into()),
        TerminalOutcome::FinalizationFailure("archive".into()),
    ] {
        fs::write(
            temp.path().join("orchestrator.complete"),
            serde_json::to_string(&outcome).unwrap(),
        )
        .unwrap();
        let (summary, details) = collect_completion_with_details(temp.path()).unwrap();
        assert_eq!(summary.failure_count, 1);
        assert_eq!(details.len(), 1);
    }
}

#[test]
fn missing_and_unparseable_outcomes_do_not_imply_success() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("stages")).unwrap();
    let summary = collect_completion_summary(temp.path()).unwrap();
    assert_eq!(summary.failure_count, 1);
    fs::write(
        temp.path().join("orchestrator.complete"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let summary = collect_completion_summary(temp.path()).unwrap();
    assert_eq!(summary.failure_count, 1);
}

#[test]
fn a_new_run_forgets_the_previous_outcome_and_tolerates_none() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(COMPLETION_MARKER), "{}").unwrap();
    fs::write(temp.path().join(SCHEDULER_FAILURE_FILE), "panic").unwrap();
    remove_previous_outcome(temp.path()).unwrap();
    assert!(!temp.path().join(COMPLETION_MARKER).exists());
    assert!(!temp.path().join(SCHEDULER_FAILURE_FILE).exists());
    assert_eq!(completion_diagnostics(temp.path()), None);
    remove_previous_outcome(temp.path()).unwrap();
}

#[test]
fn completion_frames_precede_the_summary_with_failure_detail() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("stages")).unwrap();
    fs::write(
        temp.path().join(COMPLETION_MARKER),
        serde_json::to_string(&TerminalOutcome::Success).unwrap(),
    )
    .unwrap();
    let frames = completion_frames(temp.path()).unwrap();
    assert!(matches!(
        frames.as_slice(),
        [Response::OrchestrationComplete { summary }] if summary.failure_count == 0
    ));
    fs::write(
        temp.path().join(COMPLETION_MARKER),
        serde_json::to_string(&TerminalOutcome::FinalizationFailure("archive".into())).unwrap(),
    )
    .unwrap();
    let frames = completion_frames(temp.path()).unwrap();
    assert!(matches!(
        frames.as_slice(),
        [Response::Error { message }, Response::OrchestrationComplete { summary }]
            if message.contains("Finalization failure: archive") && summary.failure_count == 1
    ));
}

#[test]
fn collect_status_returns_status_data() {
    let temp = tempfile::tempdir().unwrap();
    let wd = crate::fs::work_dir::WorkDir::new(temp.path().join(".loom/work")).unwrap();
    let stages_dir = wd.root().join("stages");
    std::fs::create_dir_all(stages_dir).unwrap();

    let mut stage = Stage::new("Test Waiting".to_string(), None);
    stage.id = "test-waiting".to_string();
    stage.status = StageStatus::WaitingForInput;
    write_stage_file(&wd.root().join("stages"), &stage);

    let response = collect_status(wd.root()).unwrap();
    match response {
        Response::StatusUpdate { data } => {
            let stage = data
                .stages
                .iter()
                .find(|row| row.id == "test-waiting")
                .unwrap();
            assert_eq!(stage.status, StageStatus::WaitingForInput);
        }
        _ => panic!("Expected StatusUpdate response"),
    }
}
