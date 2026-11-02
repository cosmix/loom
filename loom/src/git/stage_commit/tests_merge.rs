//! What a Merge scope accepts as `MERGE_HEAD`: one commit, and with a target
//! named, a commit on that target.

use super::*;

/// `root` on `loom/s1` with `main` merged in, `--no-commit`.
fn merging() -> TempDir {
    let temp = repo();
    commit_on_main(temp.path(), "m.txt");
    git_in(temp.path(), &["merge", "--no-commit", "--no-ff", "main"]);
    temp
}

/// `MERGE_HEAD`'s file in `root`'s git directory.
fn merge_head_file(root: &Path) -> PathBuf {
    let listed = git_in(
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ],
    );
    PathBuf::from(listed)
}

/// Commit the current index in a Merge scope with `main` named as the target.
fn commit_merging_into_main(root: &Path) -> Result<String, CommitRefusal> {
    let git = WorktreeGit::discovered(root);
    Committer::new(&git, root)
        .merging_into("main")
        .commit_staged(&merge_scope(), &request(root, "merge main"))
}

#[test]
fn refuses_a_merge_head_naming_more_than_one_commit() {
    let temp = merging();
    let root = temp.path();
    let first = tip(root, "MERGE_HEAD");
    let other = tip(root, "main~1");
    fs::write(merge_head_file(root), format!("{first}\n{other}\n")).unwrap();

    assert_refused(root, &merge_scope(), "MERGE_HEAD names 2 commits");
}

#[test]
fn refuses_a_merge_head_off_the_target() {
    let temp = merging();
    let root = temp.path();
    stage_gitlink(root, "sub");
    let tree = git_in(root, &["write-tree"]);
    let forged = git_in(root, &["commit-tree", &tree, "-p", "HEAD", "-m", "forged"]);
    fs::write(merge_head_file(root), format!("{forged}\n")).unwrap();
    let before = tip(root, BRANCH);

    let refusal = reason(commit_merging_into_main(root));

    assert!(refusal.contains("is not on main"), "{refusal}");
    assert_eq!(tip(root, BRANCH), before);
}

#[test]
fn accepts_a_merge_head_the_target_has_moved_past() {
    let temp = merging();
    let root = temp.path();
    let merged = tip(root, "MERGE_HEAD");
    let tree = tip(root, "main^{tree}");
    let later = git_in(root, &["commit-tree", &tree, "-p", "main", "-m", "later"]);
    git_in(root, &["update-ref", "refs/heads/main", &later]);

    let git = WorktreeGit::discovered(root);
    let id = Committer::new(&git, root)
        .merging_into("main")
        .merging_snapshot(&merged)
        .commit_staged(&merge_scope(), &request(root, "merge main"))
        .unwrap();

    assert_eq!(tip(root, &format!("{id}^2")), merged);
}

#[test]
fn refuses_historical_target_gitlink_removed_before_authorization() {
    let temp = repo();
    let root = temp.path();
    stage(root, "stage.txt", "stage\n");
    git_in(root, &["commit", "-m", "stage"]);
    git_in(root, &["checkout", "main"]);
    stage_gitlink(root, "sub");
    git_in(root, &["commit", "-m", "add gitlink"]);
    let historical = tip(root, "HEAD");
    git_in(root, &["rm", "--cached", "sub"]);
    git_in(root, &["commit", "-m", "remove gitlink"]);
    let authorized = tip(root, "HEAD");
    git_in(root, &["checkout", "loom/s1"]);
    git_in(root, &["merge", "--no-commit", "--no-ff", &historical]);
    let before = tip(root, BRANCH);
    let git = WorktreeGit::discovered(root);
    let refusal = reason(
        Committer::new(&git, root)
            .merging_into("main")
            .merging_snapshot(&authorized)
            .commit_staged(&merge_scope(), &request(root, "merge old target")),
    );
    assert!(refusal.contains("authorized target snapshot"), "{refusal}");
    assert_eq!(tip(root, BRANCH), before);
    assert_eq!(tip(root, "MERGE_HEAD"), historical);
}

