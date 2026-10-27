//! A stage's history on the status payload, and the attention entry beside it
//! describing only the current status.

use super::tests::{make_test_stage, temp_work_dir};
use super::*;
use crate::commands::status::render::attention_entries;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::IssueKind;

const STAGE: &str = "integration-verify";

/// Longer than any inline cap, so a cut anywhere shows.
fn long_reason() -> String {
    format!(
        "Building is the specified partial state. {}",
        "x".repeat(400)
    )
}

/// The escalation's evidence: the spawn error, then its causes.
fn escalation_evidence() -> Vec<String> {
    vec![
        "No adjudication session could be started for this dispute".to_string(),
        "Failed to spawn adjudication session: tmux spawn failed".to_string(),
        "loom hook scripts differ from the installed ones".to_string(),
    ]
}

/// Escalated with a sandbox-setup failure when its adjudication spawn failed,
/// approved, disputed again and accepted, completed, then conflicted on merge.
fn escalated_then_conflicted() -> Stage {
    let mut stage = make_test_stage(STAGE, StageStatus::Executing);
    stage
        .try_request_adjudication(Some("first dispute".to_string()))
        .unwrap();
    stage
        .try_request_human_review(escalation_evidence()[0].clone())
        .unwrap();
    stage.failure_info = Some(FailureInfo {
        failure_type: FailureType::SandboxSetupFailure,
        detected_at: Utc::now() - chrono::Duration::minutes(30),
        evidence: escalation_evidence(),
    });
    stage.try_approve_review().unwrap();
    stage.try_mark_executing().unwrap();
    stage.try_request_adjudication(Some(long_reason())).unwrap();
    stage.try_transition(StageStatus::Queued).unwrap();
    stage.try_mark_executing().unwrap();
    stage.try_complete(None).unwrap();
    stage.enter_merge_conflict();
    stage
}

/// Dispute 1 closed by the escalation (its marker written now); dispute 2
/// accepted by a verdict filed long before.
fn write_disputes(work_dir: &WorkDir) {
    let stage_dir = work_dir.disputes_dir().join(STAGE);
    let closed = stage_dir.join("1");
    fs::create_dir_all(&closed).unwrap();
    fs::write(
        closed.join("request.md"),
        "---\nid: 1\nkind: findings\nreason: first dispute\n\
         created_at: \"2020-01-01T00:00:00Z\"\n---\n",
    )
    .unwrap();
    fs::write(closed.join("closed.marker"), "").unwrap();
    let ruled = stage_dir.join("2");
    fs::create_dir_all(&ruled).unwrap();
    fs::write(
        ruled.join("request.md"),
        format!(
            "---\nid: 2\nkind: findings\nreason: \"{}\"\n\
             created_at: \"2020-01-02T00:00:00Z\"\n---\n",
            long_reason()
        ),
    )
    .unwrap();
    fs::write(
        ruled.join("verdict.md"),
        "---\nid: 2\nverdict:\n  verdict: accept\n  reasoning: the plan pins it\n\
         created_at: \"2020-01-02T00:05:00Z\"\n---\n",
    )
    .unwrap();
    fs::write(ruled.join("applied.marker"), "").unwrap();
}

#[test]
fn a_merge_conflict_after_an_escalation_shows_the_conflict_and_keeps_the_failure_in_history() {
    let (_tmp, work_dir) = temp_work_dir();
    write_disputes(&work_dir);
    let summary = build_stage_summary(&escalated_then_conflicted(), &[], &work_dir);

    assert_eq!(summary.failure_info, None);
    assert_eq!(summary.review_notes, None);
    let attention = attention_entries(std::slice::from_ref(&summary));
    assert_eq!(attention.len(), 1);
    assert_eq!(attention[0].label, "MERGE CONFLICT");
    assert_eq!(attention[0].failure_type, None);
    assert!(attention[0].evidence.is_empty());

    let failure = summary
        .history
        .iter()
        .find(|issue| issue.kind == IssueKind::Failure)
        .expect("the escalation's failure is in the history");
    assert_eq!(failure.failure_type, Some(FailureType::SandboxSetupFailure));
    assert_eq!(failure.evidence, escalation_evidence());
    assert_eq!(failure.ended_by, "NeedsHumanReview -> Queued");
}

#[test]
fn the_history_is_newest_first_and_carries_every_ended_adjudication_round_whole() {
    let (_tmp, work_dir) = temp_work_dir();
    write_disputes(&work_dir);
    let summary = build_stage_summary(&escalated_then_conflicted(), &[], &work_dir);

    let ended: Vec<_> = summary.history.iter().map(|issue| issue.ended_at).collect();
    assert!(ended.windows(2).all(|pair| pair[0] >= pair[1]), "{ended:?}");
    let rounds: Vec<_> = summary
        .history
        .iter()
        .filter(|issue| issue.kind == IssueKind::Adjudication)
        .collect();
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].dispute_id, Some(1));
    assert_eq!(rounds[0].ended_by, "closed without a verdict");
    assert_eq!(rounds[1].dispute_id, Some(2));
    assert_eq!(rounds[1].ended_by, "adjudicator verdict: accept");
    assert_eq!(
        rounds[1].evidence,
        [
            "findings dispute 2".to_string(),
            long_reason(),
            "the plan pins it".to_string()
        ]
    );
    assert_eq!(
        rounds[1].detected_at.map(|at| at.to_rfc3339()),
        Some("2020-01-02T00:00:00+00:00".to_string())
    );
    // Each dispute's reason was the stage's review reason while it was open;
    // its round stands for it, so only the escalation's reason is listed.
    let reviews: Vec<&[String]> = summary
        .history
        .iter()
        .filter(|issue| issue.kind == IssueKind::Review)
        .map(|issue| issue.evidence.as_slice())
        .collect();
    assert_eq!(reviews, [&escalation_evidence()[..1]]);
}

#[test]
fn an_open_dispute_is_not_history() {
    let (_tmp, work_dir) = temp_work_dir();
    let open = work_dir.disputes_dir().join(STAGE).join("1");
    fs::create_dir_all(&open).unwrap();
    fs::write(
        open.join("request.md"),
        "---\nid: 1\nreason: pending\n---\n",
    )
    .unwrap();
    let stage = make_test_stage(STAGE, StageStatus::NeedsAdjudication);

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert!(summary.history.is_empty(), "{:?}", summary.history);
}
