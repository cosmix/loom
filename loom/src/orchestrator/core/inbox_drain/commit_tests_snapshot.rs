use super::*;

#[test]
fn legacy_merge_session_preserves_its_in_progress_merge() {
    let fx = fixture();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    std::fs::remove_file(fx.work_dir.join("merge-target-snapshots").join(&record.id)).unwrap();
    stage_in(&fx, StageStatus::MergeConflict, None);
    let before_head = tip(&worktree, BRANCH);
    let before_merge = tip(&worktree, "MERGE_HEAD");
    let payload = staged_payload(&worktree);
    let expected_index = payload["expected_tree"].clone();
    let result = relay_commit(&fx, &mut fx.host(true), &record, payload);
    assert!(reason_of(&result, LedgerOutcome::Refused).contains("snapshot unavailable"));
    assert_eq!(tip(&worktree, BRANCH), before_head);
    assert_eq!(tip(&worktree, "MERGE_HEAD"), before_merge);
    assert_eq!(
        git_in(&worktree, &["write-tree"]),
        expected_index.as_str().unwrap()
    );
}

#[test]
fn in_progress_merge_must_match_the_recorded_snapshot() {
    let fx = fixture();
    let worktree = merging_repository(&fx);
    let record = merge_session(&fx);
    let path = fx.work_dir.join("merge-target-snapshots").join(&record.id);
    std::fs::write(path, tip(&fx.repo_root, "main~1")).unwrap();
    stage_in(&fx, StageStatus::MergeConflict, None);
    let before_head = tip(&worktree, BRANCH);
    let before_merge = tip(&worktree, "MERGE_HEAD");
    let payload = staged_payload(&worktree);
    let expected_index = payload["expected_tree"].clone();
    let result = relay_commit(&fx, &mut fx.host(true), &record, payload);
    assert!(reason_of(&result, LedgerOutcome::Refused).contains("authorized target snapshot"));
    assert_eq!(tip(&worktree, BRANCH), before_head);
    assert_eq!(tip(&worktree, "MERGE_HEAD"), before_merge);
    assert_eq!(
        git_in(&worktree, &["write-tree"]),
        expected_index.as_str().unwrap()
    );
}
