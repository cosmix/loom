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

    let id = commit_merging_into_main(root).unwrap();

    assert_eq!(tip(root, &format!("{id}^2")), merged);
}
