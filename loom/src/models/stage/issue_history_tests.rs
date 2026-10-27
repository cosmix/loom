use super::*;

fn stage_in(status: StageStatus) -> Stage {
    Stage {
        id: "s".to_string(),
        status,
        ..Stage::default()
    }
}

fn failure(failure_type: FailureType, evidence: &[&str]) -> FailureInfo {
    FailureInfo {
        failure_type,
        detected_at: Utc::now(),
        evidence: evidence.iter().map(|line| line.to_string()).collect(),
    }
}

fn kinds(stage: &Stage) -> Vec<IssueKind> {
    stage.issue_history.iter().map(|issue| issue.kind).collect()
}

fn review_texts(stage: &Stage) -> Vec<String> {
    let reviews = stage.issue_history.iter();
    let reviews = reviews.filter(|issue| issue.kind == IssueKind::Review);
    reviews.map(|issue| issue.evidence.join("\n")).collect()
}

fn first_failure(stage: &Stage) -> &StageIssue {
    let mut failures = stage.issue_history.iter();
    failures
        .find(|issue| issue.kind == IssueKind::Failure)
        .expect("a failure is archived")
}

/// An adjudication spawn failure escalates the stage with `failure_info`; the
/// review is approved, a later dispute is accepted, the stage completes and
/// its merge conflicts. Only the conflict may describe the stage now, and the
/// escalation's failure must survive whole in the history.
#[test]
fn an_escalation_failure_is_archived_when_the_stage_resumes_and_not_shown_at_merge_conflict() {
    let mut stage = stage_in(StageStatus::Executing);
    stage
        .try_request_adjudication(Some("first dispute".to_string()))
        .unwrap();
    stage
        .try_request_human_review("no adjudication session could be started".to_string())
        .unwrap();
    let escalation = failure(
        FailureType::SandboxSetupFailure,
        &["Failed to spawn adjudication session", "tmux spawn failed"],
    );
    stage.failure_info = Some(escalation.clone());

    stage.try_approve_review().unwrap();
    stage.try_mark_executing().unwrap();
    stage
        .try_request_adjudication(Some("second dispute".to_string()))
        .unwrap();
    stage.try_transition(StageStatus::Queued).unwrap();
    stage.try_mark_executing().unwrap();
    stage.try_complete(None).unwrap();
    stage.enter_merge_conflict();

    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(stage.merge_conflict);
    assert_eq!(stage.failure_info, None);
    assert_eq!(stage.review_reason, None);
    let archived = first_failure(&stage);
    assert_eq!(
        archived.failure_type,
        Some(FailureType::SandboxSetupFailure)
    );
    assert_eq!(archived.evidence, escalation.evidence);
    assert_eq!(archived.detected_at, Some(escalation.detected_at));
    assert_eq!(archived.ended_by, "NeedsHumanReview -> Queued");
    assert_eq!(
        review_texts(&stage),
        [
            "first dispute",
            "no adjudication session could be started",
            "second dispute"
        ]
    );
}

#[test]
fn entering_merge_conflict_archives_a_failure_an_earlier_status_left() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.failure_info = Some(failure(FailureType::InfrastructureError, &["git lock"]));
    stage.enter_merge_conflict();
    assert_eq!(stage.failure_info, None);
    assert_eq!(kinds(&stage), [IssueKind::Failure]);
    assert_eq!(
        stage.issue_history[0].ended_by,
        "Completed -> MergeConflict"
    );
}

#[test]
fn leaving_merge_conflict_archives_the_conflict() {
    let mut stage = stage_in(StageStatus::MergeConflict);
    stage.merge_conflict = true;
    stage.try_complete_merge().unwrap();
    assert_eq!(kinds(&stage), [IssueKind::MergeConflict]);
    assert_eq!(
        stage.issue_history[0].ended_by,
        "MergeConflict -> Completed"
    );
}

#[test]
fn a_failure_recorded_on_entering_blocked_stays_current_until_the_retry() {
    let mut stage = stage_in(StageStatus::Executing);
    stage.try_mark_blocked().unwrap();
    stage.failure_info = Some(failure(FailureType::SessionCrash, &["exit 1"]));
    stage.try_transition(StageStatus::Blocked).unwrap();
    assert!(
        stage.failure_info.is_some(),
        "a self transition ends nothing"
    );
    assert!(stage.issue_history.is_empty());

    stage.try_mark_queued().unwrap();
    assert_eq!(stage.failure_info, None);
    assert_eq!(stage.issue_history[0].ended_by, "Blocked -> Queued");
}

#[test]
fn a_rejected_review_keeps_its_reason_and_archives_the_one_it_replaced() {
    let mut stage = stage_in(StageStatus::Executing);
    stage
        .try_request_human_review("needs a look".to_string())
        .unwrap();
    stage
        .try_reject_review("wrong approach".to_string())
        .unwrap();
    assert_eq!(stage.review_reason.as_deref(), Some("wrong approach"));
    assert_eq!(kinds(&stage), [IssueKind::Review]);
    assert_eq!(stage.issue_history[0].evidence, ["needs a look"]);
}

