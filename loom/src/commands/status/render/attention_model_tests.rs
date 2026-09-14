use super::*;
use crate::commands::status::data::{
    ActivityStatus, CompletionBlockerState, CompletionBlockerSummary, StageType,
};
use crate::models::failure::FailureType;
use crate::models::stage::StageStatus;

fn make_stage_summary(id: &str, status: StageStatus) -> StageSummary {
    StageSummary {
        id: id.to_string(),
        name: id.to_string(),
        status,
        stage_type: StageType::Standard,
        dependencies: vec![],
        context_tokens: None,
        elapsed_secs: None,
        execution_secs: None,
        base_branch: None,
        base_merged_from: vec![],
        failure_info: None,
        activity_status: ActivityStatus::default(),
        last_tool: None,
        last_activity: None,
        staleness_secs: None,
        context_ceiling_tokens: None,
        review_reason: None,
        merged: false,
        merge_assumed: false,
        cleanup_warning: None,
        held: false,
        retry_count: 0,
        max_retries: None,
        pid: None,
        session_alive: false,
        model: "opus".to_string(),
        session_type: None,
        incoherence: None,
        execution_models: vec![],
        dispute_count: 0,
        judge_heartbeat_secs: None,
        session_backend: None,
        outgoing_session_exit_reason: None,
        completion_blocker: None,
    }
}

fn completion_blocker(state: CompletionBlockerState) -> CompletionBlockerSummary {
    CompletionBlockerSummary {
        state,
        fingerprint: "feedface1234".to_string(),
        failure_code: "sandbox_denied".to_string(),
        summary: Some("sandbox denied execution".to_string()),
        commit: "0123456789ab".to_string(),
        repeat_count: 2,
        first_observed_at: Some("2026-09-14T10:00:00Z".to_string()),
        last_observed_at: Some("2026-09-14T10:01:00Z".to_string()),
        next_action: "fix sandbox access, then retry".to_string(),
    }
}

fn stages_for_all_statuses() -> Vec<StageSummary> {
    let statuses = [
        StageStatus::WaitingForDeps,
        StageStatus::Queued,
        StageStatus::Executing,
        StageStatus::WaitingForInput,
        StageStatus::Blocked,
        StageStatus::Completed,
        StageStatus::NeedsHandoff,
        StageStatus::Skipped,
        StageStatus::MergeConflict,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
        StageStatus::NeedsHumanReview,
        StageStatus::NeedsAdjudication,
    ];
    statuses
        .into_iter()
        .enumerate()
        .map(|(index, status)| make_stage_summary(&format!("stage-{index}"), status))
        .collect()
}

#[test]
fn entries_cover_adjudication_and_input() {
    let mut stages = stages_for_all_statuses();
    stages[12].dispute_count = 2;
    stages[12].judge_heartbeat_secs = Some(30);

    let entries = attention_entries(&stages);
    let details = entries
        .iter()
        .map(|entry| (entry.label, entry.hint.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(
        details,
        vec![
            ("NEEDS INPUT", "loom stage resume stage-3"),
            ("BLOCKED", "loom stage retry stage-4"),
            ("MERGE CONFLICT", "loom stage merge stage-8"),
            ("ACCEPTANCE FAILED", "loom stage retry stage-9"),
            ("MERGE ERROR", "loom stage merge stage-10"),
            ("NEEDS REVIEW", "loom stage human-review stage-11"),
            ("ADJUDICATING", "loom status --verbose"),
        ]
    );
    assert!(entries.iter().any(|entry| entry.label == "NEEDS INPUT"));
    assert!(entries.iter().any(|entry| entry.label == "ADJUDICATING"));
    let adjudication = entries
        .iter()
        .find(|entry| entry.label == "ADJUDICATING")
        .expect("adjudication entry");
    assert_eq!(adjudication.dispute_count, Some(2));
    assert_eq!(adjudication.judge_heartbeat_secs, Some(30));
}

#[test]
fn healthy_statuses_need_no_attention() {
    let stages = [
        StageStatus::Executing,
        StageStatus::Completed,
        StageStatus::Queued,
        StageStatus::WaitingForDeps,
        StageStatus::Skipped,
        StageStatus::NeedsHandoff,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, status)| make_stage_summary(&format!("stage-{index}"), status))
    .collect::<Vec<_>>();

    assert!(attention_entries(&stages).is_empty());
}

#[test]
fn cleanup_warning_wins_over_completed_status() {
    let mut stage = make_stage_summary("cleanup-stage", StageStatus::Completed);
    stage.cleanup_warning = Some("could not remove worktree".to_string());
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::Blocked));

    let entries = attention_entries(&[stage]);

    assert_eq!(entries[0].label, "CLEANUP FAILED");
    assert_eq!(entries[0].hint, "loom worktree remove cleanup-stage");
}

#[test]
fn executing_pending_blocker_requires_attention() {
    let mut stage = make_stage_summary("writer", StageStatus::Executing);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::Pending));

    let entries = attention_entries(&[stage]);

    assert_eq!(entries[0].label, "COMPLETION PENDING");
}

#[test]
fn blocked_completion_outranks_needs_review() {
    let mut stage = make_stage_summary("writer", StageStatus::NeedsHumanReview);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::Blocked));

    let entries = attention_entries(&[stage]);

    assert_eq!(entries[0].label, "COMPLETION BLOCKED");
}

#[test]
fn unknown_writer_ownership_has_distinct_label() {
    let mut stage = make_stage_summary("writer", StageStatus::NeedsHumanReview);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::OwnershipUnknown));

    let entries = attention_entries(&[stage]);

    assert_eq!(entries[0].label, "WRITER UNCONFIRMED");
}

#[test]
fn stage_without_blocker_keeps_existing_attention_label() {
    let stage = make_stage_summary("writer", StageStatus::NeedsHumanReview);

    let entries = attention_entries(&[stage]);

    assert_eq!(entries[0].label, "NEEDS REVIEW");
}

#[test]
fn failure_labels_are_short_and_stable() {
    assert_eq!(failure_label(&FailureType::TestFailure), "test");
    assert_eq!(failure_label(&FailureType::SandboxSetupFailure), "sandbox");
}

#[test]
fn entries_follow_input_order() {
    let stages = vec![
        make_stage_summary("second", StageStatus::MergeConflict),
        make_stage_summary("first", StageStatus::Blocked),
        make_stage_summary("third", StageStatus::WaitingForInput),
    ];

    let ids = attention_entries(&stages)
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert_eq!(ids, ["second", "first", "third"]);
}
