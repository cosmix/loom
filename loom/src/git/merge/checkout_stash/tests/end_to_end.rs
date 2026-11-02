//! A whole merge around dirty changes against the same merge done by hand with
//! `git stash push`, `git merge` and `git stash pop --index`, on twin checkouts.

use super::*;
use crate::git::merge::test_support::{lock_dir, snapshot};
use crate::git::merge::{merge_stage, MergeGate, MergeResult};

fn by_loom(root: &Path) {
    let work = lock_dir();
    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();
    let MergeResult::Success { stash, .. } = result else {
        panic!("expected a landed merge, got {result:?}");
    };
    let stash = stash.expect("the merge went through the stash path");
    assert!(stash.restored, "{stash:?}");
}

fn by_hand(root: &Path) {
    git_ok(root, &["stash", "push", "-q"]);
    git_ok(root, &["merge", "-q", "--no-ff", "--no-edit", "loom/s1"]);
    let indexed = isolated_git(root, &["stash", "pop", "--index", "--quiet"]);
    if !indexed.status.success() {
        git_ok(root, &["stash", "pop", "--quiet"]);
    }
}

/// A stage branch editing the start of `f.txt`, `prepare`'s changes, and a local
/// edit at the end of `f.txt`, which sends the merge through the stash path.
fn prepared(prepare: &impl Fn(&Path)) -> TempDir {
    let repo = repo();
    let root = repo.path();
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(root, "s1", &[("f.txt", branch_text.as_str())]);
    prepare(root);
    write(root, "f.txt", &LINES.replace("l10\n", "l10b\n"));
    repo
}

/// Every file, the status and the merged tree, with both routes run from the
/// same start.
fn assert_same_result(case: &str, prepare: impl Fn(&Path)) {
    let (ours, theirs) = (prepared(&prepare), prepared(&prepare));
    by_loom(ours.path());
    by_hand(theirs.path());
    assert_eq!(snapshot(ours.path()), snapshot(theirs.path()), "{case}");
    assert_eq!(
        git_out(ours.path(), &["rev-parse", "HEAD^{tree}"]),
        git_out(theirs.path(), &["rev-parse", "HEAD^{tree}"]),
        "{case}"
    );
    assert_eq!(git_out(ours.path(), &["stash", "list"]), "", "{case}");
}

#[test]
fn a_dirty_merge_ends_where_git_stash_ends() {
    assert_same_result("unstaged only", |_| {});
    assert_same_result("other staged and unstaged edits", |root| {
        write(root, "dir/b.txt", "staged\n");
        git_ok(root, &["add", "dir/b.txt"]);
        write(root, "dir/b.txt", "staged and more\n");
        git_ok(root, &["rm", "-q", "gone/c.txt"]);
        write(root, "new.txt", "new\n");
        git_ok(root, &["add", "new.txt"]);
        write(root, "untracked.txt", "stays\n");
    });
    assert_same_result("mode and symlink changes", |root| {
        let mode = fs::Permissions::from_mode(0o755);
        fs::set_permissions(root.join("run.sh"), mode).unwrap();
        fs::remove_file(root.join("dir/b.txt")).unwrap();
        symlink("../f.txt", root.join("dir/b.txt")).unwrap();
    });
    assert_same_result("a directory over a tracked file", |root| {
        fs::remove_file(root.join("dir/b.txt")).unwrap();
        fs::create_dir(root.join("dir/b.txt")).unwrap();
        write(root, "dir/b.txt/inside", "untracked\n");
    });
}

/// `out/d.txt` is outside the cone, with a staged change only the index holds.
/// A private index without skip-worktree bits reads its absence from disk as a
/// deletion, and the pop leaves it staged and deleted.
#[test]
fn a_sparse_checkout_with_a_staged_out_of_cone_path_ends_where_git_stash_ends() {
    let prepare = |root: &Path| sparse_cone(root, &[]);
    assert_same_result("cone with a staged out-of-cone path", prepare);

    let repo = prepared(&prepare);
    let root = repo.path();
    by_loom(root);
    assert_eq!(git_out(root, &["show", ":out/d.txt"]), "d2");
    assert_eq!(
        git_out(root, &["ls-files", "-t", "out/d.txt"]),
        "S out/d.txt"
    );
    let status = git_out(root, &["status", "--porcelain", "--", "out/d.txt"]);
    assert_eq!(status, "M  out/d.txt");
}

#[test]
fn a_sparse_index_ends_where_git_stash_ends() {
    assert_same_result("sparse index", |root| {
        sparse_cone(root, &["--sparse-index"]);
    });
}

#[test]
fn a_skip_worktree_staged_path_ends_where_git_stash_ends() {
    assert_same_result("skip-worktree marked by hand", staged_skip_worktree);
}
