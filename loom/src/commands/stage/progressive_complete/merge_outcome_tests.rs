use super::*;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

fn executing() -> Stage {
    Stage {
        id: "s".into(),
        status: StageStatus::Executing,
        completed_commit: Some("old".into()),
        ..Stage::default()
    }
}

#[test]
fn progressive_signing_failure_is_durably_held_for_review() {
    let dir = tempfile::tempdir().unwrap();
    let mut stage = executing();
    save_stage(&stage, dir.path()).unwrap();
    let claim = Claim::new(&stage, Landing::Session);
    let at = ConflictTarget {
        merge_point: "main",
        repo_root: dir.path(),
        work_dir: dir.path(),
    };
    let error = crate::git::signing::CommitTreeError {
        signing: true,
        detail: "key unavailable".into(),
    };
    assert!(matches!(
        record_merge_result(&mut stage, Err(error.into()), &claim, &at).unwrap(),
        MergeOutcome::SigningHeld
    ));
    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::NeedsHumanReview);
    assert!(saved.review_reason.unwrap().contains("key unavailable"));
}

#[test]
fn progressive_failure_refuses_changed_completion_proof() {
    let dir = tempfile::tempdir().unwrap();
    let mut stage = executing();
    save_stage(&stage, dir.path()).unwrap();
    let claim = Claim::new(&stage, Landing::Session);
    update_stage("s", dir.path(), |current| {
        current.completed_commit = Some("new".into());
        Ok(())
    })
    .unwrap();
    let at = ConflictTarget {
        merge_point: "main",
        repo_root: dir.path(),
        work_dir: dir.path(),
    };
    assert!(record_merge_result(&mut stage, Err(anyhow::anyhow!("failure")), &claim, &at).is_err());
    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::Executing);
    assert_eq!(saved.completed_commit.as_deref(), Some("new"));
}

#[test]
fn progressive_merge_signing_failure_from_git_routes_to_review() {
    let (repo, work, commit) = crate::stage_service::merge_test_fixtures::failing_signer_repo();
    let root = repo.path();
    let mut stage = executing();
    stage.completed_commit = Some(commit);
    save_stage(&stage, &work).unwrap();
    assert!(matches!(
        attempt_progressive_merge(&mut stage, root, &work, Landing::Session).unwrap(),
        MergeOutcome::SigningHeld
    ));
    let saved = load_stage("s", &work).unwrap();
    assert_eq!(saved.status, StageStatus::NeedsHumanReview);
    assert!(saved.review_reason.unwrap().contains("fake signer refused"));
}

#[test]
fn signing_hold_reports_the_signing_remedy_to_the_session() {
    let (repo, work, commit) = crate::stage_service::merge_test_fixtures::failing_signer_repo();
    let mut stage = executing();
    stage.completed_commit = Some(commit);
    save_stage(&stage, &work).unwrap();
    let error = complete_with_merge(&mut stage, repo.path(), &work, Landing::Session).unwrap_err();
    assert!(error.to_string().contains("signing failed"));
    assert!(!error.to_string().contains("control path"));
}
