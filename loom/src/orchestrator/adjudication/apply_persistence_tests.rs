use super::*;
use crate::plan::schema::AcceptanceCriterion;

#[test]
fn cached_verdict_preserves_newer_policy_and_amendment_tally() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let mut cached = Stage {
        id: "a".into(),
        name: "a".into(),
        status: StageStatus::NeedsAdjudication,
        acceptance: vec![AcceptanceCriterion::Simple("old".into())],
        ..Stage::default()
    };
    cached.tally.amendments_applied = 1;
    let mut fresh = cached.clone();
    fresh.acceptance = vec![AcceptanceCriterion::Simple("new".into())];
    fresh.tally.amendments_applied = 2;
    crate::verify::transitions::save_stage(&fresh, work).unwrap();
    let marker = work.join("applied.marker");
    persist_verdict_result(work, "a", 1, &cached, None, cached.updated_at, &marker).unwrap();
    let persisted = load_stage("a", work).unwrap();
    assert_eq!(persisted.acceptance, fresh.acceptance);
    assert_eq!(persisted.tally.amendments_applied, 2);
    assert!(marker.exists());
}

/// A sibling dispute filed after the verdict's copy was loaded wrote a
/// newer reason; a verdict that left the reason alone must keep it and
/// archive nothing.
#[test]
fn a_verdict_keeps_a_review_reason_written_after_it_loaded_the_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let cached = Stage {
        id: "a".into(),
        status: StageStatus::NeedsAdjudication,
        review_reason: Some("first dispute".into()),
        ..Stage::default()
    };
    let fresh = Stage {
        review_reason: Some("second dispute".into()),
        ..cached.clone()
    };
    crate::verify::transitions::save_stage(&fresh, work).unwrap();
    let marker = work.join("applied.marker");

    persist_verdict_result(
        work,
        "a",
        1,
        &cached,
        Some("first dispute"),
        cached.updated_at,
        &marker,
    )
    .unwrap();

    let persisted = load_stage("a", work).unwrap();
    assert_eq!(persisted.review_reason.as_deref(), Some("second dispute"));
    assert!(
        persisted.issue_history.is_empty(),
        "{:?}",
        persisted.issue_history
    );
}

/// A verdict that set its own reason replaces the newer one on disk, and
/// the replaced one is archived once, as replaced.
#[test]
fn a_verdicts_own_reason_replaces_a_newer_one_and_archives_it() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let cached = Stage {
        id: "a".into(),
        status: StageStatus::NeedsHumanReview,
        review_reason: Some("upheld".into()),
        ..Stage::default()
    };
    let fresh = Stage {
        status: StageStatus::NeedsAdjudication,
        review_reason: Some("second dispute".into()),
        ..cached.clone()
    };
    crate::verify::transitions::save_stage(&fresh, work).unwrap();
    std::fs::create_dir_all(work.join("disputes/a/1")).unwrap();
    std::fs::create_dir_all(work.join("disputes/a/2")).unwrap();
    let marker = work.join("applied.marker");

    persist_verdict_result(
        work,
        "a",
        1,
        &cached,
        Some("first dispute"),
        cached.updated_at,
        &marker,
    )
    .unwrap();

    let persisted = load_stage("a", work).unwrap();
    assert_eq!(persisted.status, StageStatus::NeedsHumanReview);
    assert_eq!(persisted.review_reason.as_deref(), Some("upheld"));
    let archived: Vec<(&[String], &str)> = persisted
        .issue_history
        .iter()
        .map(|issue| (issue.evidence.as_slice(), issue.ended_by.as_str()))
        .collect();
    assert_eq!(
        archived,
        [(
            &["second dispute".to_string()][..],
            "replaced by a newer review reason"
        )]
    );
}

/// A verdict that cleared the reason it loaded, without moving the stage to a
/// status that supersedes it, retires the reason still on disk.
#[test]
fn a_verdict_that_cleared_its_loaded_reason_retires_it_from_disk() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let cached = Stage {
        id: "a".into(),
        status: StageStatus::NeedsAdjudication,
        review_reason: None,
        ..Stage::default()
    };
    let fresh = Stage {
        review_reason: Some("first dispute".into()),
        ..cached.clone()
    };
    crate::verify::transitions::save_stage(&fresh, work).unwrap();
    let marker = work.join("applied.marker");

    persist_verdict_result(
        work,
        "a",
        1,
        &cached,
        Some("first dispute"),
        cached.updated_at,
        &marker,
    )
    .unwrap();

    let persisted = load_stage("a", work).unwrap();
    assert_eq!(persisted.review_reason, None);
    assert_eq!(persisted.issue_history.len(), 1);
    assert_eq!(persisted.issue_history[0].evidence, ["first dispute"]);
    assert_eq!(
        persisted.issue_history[0].ended_by,
        "adjudicator verdict applied"
    );
}

#[test]
fn a_reset_after_prepare_refuses_the_old_verdict_commit() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let cached = Stage {
        id: "a".into(),
        status: StageStatus::NeedsAdjudication,
        ..Stage::default()
    };
    let mut reset = cached.clone();
    reset.status = StageStatus::WaitingForDeps;
    reset.updated_at += chrono::Duration::seconds(1);
    crate::verify::transitions::save_stage(&reset, work).unwrap();
    let marker = work.join("applied.marker");

    let error = persist_verdict_result(work, "a", 1, &cached, None, cached.updated_at, &marker)
        .unwrap_err();
    assert!(format!("{error:#}").contains("lifecycle changed"));
    let actual = load_stage("a", work).unwrap();
    assert_eq!(actual.status, StageStatus::WaitingForDeps);
    assert!(actual.tally.request_receipts.is_empty());
    assert!(!marker.exists());
}
