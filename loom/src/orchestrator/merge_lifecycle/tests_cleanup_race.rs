//! Two processes cleaning up one merged stage: the second waits for the first
//! and finds nothing left, instead of failing against a half-removed worktree.

use std::sync::mpsc;
use std::time::Duration;

use super::cleanup_guard::lock_stage_cleanup;
use super::tests::{
    cleanup_from_outside, merge_stage_branch, repo_with_stage_commit, worktree_of,
    write_stage_record,
};
use super::CleanupOutcome;
use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::{branch_exists_strict, cleanup_after_merge, CleanupConfig};
use crate::verify::transitions::load_stage;

#[test]
fn a_cleanup_waits_for_a_concurrent_one_and_finds_nothing_to_do() {
    let stage_id = "raced-cleanup-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path().to_path_buf();
    let work_dir = root.join(".loom").join("work");
    write_stage_record(&work_dir, stage_id, &head);
    merge_stage_branch(&root, stage_id);

    // Another process (the daemon's leftover sweep) is mid-cleanup.
    let held = lock_stage_cleanup(&work_dir, stage_id).expect("cleanup lock");
    let (done_tx, done_rx) = mpsc::channel();
    let racer_root = root.clone();
    let racer = std::thread::spawn(move || {
        let outcome = cleanup_from_outside(&racer_root, stage_id);
        done_tx.send(()).unwrap();
        outcome
    });
    assert!(
        done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "the second cleanup ran while the first held the stage cleanup lock"
    );
    cleanup_after_merge(stage_id, &root, &CleanupConfig::quiet()).unwrap();
    drop(held);

    let outcome = racer.join().unwrap();
    assert!(
        matches!(outcome, CleanupOutcome::NothingToDo),
        "got {outcome:?}"
    );
    assert!(!worktree_of(&root, stage_id).exists());
    assert!(!branch_exists_strict(&branch_name_for_stage(stage_id), &root).unwrap());
    assert!(
        load_stage(stage_id, &work_dir)
            .unwrap()
            .cleanup_warning
            .is_none(),
        "a cleanup another process finished is not a warning"
    );
}
