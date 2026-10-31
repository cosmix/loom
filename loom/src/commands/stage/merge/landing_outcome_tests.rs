use super::*;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

fn signing_error() -> anyhow::Error {
    crate::git::signing::CommitTreeError {
        signing: true,
        detail: "key unavailable".into(),
    }
    .into()
}

#[test]
fn manual_signing_failure_is_durably_held_for_review() {
    let dir = tempfile::tempdir().unwrap();
    let stage = Stage {
        id: "s".into(),
        status: StageStatus::MergeConflict,
        completed_commit: Some("old".into()),
        ..Stage::default()
    };
    save_stage(&stage, dir.path()).unwrap();
    persist_merge_error(&stage, dir.path(), &signing_error()).unwrap();
    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::NeedsHumanReview);
    assert!(saved.review_reason.unwrap().contains("key unavailable"));
}

#[test]
fn manual_failure_refuses_changed_completion_proof() {
    let dir = tempfile::tempdir().unwrap();
    let stage = Stage {
        id: "s".into(),
        status: StageStatus::MergeConflict,
        completed_commit: Some("old".into()),
        ..Stage::default()
    };
    save_stage(&stage, dir.path()).unwrap();
    update_stage("s", dir.path(), |current| {
        current.completed_commit = Some("new".into());
        Ok(())
    })
    .unwrap();
    assert!(persist_merge_error(&stage, dir.path(), &signing_error()).is_err());
    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::MergeConflict);
    assert_eq!(saved.completed_commit.as_deref(), Some("new"));
}

#[test]
fn manual_retry_propagates_git_failure_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let stage = Stage {
        id: "s".into(),
        status: StageStatus::MergeConflict,
        completed_commit: Some("old".into()),
        ..Stage::default()
    };
    save_stage(&stage, dir.path()).unwrap();
    assert!(attempt_retried_merge(&stage, dir.path(), dir.path(), "main").is_err());
    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::MergeBlocked);
    assert!(saved.failure_info.is_some());
}

#[test]
fn manual_retry_signing_failure_returns_error_and_routes_to_review() {
    let (repo, work, commit) = crate::stage_service::merge_test_fixtures::failing_signer_repo();
    let root = repo.path();
    let stage = Stage {
        id: "s".into(),
        status: StageStatus::MergeBlocked,
        completed_commit: Some(commit),
        ..Stage::default()
    };
    save_stage(&stage, &work).unwrap();
    assert!(attempt_retried_merge(&stage, &work, root, "main").is_err());
    let saved = load_stage("s", &work).unwrap();
    assert_eq!(saved.status, StageStatus::NeedsHumanReview);
    assert!(saved.review_reason.unwrap().contains("fake signer refused"));
}
