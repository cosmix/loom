//! A block naming a quarantine directory is not retried until the operator
//! deletes that directory.

use super::*;

#[test]
fn a_block_with_files_in_quarantine_is_not_retried_until_the_directory_is_gone() {
    let (repo, mut orchestrator) = blocked_stage();
    let root = repo.path();
    // The cause of the block is gone: only the quarantine holds the retry.
    git(root, &["stash", "push", "-q", "--include-untracked"]);
    let quarantine = TempDir::new().unwrap();
    let held = quarantine.path().to_path_buf();
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.block_merge(MergeBlock::CheckoutRestoreIncomplete {
            quarantine: held.clone(),
            failures: vec![],
            detail: None,
        });
        Ok(())
    })
    .unwrap();
    let (objects, stage_file) = (object_counts(root), stage_file_text(&orchestrator));

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    assert_eq!(object_counts(root), objects);
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);

    quarantine.close().unwrap();
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
}
