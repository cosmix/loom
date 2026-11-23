//! Orphan classification measures against the guard's accepted tip, so a
//! branch that only the live target contains is kept.

use super::super::tests::repo_with_stage_commit;
use super::*;
use crate::git::target_guard;
use crate::test_git::git;

#[test]
fn a_branch_contained_only_in_the_live_target_is_kept() {
    let id = "accepted-tip";
    let (temp, head) = repo_with_stage_commit(id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    let pre_merge_tip = git(root, &["rev-parse", "main"]);
    // No entry yet: the tip is trusted and recorded as the accepted tip.
    target_guard::check(root, &work_dir, "main").unwrap();
    // A merge outside loom moves the live target past the accepted tip.
    git(
        root,
        &[
            "merge",
            "--no-ff",
            "-m",
            "merge stage",
            &branch_name_for_stage(id),
        ],
    );
    let stage = Stage {
        status: StageStatus::Completed,
        merged: true,
        completed_commit: Some(head),
        ..Stage::default()
    };

    assert!(!classify_stage(id, &stage, "main", root, &work_dir));

    let merged_tip = git(root, &["rev-parse", "main"]);
    target_guard::record_advance(&work_dir, "main", &pre_merge_tip, &merged_tip).unwrap();
    assert!(classify_stage(id, &stage, "main", root, &work_dir));
}
