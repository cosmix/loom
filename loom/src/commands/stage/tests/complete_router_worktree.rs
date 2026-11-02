//! Rule 2 of the router reads a stage worktree through pinned git.

use super::*;

/// `.worktrees/st` exists but the repository does not register it: nothing
/// about a merge in it can be read through pinned git, and that is a refusal.
#[test]
fn a_stage_worktree_the_repository_does_not_register_refuses_completion() {
    let repo = init_repo();
    let work_dir = make_work_dir(repo.path());
    crate::git::worktree::capture_main_identity(repo.path(), &work_dir).unwrap();
    run_git(
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/other",
            ".worktrees/other",
        ],
        repo.path(),
    );
    std::fs::create_dir_all(repo.path().join(".worktrees/st")).unwrap();
    let stage = make_stage("st", StageStatus::Executing);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();

    let CompleteConflictRoute::Refuse { message } = route else {
        panic!("an unregistered worktree must refuse: {route:?}");
    };
    assert!(message.contains("registered worktree"), "{message}");
}

/// The positive control: the registered worktree with no merge in it passes
/// this rule.
#[test]
fn a_registered_stage_worktree_with_no_merge_is_not_refused() {
    let repo = init_repo();
    let work_dir = make_work_dir(repo.path());
    crate::git::worktree::capture_main_identity(repo.path(), &work_dir).unwrap();
    run_git(
        &["worktree", "add", "-q", "-b", "loom/st", ".worktrees/st"],
        repo.path(),
    );
    let stage = make_stage("st", StageStatus::Executing);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();

    assert_eq!(route, CompleteConflictRoute::Proceed);
}