#[test]
fn replacing_a_review_reason_with_the_same_text_archives_nothing() {
    let mut stage = stage_in(StageStatus::NeedsHumanReview);
    stage.replace_review_reason("same".to_string());
    stage.replace_review_reason("same".to_string());
    assert!(stage.issue_history.is_empty());
}

#[test]
fn a_cleared_merge_block_moves_its_failure_into_history_labelled_by_the_transition() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(crate::git::MergeBlock::TargetMoved);
    stage.try_mark_queued().unwrap();
    assert_eq!(stage.failure_info, None);
    assert_eq!(stage.merge.block, None);
    assert_eq!(kinds(&stage), [IssueKind::Failure]);
    assert_eq!(stage.issue_history[0].ended_by, "MergeBlocked -> Queued");
    assert_eq!(
        stage.issue_history[0].evidence,
        [crate::git::MergeBlock::TargetMoved.to_string()]
    );
}

/// The daemon re-records the block every tick: the same block must keep its
/// failure as first recorded, and only a different one archives it.
#[test]
fn a_changed_merge_block_archives_the_old_failure_once_and_a_repeat_archives_nothing() {
    use crate::git::MergeBlock;

    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    let first = stage.failure_info.clone().unwrap();
    stage.block_merge(MergeBlock::TargetMoved);
    assert!(stage.issue_history.is_empty());
    assert_eq!(stage.failure_info.as_ref(), Some(&first));

    let overlap = MergeBlock::UncommittedOverlap {
        paths: vec!["src/lib.rs".to_string()],
    };
    stage.block_merge(overlap.clone());
    stage.block_merge(overlap);
    assert_eq!(kinds(&stage), [IssueKind::Failure]);
    assert_eq!(stage.issue_history[0].evidence, first.evidence);
    assert_eq!(
        stage.issue_history[0].ended_by,
        "replaced by a newer failure"
    );
    assert_ne!(stage.failure_info.as_ref(), Some(&first));
}

#[test]
fn a_merge_conflict_is_archived_with_when_it_was_entered() {
    let mut stage = stage_in(StageStatus::Completed);
    let before = Utc::now();
    stage.enter_merge_conflict();
    let entered = stage.merge.conflict_since.expect("entering stamps it");
    assert!(entered >= before);
    stage.enter_merge_conflict();
    assert_eq!(
        stage.merge.conflict_since,
        Some(entered),
        "a re-entry keeps it"
    );

    stage.try_complete_merge().unwrap();
    assert_eq!(stage.merge.conflict_since, None);
    assert_eq!(kinds(&stage), [IssueKind::MergeConflict]);
    assert_eq!(stage.issue_history[0].detected_at, Some(entered));
}

#[test]
fn a_reset_to_waiting_for_deps_ends_the_review_reason_and_the_conflict() {
    let mut stage = stage_in(StageStatus::MergeConflict);
    stage.review_reason = Some("resolver gave up".to_string());
    stage.force_status_with_reason(StageStatus::WaitingForDeps, "loom stage reset");
    assert_eq!(stage.review_reason, None);
    assert_eq!(kinds(&stage), [IssueKind::MergeConflict, IssueKind::Review]);
    assert!(stage
        .issue_history
        .iter()
        .all(|issue| issue.ended_by == "MergeConflict -> WaitingForDeps"));
}

#[test]
fn the_history_round_trips_through_the_stage_file() {
    let mut stage = stage_in(StageStatus::Blocked);
    stage.failure_info = Some(failure(FailureType::Timeout, &["slow"]));
    stage.try_mark_queued().unwrap();
    let text = crate::yaml::to_string(&stage).unwrap();
    let back: Stage = crate::yaml::from_str(&text).unwrap();
    assert_eq!(back.issue_history, stage.issue_history);
}

#[test]
fn reset_to_waiting_for_deps_archives_what_the_left_status_recorded() {
    let mut stage = stage_in(StageStatus::NeedsHumanReview);
    stage.review_reason = Some("zero commits".to_string());
    stage.failure_info = Some(failure(FailureType::SessionCrash, &["pane died"]));

    stage.reset_to_waiting_for_deps();

    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.review_reason, None);
    assert_eq!(stage.failure_info, None);
    assert_eq!(
        first_failure(&stage).ended_by,
        "NeedsHumanReview -> WaitingForDeps"
    );
    assert_eq!(review_texts(&stage), ["zero commits"]);
}

#[test]
fn nothing_but_reset_enters_waiting_for_deps() {
    let mut stage = stage_in(StageStatus::Queued);
    assert!(stage.try_transition(StageStatus::WaitingForDeps).is_err());
    assert_eq!(stage.status, StageStatus::Queued);
}
