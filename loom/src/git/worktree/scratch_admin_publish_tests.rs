//! What git writes through a private git directory reaches the real worktree
//! state: the index and reflog through links, new and deleted pseudorefs and a
//! moved `HEAD` by publication.

use super::*;

#[test]
fn staging_and_committing_reach_the_real_worktree_state() {
    let fixture = Fixture::new();
    let pinned = fixture.pinned();
    std::fs::write(fixture.worktree.join("new.txt"), "new\n").unwrap();

    pinned.run_checked(&["add", "--", "new.txt"]).unwrap();

    let staged = ok(&fixture.worktree, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged.trim(), "new.txt");
    assert!(!fixture.admin.join("index.lock").exists());

    let before = std::fs::read_to_string(fixture.admin.join("logs/HEAD")).unwrap();
    pinned
        .run_checked(&["commit", "-q", "-m", "pinned"])
        .unwrap();

    let subject = ok(&fixture.worktree, &["log", "-1", "--format=%s", "loom/s1"]);
    assert_eq!(subject.trim(), "pinned");
    let tracked = ["status", "--porcelain", "--untracked-files=no"];
    assert_eq!(ok(&fixture.worktree, &tracked).trim(), "");
    let after = std::fs::read_to_string(fixture.admin.join("logs/HEAD")).unwrap();
    assert_eq!(after.lines().count(), before.lines().count() + 1);
    assert!(fixture.admin.join("COMMIT_EDITMSG").is_file());
    assert_eq!(
        ok(&fixture.repo, &["rev-parse", "loom/s1"]),
        pinned.run_checked(&["rev-parse", "HEAD"]).unwrap(),
    );
}

/// The checkout's own git directory is the common directory, and
/// `<common>/config.worktree` is what git would read there.
#[test]
fn the_main_checkout_gets_the_same_private_directory() {
    let fixture = Fixture::new();
    let file = fixture.common.join(CONFIG_WORKTREE);
    std::fs::write(&file, fixture.hostile_config()).unwrap();
    let pinned = WorktreeGit::pinned_main(&fixture.repo).unwrap();

    let error = pinned.run(&["status", "--porcelain"]).unwrap_err();
    assert!(format!("{error:#}").contains("filter.x.clean"), "{error:#}");
    std::fs::remove_file(&file).unwrap();

    std::fs::write(fixture.repo.join("main.txt"), "main\n").unwrap();
    pinned.run_checked(&["add", "--", "main.txt"]).unwrap();
    pinned
        .run_checked(&["commit", "-q", "-m", "on main"])
        .unwrap();

    let subject = ok(&fixture.repo, &["log", "-1", "--format=%s", "main"]);
    assert_eq!(subject.trim(), "on main");
    let staged = ok(&fixture.repo, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged.trim(), "");
    assert_eq!(
        pinned
            .run_checked(&["symbolic-ref", "HEAD"])
            .unwrap()
            .trim(),
        "refs/heads/main"
    );
}

#[test]
fn a_status_refresh_writes_the_real_index() {
    let fixture = Fixture::new();
    ok(&fixture.worktree, &["read-tree", "HEAD"]);
    let before = std::fs::read(fixture.admin.join("index")).unwrap();

    fixture
        .pinned()
        .run_checked(&["status", "--porcelain"])
        .unwrap();

    let after = std::fs::read(fixture.admin.join("index")).unwrap();
    assert_ne!(
        before, after,
        "the refreshed index never reached the real file"
    );
    assert!(std::fs::symlink_metadata(fixture.admin.join("index"))
        .unwrap()
        .is_file());
}

/// Conflicting commits on `main` and the stage branch.
fn conflicting_commits(fixture: &Fixture) {
    std::fs::write(fixture.repo.join("README.md"), "main\n").unwrap();
    ok(&fixture.repo, &["commit", "-qam", "main side"]);
    std::fs::write(fixture.worktree.join("README.md"), "stage\n").unwrap();
    ok(&fixture.worktree, &["commit", "-qam", "stage side"]);
}

#[test]
fn a_conflicted_merge_publishes_its_state_and_quit_removes_it() {
    let fixture = Fixture::new();
    conflicting_commits(&fixture);
    let pinned = fixture.pinned();

    let merge = pinned.run(&["merge", "--no-edit", "main"]).unwrap();

    assert!(!merge.status.success(), "{merge:?}");
    for name in ["MERGE_HEAD", "MERGE_MSG", "MERGE_MODE", "ORIG_HEAD"] {
        let meta = std::fs::symlink_metadata(fixture.admin.join(name)).unwrap();
        assert!(meta.is_file(), "{name} did not reach the real directory");
    }
    let unmerged = ok(&fixture.worktree, &["status", "--porcelain"]);
    assert!(unmerged.starts_with("UU README.md"), "{unmerged}");
    assert_eq!(
        pinned
            .run_checked(&["rev-parse", "--verify", "MERGE_HEAD"])
            .unwrap(),
        ok(&fixture.repo, &["rev-parse", "main"]),
    );

    pinned.run_checked(&["merge", "--quit"]).unwrap();

    assert!(!fixture.admin.join("MERGE_HEAD").exists());
    assert!(!fixture.admin.join("MERGE_MSG").exists());
}

#[test]
fn committing_a_merge_clears_the_real_merge_state() {
    let fixture = Fixture::new();
    conflicting_commits(&fixture);
    let pinned = fixture.pinned();
    assert!(!pinned
        .run(&["merge", "--no-edit", "main"])
        .unwrap()
        .status
        .success());
    std::fs::write(fixture.worktree.join("README.md"), "both\n").unwrap();

    pinned.run_checked(&["add", "--", "README.md"]).unwrap();
    pinned.run_checked(&["commit", "-q", "--no-edit"]).unwrap();

    assert!(!fixture.admin.join("MERGE_HEAD").exists());
    let parents = ok(&fixture.worktree, &["log", "-1", "--format=%p", "loom/s1"]);
    assert_eq!(parents.split_whitespace().count(), 2, "{parents}");
}

#[test]
fn a_moved_head_is_published_and_a_racing_one_is_reported() {
    let fixture = Fixture::new();
    let pinned = fixture.pinned();
    pinned.run_checked(&["checkout", "-q", "--detach"]).unwrap();
    let real = std::fs::read_to_string(fixture.admin.join("HEAD")).unwrap();
    assert!(
        !real.starts_with("ref:"),
        "the detached HEAD stayed private: {real}"
    );

    let scratch = fixture.scratch();
    std::fs::write(scratch.path().join(HEAD), "ref: refs/heads/main\n").unwrap();
    std::fs::write(fixture.admin.join(HEAD), "ref: refs/heads/loom/s1\n").unwrap();
    let error = scratch.publish().unwrap_err();
    assert!(
        format!("{error:#}").contains("changed while git"),
        "{error:#}"
    );
    let kept = std::fs::read_to_string(fixture.admin.join(HEAD)).unwrap();
    assert_eq!(kept, "ref: refs/heads/loom/s1\n");
}

#[test]
fn a_state_directory_git_creates_privately_is_reported() {
    let fixture = Fixture::new();
    let scratch = fixture.scratch();
    std::fs::create_dir(scratch.path().join("rebase-merge")).unwrap();

    let error = scratch.publish().unwrap_err();

    assert!(
        format!("{error:#}").contains("publishes only files"),
        "{error:#}"
    );
    assert!(!fixture.admin.join("rebase-merge").exists());
}
