use super::test_support::{init_repo, lock_dir, rev, stage_branch};
use super::{merge_stage, MergeGate};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::save_stage;

#[test]
fn changed_resolver_tip_requires_new_completion_proof_even_when_clean() {
    use super::test_support::{commit_file, git_ok};
    let repo = init_repo();
    let work = lock_dir();
    stage_branch(repo.path(), "api", &[("api.rs", "verified")]);
    let verified = rev(repo.path(), "loom/api");
    git_ok(
        repo.path(),
        &["worktree", "add", ".worktrees/api", "loom/api"],
    );
    let checkout = repo.path().join(".worktrees/api");
    commit_file(&checkout, "api.rs", "resolution", "resolve");
    let stage = Stage {
        id: "api".into(),
        status: StageStatus::Completed,
        completed_commit: Some(verified.clone()),
        ..Default::default()
    };
    save_stage(&stage, work.path()).unwrap();
    let ordinary = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
    assert!(format!("{:#}", ordinary.unwrap_err()).contains("changed after verification"));
    crate::verify::transitions::update_stage("api", work.path(), |current| {
        current.status = StageStatus::MergeConflict;
        Ok(())
    })
    .unwrap();
    std::fs::write(checkout.join("api.rs"), "dirty").unwrap();
    let dirty = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
    assert!(format!("{:#}", dirty.unwrap_err()).contains("uncommitted changes"));
    std::fs::write(checkout.join("api.rs"), "resolution").unwrap();
    let main_before = rev(repo.path(), "main");
    let resolved = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
    assert!(format!("{:#}", resolved.unwrap_err()).contains("changed after verification"));
    assert_eq!(rev(repo.path(), "main"), main_before);
    let stage = crate::verify::transitions::load_stage("api", work.path()).unwrap();
    assert_eq!(stage.completed_commit.as_deref(), Some(verified.as_str()));
}

#[test]
fn merge_refuses_branch_tip_that_moved_after_verification() {
    let repo = init_repo();
    let work = lock_dir();
    let verified = rev(repo.path(), "main");
    stage_branch(repo.path(), "api", &[("api.rs", "unverified")]);
    std::fs::create_dir(work.path().join("stages")).unwrap();
    let stage = Stage {
        id: "api".into(),
        completed_commit: Some(verified.clone()),
        ..Default::default()
    };
    save_stage(&stage, work.path()).unwrap();
    let result = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
    assert!(format!("{:#}", result.unwrap_err()).contains("changed after verification"));
    assert_eq!(rev(repo.path(), "main"), verified);
}

#[test]
fn merge_refuses_missing_stage_in_initialized_store() {
    let repo = init_repo();
    let work = lock_dir();
    let original = rev(repo.path(), "main");
    stage_branch(repo.path(), "api", &[("api.rs", "unverified")]);
    std::fs::create_dir(work.path().join("stages")).unwrap();
    let result = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
    assert!(format!("{:#}", result.unwrap_err()).contains("Stage file not found"));
    assert_eq!(rev(repo.path(), "main"), original);
}

#[test]
fn merge_refuses_every_landing_state_without_verified_commit() {
    for status in [
        StageStatus::Completed,
        StageStatus::MergeBlocked,
        StageStatus::MergeConflict,
        StageStatus::NeedsHumanReview,
    ] {
        let repo = init_repo();
        let work = lock_dir();
        let original = rev(repo.path(), "main");
        stage_branch(repo.path(), "api", &[("api.rs", "unverified")]);
        let stage = Stage {
            id: "api".into(),
            status,
            ..Default::default()
        };
        save_stage(&stage, work.path()).unwrap();
        let result = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce);
        assert!(format!("{:#}", result.unwrap_err()).contains("no verified commit"));
        assert_eq!(rev(repo.path(), "main"), original);
    }
}

#[test]
fn merge_lands_verified_commit_from_numbered_stage_record() {
    let repo = init_repo();
    let work = lock_dir();
    stage_branch(repo.path(), "api", &[("api.rs", "verified")]);
    let verified = rev(repo.path(), "loom/api");
    let stage = Stage {
        id: "api".into(),
        status: StageStatus::Completed,
        completed_commit: Some(verified.clone()),
        ..Default::default()
    };
    save_stage(&stage, work.path()).unwrap();
    let path = crate::fs::stage_files::find_stage_file(&work.path().join("stages"), "api")
        .unwrap()
        .unwrap();
    assert_ne!(path.file_name().unwrap(), "api.md");
    let result = merge_stage("api", "main", repo.path(), work.path(), MergeGate::Enforce).unwrap();
    assert!(matches!(result, super::MergeResult::Success { .. }));
    assert!(crate::git::branch::is_ancestor_of(&verified, "main", repo.path()).unwrap());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("api.rs")).unwrap(),
        "verified"
    );
}
