//! A block holding the operator's work in a backup stash ref is not retried
//! until that ref is gone.

use super::*;

const BACKUP_REF: &str = "refs/loom/autostash/s-1-abc";

fn stash_list(root: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["stash", "list"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_block_with_the_operator_work_in_a_backup_stash_is_not_retried_until_the_ref_is_gone() {
    let (repo, mut orchestrator) = blocked_stage();
    let root = repo.path();
    // The cause of the block is gone: only the backup ref holds the retry.
    git_ok(root, &["stash", "push", "-q", "--include-untracked"]);
    git_ok(root, &["update-ref", BACKUP_REF, "HEAD"]);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.block_merge(MergeBlock::StashNotRestored {
            backup_ref: BACKUP_REF.to_string(),
        });
        Ok(())
    })
    .unwrap();
    let (objects, stage_file, stashes) = (
        object_counts(root),
        stage_file_text(&orchestrator),
        stash_list(root),
    );

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    assert_eq!(object_counts(root), objects);
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(stash_list(root), stashes);
    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);

    git_ok(root, &["update-ref", "-d", BACKUP_REF]);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
}

/// A refused reset leaves the changes in the checkout and spare copies in the
/// stash, so nothing else stops a retry from stashing them again every tick.
#[test]
fn a_refused_reset_is_not_retried_until_its_spare_ref_is_gone() {
    let (repo, mut orchestrator) = blocked_stage();
    let root = repo.path();
    // The cause of the block is gone: only the spare ref holds the retry.
    git_ok(root, &["stash", "push", "-q", "--include-untracked"]);
    git_ok(root, &["update-ref", BACKUP_REF, "HEAD"]);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.block_merge(MergeBlock::StashResetRefused {
            backup_ref: BACKUP_REF.to_string(),
        });
        Ok(())
    })
    .unwrap();
    let (objects, stage_file, stashes) = (
        object_counts(root),
        stage_file_text(&orchestrator),
        stash_list(root),
    );

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    assert_eq!(object_counts(root), objects);
    assert_eq!(stage_file_text(&orchestrator), stage_file);
    assert_eq!(stash_list(root), stashes);

    git_ok(root, &["update-ref", "-d", BACKUP_REF]);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    assert_eq!(on_disk(&orchestrator).status, StageStatus::Completed);
}

#[test]
fn a_stage_with_an_unrestored_backup_ref_that_still_exists_is_not_retried() {
    let (repo, mut orchestrator) = blocked_stage();
    git_ok(repo.path(), &["stash", "push", "-q", "--include-untracked"]);
    git_ok(repo.path(), &["update-ref", BACKUP_REF, "HEAD"]);
    let mut stage = on_disk(&orchestrator);
    stage.merge.unrestored.push(BACKUP_REF.to_string());

    orchestrator.retry_blocked_merge(&stage);

    assert_eq!(on_disk(&orchestrator).status, StageStatus::MergeBlocked);
}
