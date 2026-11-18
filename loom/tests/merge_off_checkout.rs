//! Merging a stage never uses the operator's main checkout as a workspace: a
//! conflict is resolved inside the stage worktree, unrelated staged and
//! unstaged work in the main checkout survives, and an overlapping uncommitted
//! edit blocks the merge with the path named.
//!
//! Only synchronous `git` commands run here: no daemon and no loom binary.

use loom::git::merge::{
    check_resolved_worktree, merge_head_exists, merge_stage, MergeBlock, MergeGate, MergeResult,
};
use serial_test::serial;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[path = "test_support/mod.rs"]
mod test_support;
use test_support::git_repo::{git, git as git_out, git_unchecked, TestRepo};

fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-m", &format!("edit {name}")]);
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// A repository on `main` holding `files`, with `.worktrees` excluded so the
/// stage worktrees never show in its status.
fn init_repo(files: &[(&str, &str)]) -> TempDir {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    TestRepo::builder().no_seed().init_in(root);
    std::fs::write(root.join(".git/info/exclude"), ".worktrees/\n").unwrap();
    for (name, text) in files {
        commit_file(root, name, text);
    }
    repo
}

/// Create `.worktrees/<id>` on `loom/<id>` from `main`.
fn add_stage_worktree(root: &Path, id: &str) -> PathBuf {
    let path = root.join(".worktrees").join(id);
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &format!("loom/{id}"),
            path.to_str().unwrap(),
        ],
    );
    path
}

/// The merge lock's work directory, outside the repository so the lock file
/// never shows as an untracked file there.
fn merge(root: &Path, work: &TempDir, id: &str) -> MergeResult {
    merge_stage(id, "main", root, work.path(), MergeGate::Enforce).unwrap()
}

fn status(root: &Path) -> String {
    git_out(root, &["status", "--porcelain=v2", "--untracked-files=all"])
}

fn no_merge_head(root: &Path) {
    assert!(
        !merge_head_exists(root).unwrap(),
        "MERGE_HEAD in the main checkout"
    );
}

fn parent_count(root: &Path, rev: &str) -> usize {
    git_out(root, &["rev-list", "--parents", "-n1", rev])
        .split_whitespace()
        .count()
        - 1
}

/// `conflict.txt` conflicts between `main` and `loom/s1`; the main checkout
/// holds a staged new file and an unstaged edit to `notes.txt`.
fn conflicting_stage_with_dirty_main() -> (TempDir, PathBuf) {
    let repo = init_repo(&[("conflict.txt", "base\n"), ("notes.txt", "notes\n")]);
    let root = repo.path();
    let wt = add_stage_worktree(root, "s1");
    commit_file(&wt, "conflict.txt", "stage side\n");
    commit_file(root, "conflict.txt", "main side\n");
    std::fs::write(root.join("staged.txt"), "staged\n").unwrap();
    git(root, &["add", "staged.txt"]);
    std::fs::write(root.join("notes.txt"), "notes\nunstaged edit\n").unwrap();
    (repo, wt)
}

/// What the resolver does in its worktree: merge `main`, resolve, commit.
fn resolve_in_worktree(root: &Path, wt: &Path) {
    let own = git_out(wt, &["rev-parse", "HEAD"]);
    let own = Some(own.as_str());
    assert!(
        !git_unchecked(wt, &["merge", "main"]).status.success(),
        "the merge must conflict"
    );
    let reason = check_resolved_worktree(root, "s1", own).expect_err("merge in progress");
    assert!(reason.contains("MERGE_HEAD"), "{reason}");
    std::fs::write(wt.join("conflict.txt"), "resolved\n").unwrap();
    git(wt, &["add", "conflict.txt"]);
    git(wt, &["commit", "--no-edit"]);
    assert_eq!(check_resolved_worktree(root, "s1", own), Ok(()));
}