/// The authorized snapshot itself may carry a gitlink the target added: the
/// merge-side exemption stays for a fresh merge of the current target.
#[test]
fn accepts_a_gitlink_the_authorized_snapshot_carries() {
    let temp = repo();
    let root = temp.path();
    stage(root, "stage.txt", "stage\n");
    git_in(root, &["commit", "-m", "stage"]);
    git_in(root, &["checkout", "main"]);
    stage_gitlink(root, "sub");
    git_in(root, &["commit", "-m", "add gitlink"]);
    let authorized = tip(root, "HEAD");
    git_in(root, &["checkout", "loom/s1"]);
    git_in(root, &["merge", "--no-commit", "--no-ff", &authorized]);

    let git = WorktreeGit::discovered(root);
    let id = Committer::new(&git, root)
        .merging_into("main")
        .merging_snapshot(&authorized)
        .commit_staged(&merge_scope(), &request(root, "merge main"))
        .unwrap();

    assert_eq!(tip(root, &format!("{id}^2")), authorized);
}

/// The target moves past the snapshot while the resolver works, and removes
/// the gitlink meanwhile. The snapshot was authorized when the resolver
/// started, so its merge still commits; later landing gates judge the result.
#[test]
fn accepts_the_authorized_snapshot_after_the_target_removes_its_gitlink() {
    let temp = repo();
    let root = temp.path();
    stage(root, "stage.txt", "stage\n");
    git_in(root, &["commit", "-m", "stage"]);
    git_in(root, &["checkout", "main"]);
    stage_gitlink(root, "sub");
    git_in(root, &["commit", "-m", "add gitlink"]);
    let authorized = tip(root, "HEAD");
    git_in(root, &["rm", "--cached", "sub"]);
    git_in(root, &["commit", "-m", "remove gitlink"]);
    git_in(root, &["checkout", "loom/s1"]);
    git_in(root, &["merge", "--no-commit", "--no-ff", &authorized]);

    let git = WorktreeGit::discovered(root);
    let id = Committer::new(&git, root)
        .merging_into("main")
        .merging_snapshot(&authorized)
        .commit_staged(&merge_scope(), &request(root, "merge main"))
        .unwrap();

    assert_eq!(tip(root, &format!("{id}^2")), authorized);
}

/// Through a pinned worktree of a repository with `extensions.worktreeConfig`
/// on, whose git directory is private to each command: the single-`MERGE_HEAD`
/// check still reads the real merge state, and the commit clears it.
#[test]
fn a_pinned_merge_commit_with_worktree_config_on_reads_and_clears_the_real_state() {
    let temp = repo();
    let root = temp.path();
    commit_on_main(root, "m.txt");
    let linked = root.join("wt");
    git_in(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_str().unwrap(),
            "loom/s1",
        ],
    );
    git_in(root, &["checkout", "main"]);
    git_in(&linked, &["checkout", "-q", "loom/s1"]);
    git_in(root, &["config", "extensions.worktreeConfig", "true"]);
    git_in(&linked, &["merge", "--no-commit", "--no-ff", "main"]);
    let admin = root.join(".git/worktrees/wt");
    assert!(admin.join("MERGE_HEAD").is_file());

    let git = WorktreeGit::pinned(root, &linked).unwrap();
    let id = Committer::new(&git, root)
        .merging_into("main")
        .commit_staged(&merge_scope(), &request(&linked, "merge main"))
        .unwrap();

    assert_eq!(tip(root, &format!("{id}^2")), tip(root, "main"));
    assert!(!admin.join("MERGE_HEAD").exists());

    git_in(&linked, &["reset", "-q", "--hard", "HEAD^"]);
    git_in(&linked, &["merge", "--no-commit", "--no-ff", "main"]);
    let head = fs::read_to_string(admin.join("MERGE_HEAD")).unwrap();
    fs::write(admin.join("MERGE_HEAD"), format!("{head}{head}")).unwrap();
    let refusal = reason(
        Committer::new(&git, root)
            .merging_into("main")
            .commit_staged(&merge_scope(), &request(&linked, "merge main")),
    );
    assert!(refusal.contains("MERGE_HEAD names 2 commits"), "{refusal}");
}

#[test]
fn the_refusal_outside_a_merge_scope_names_each_way_out() {
    let temp = merging();
    let root = temp.path();
    let before = tip(root, BRANCH);

    let refusal = reason(commit_staged(
        root,
        &stage_scope(),
        &request(root, "feat(s1): x"),
    ));

    for expected in [
        "MERGE_HEAD",
        "A merge session finishes it with loom stage commit",
        "git commit --no-edit, then loom stage merge s1 --resolved",
        "To discard the merge instead: git merge --abort",
    ] {
        assert!(refusal.contains(expected), "{refusal}");
    }
    assert_eq!(tip(root, BRANCH), before);
}