#[test]
#[serial]
fn a_conflict_resolved_in_the_worktree_reaches_main_and_keeps_local_work() {
    let (repo, wt) = conflicting_stage_with_dirty_main();
    let root = repo.path();
    let work = TempDir::new().unwrap();
    let (main_before, status_before) = (git_out(root, &["rev-parse", "main"]), status(root));

    let MergeResult::Conflict { conflicting_files } = merge(root, &work, "s1") else {
        panic!("expected a conflict");
    };
    assert!(conflicting_files.contains(&"conflict.txt".to_string()));
    assert_eq!(git_out(root, &["rev-parse", "main"]), main_before);
    assert_eq!(status(root), status_before);
    no_merge_head(root);

    resolve_in_worktree(root, &wt);
    no_merge_head(root);
    let landed = merge(root, &work, "s1");
    assert!(matches!(landed, MergeResult::Success { .. }), "{landed:?}");

    no_merge_head(root);
    assert_eq!(parent_count(root, "main"), 2);
    assert_eq!(git_out(root, &["show", "main:conflict.txt"]), "resolved");
    assert_eq!(read(root, "conflict.txt"), "resolved\n");
    assert_eq!(
        git_out(root, &["diff", "--cached", "--name-only"]),
        "staged.txt"
    );
    assert_eq!(read(root, "staged.txt"), "staged\n");
    assert_eq!(git_out(root, &["diff", "--name-only"]), "notes.txt");
    assert_eq!(read(root, "notes.txt"), "notes\nunstaged edit\n");
}

/// `shared.txt` changes on line 2 in `loom/s2`; the main checkout edits the
/// adjacent line 3 without committing, which the reapply dry run predicts to
/// conflict.
fn stage_overlapping_an_uncommitted_edit() -> TempDir {
    let repo = init_repo(&[("shared.txt", "l1\nl2\nl3\nl4\n")]);
    let wt = add_stage_worktree(repo.path(), "s2");
    commit_file(&wt, "shared.txt", "l1\nl2 stage\nl3\nl4\n");
    std::fs::write(repo.path().join("shared.txt"), "l1\nl2\nl3 local\nl4\n").unwrap();
    repo
}

fn blocked_paths(result: MergeResult) -> Vec<String> {
    match result {
        MergeResult::Blocked(MergeBlock::UncommittedOverlap { paths }) => paths,
        other => panic!("expected UncommittedOverlap, got {other:?}"),
    }
}

#[test]
#[serial]
fn an_overlapping_uncommitted_edit_blocks_until_the_operator_sets_it_aside() {
    let repo = stage_overlapping_an_uncommitted_edit();
    let root = repo.path();
    let work = TempDir::new().unwrap();
    let (main_before, status_before) = (git_out(root, &["rev-parse", "main"]), status(root));

    let paths = blocked_paths(merge(root, &work, "s2"));

    assert_eq!(paths, vec!["shared.txt".to_string()]);
    assert_eq!(read(root, "shared.txt"), "l1\nl2\nl3 local\nl4\n");
    assert_eq!(status(root), status_before);
    assert_eq!(git_out(root, &["rev-parse", "main"]), main_before);
    assert_eq!(git_out(root, &["stash", "list"]), "");
    no_merge_head(root);

    git(root, &["stash"]);
    let landed = merge(root, &work, "s2");
    assert!(matches!(landed, MergeResult::Success { .. }), "{landed:?}");
    assert_eq!(read(root, "shared.txt"), "l1\nl2 stage\nl3\nl4\n");
    git(root, &["merge-base", "--is-ancestor", "loom/s2", "main"]);
}

#[test]
#[serial]
fn committing_the_overlapping_edit_turns_the_block_into_a_conflict() {
    let repo = stage_overlapping_an_uncommitted_edit();
    let root = repo.path();
    let work = TempDir::new().unwrap();
    blocked_paths(merge(root, &work, "s2"));

    git(root, &["add", "shared.txt"]);
    git(root, &["commit", "-m", "local edit"]);
    let main_before = git_out(root, &["rev-parse", "main"]);

    let MergeResult::Conflict { conflicting_files } = merge(root, &work, "s2") else {
        panic!("the same lines conflict once the edit is committed");
    };
    assert_eq!(conflicting_files, vec!["shared.txt".to_string()]);
    assert_eq!(git_out(root, &["rev-parse", "main"]), main_before);
    no_merge_head(root);
}
