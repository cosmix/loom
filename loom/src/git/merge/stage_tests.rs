//! `merge_stage` against scratch repositories: what happens to the operator's
//! checkout for each kind of uncommitted state.

use super::test_support::{
    commit_file, git_ok, git_out, init_repo, lock_dir, rev, sibling_dir, snapshot, stage_branch,
};
use super::*;

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

fn merge(root: &Path, work: &tempfile::TempDir) -> MergeResult {
    merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap()
}

fn overlap_paths(result: MergeResult) -> Vec<String> {
    match result {
        MergeResult::Blocked(MergeBlock::UncommittedOverlap { paths }) => paths,
        other => panic!("expected UncommittedOverlap, got {other:?}"),
    }
}

fn parent_count(root: &Path, rev_name: &str) -> usize {
    git_out(root, &["rev-list", "--parents", "-n1", rev_name])
        .split_whitespace()
        .count()
        - 1
}

#[test]
fn unrelated_local_work_survives_a_merge() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "s.txt", "s0", "s");
    commit_file(root, "u.txt", "u0", "u");
    stage_branch(root, "s1", &[("b.txt", "from branch")]);
    let before = rev(root, "main");
    std::fs::write(root.join("s.txt"), "staged").unwrap();
    git_ok(root, &["add", "s.txt"]);
    std::fs::write(root.join("u.txt"), "unstaged").unwrap();
    std::fs::write(root.join("new.txt"), "untracked").unwrap();
    let work = lock_dir();

    let result = merge(root, &work);

    assert!(matches!(result, MergeResult::Success { .. }));
    assert_eq!(parent_count(root, "main"), 2);
    assert_eq!(rev(root, "main^1"), before);
    assert_eq!(git_out(root, &["diff", "--cached", "--name-only"]), "s.txt");
    assert_eq!(git_out(root, &["diff", "--name-only"]), "u.txt");
    assert_eq!(
        std::fs::read_to_string(root.join("s.txt")).unwrap(),
        "staged"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("u.txt")).unwrap(),
        "unstaged"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "untracked"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "from branch"
    );
    assert!(!root.join(".git").join("MERGE_HEAD").exists());
}

#[test]
fn predicted_conflict_with_local_edit_blocks_and_changes_nothing() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", "l1\nl2\nl3\nl4\n", "f");
    stage_branch(
        root,
        "s1",
        &[("f.txt", "l1\nl2b\nl3\nl4\n"), ("b.txt", "new")],
    );
    std::fs::write(root.join("f.txt"), "l1\nl2\nl3b\nl4\n").unwrap();
    let (before, before_head) = (snapshot(root), rev(root, "main"));
    let work = lock_dir();

    let paths = overlap_paths(merge(root, &work));

    assert_eq!(paths, vec!["f.txt".to_string()]);
    assert_eq!(snapshot(root), before);
    assert_eq!(rev(root, "main"), before_head);
    assert!(!root.join("b.txt").exists());
    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert_eq!(git_out(root, &["for-each-ref", "refs/loom"]), "");
}

#[test]
fn a_clean_overlap_blocks_and_changes_nothing() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(root, "s1", &[("f.txt", branch_text.as_str())]);
    let local = LINES.replace("l10\n", "l10b\n");
    std::fs::write(root.join("f.txt"), &local).unwrap();
    let before_head = rev(root, "main");
    let work = lock_dir();

    let paths = overlap_paths(merge(root, &work));

    assert_eq!(paths, vec!["f.txt".to_string()]);
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), local);
    assert_eq!(rev(root, "main"), before_head);
}

#[test]
fn identical_untracked_file_is_replaced_by_the_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "same")]);
    std::fs::write(root.join("b.txt"), "same").unwrap();
    let work = lock_dir();

    assert!(matches!(merge(root, &work), MergeResult::Success { .. }));

    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "same");
    assert_eq!(
        git_out(root, &["ls-tree", "--name-only", "main", "b.txt"]),
        "b.txt"
    );
    assert_eq!(git_out(root, &["status", "--porcelain"]), "");
}

#[test]
fn different_untracked_file_blocks_the_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    std::fs::write(root.join("b.txt"), "mine").unwrap();
    let before = rev(root, "main");
    let work = lock_dir();

    let paths = overlap_paths(merge(root, &work));

    assert_eq!(paths, vec!["b.txt".to_string()]);
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "mine");
    assert_eq!(rev(root, "main"), before);
}

#[test]
fn target_not_checked_out_is_advanced_with_update_ref() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    git_ok(root, &["checkout", "-b", "other"]);
    let (head, before) = (rev(root, "HEAD"), snapshot(root));
    let work = lock_dir();

    assert!(matches!(merge(root, &work), MergeResult::Success { .. }));

    assert_eq!(
        git_out(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "other"
    );
    assert_eq!(rev(root, "HEAD"), head);
    assert_eq!(snapshot(root), before);
    assert_eq!(parent_count(root, "main"), 2);
}

#[test]
fn a_full_ref_target_is_advanced_with_update_ref() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    git_ok(root, &["checkout", "-b", "other"]);
    let work = lock_dir();

    let result = merge_stage(
        "s1",
        "refs/heads/main",
        root,
        work.path(),
        MergeGate::Enforce,
    )
    .unwrap();

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_eq!(parent_count(root, "main"), 2);
}

#[test]
fn target_checked_out_in_another_worktree_blocks() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    git_ok(root, &["checkout", "-b", "other"]);
    let (_keep, second) = sibling_dir();
    git_ok(root, &["worktree", "add", second.to_str().unwrap(), "main"]);
    let work = lock_dir();

    match merge(root, &work) {
        MergeResult::Blocked(MergeBlock::TargetCheckedOutElsewhere { path }) => {
            assert_eq!(path.canonicalize().unwrap(), second.canonicalize().unwrap());
        }
        other => panic!("expected TargetCheckedOutElsewhere, got {other:?}"),
    }
}

#[test]
fn conflicting_branch_reports_paths_and_leaves_the_checkout_alone() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("a.txt", "branch")]);
    commit_file(root, "a.txt", "main", "main");
    let before = snapshot(root);
    let work = lock_dir();

    match merge(root, &work) {
        MergeResult::Conflict { conflicting_files } => {
            assert_eq!(conflicting_files, vec!["a.txt".to_string()]);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(snapshot(root), before);
    assert!(!root.join(".git").join("MERGE_HEAD").exists());
}

#[test]
fn branch_already_in_target_is_up_to_date() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[]);
    let before = rev(root, "main");
    let work = lock_dir();

    assert!(matches!(merge(root, &work), MergeResult::AlreadyUpToDate));
    assert_eq!(rev(root, "main"), before);
}

/// A real repository at `sub`, recorded in `main` as a gitlink. Returns its
/// `HEAD` and a second commit it holds but has not checked out.
fn submodule(root: &Path) -> (String, String) {
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    crate::test_git::TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .init_in(&sub);
    std::fs::write(sub.join(".gitattributes"), "* filter=planted\n").unwrap();
    commit_file(&sub, "a.txt", "first\n", "first");
    commit_file(&sub, "a.txt", "second\n", "second");
    let second = rev(&sub, "HEAD");
    git_ok(&sub, &["reset", "-q", "--hard", "HEAD~1"]);
    git_ok(root, &["add", "sub"]);
    let entry = git_out(root, &["ls-files", "-s", "sub"]);
    assert!(entry.starts_with("160000 "), "{entry}");
    git_ok(root, &["commit", "-q", "-m", "submodule"]);
    (rev(&sub, "HEAD"), second)
}

/// Give `sub` a clean filter that appends to `marker` and make its tracked
/// `a.txt` dirty: a `git status` inside it now runs the filter. Run after
/// every fixture git command in the root, which would recurse into it.
fn dirty_submodule(root: &Path, marker: &Path) {
    let sub = root.join("sub");
    let filter = format!("sh -c 'echo ran >> {}; cat'", marker.display());
    git_ok(&sub, &["config", "filter.planted.clean", &filter]);
    std::fs::write(sub.join("a.txt"), "dirty\n").unwrap();
}

fn assert_submodule_intact(root: &Path, head: &str, marker: &Path) {
    let sub = root.join("sub");
    assert_eq!(rev(&sub, "HEAD"), head);
    assert_eq!(std::fs::read(sub.join("a.txt")).unwrap(), b"dirty\n");
    assert!(!marker.exists(), "a git command ran the submodule's filter");
}

#[test]
fn a_dirty_submodule_checkout_survives_an_unrelated_merge() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    let marker = work.path().join("filter-ran");
    let (head, _) = submodule(root);
    stage_branch(root, "s1", &[("b.txt", "from branch")]);
    let before = rev(root, "main");
    dirty_submodule(root, &marker);

    let result = merge(root, &work);

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_ne!(rev(root, "main"), before);
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "from branch"
    );
    assert_submodule_intact(root, &head, &marker);
}

/// Git's fast-forward records the branch's gitlink without touching the
/// submodule's checkout, so the merge lands and the operator's submodule
/// keeps its commit and its dirty file; `sub` then shows as modified.
#[test]
fn a_branch_that_moves_a_dirty_submodule_gitlink_lands_and_leaves_the_submodule() {
    let repo = init_repo();
    let root = repo.path();
    let work = lock_dir();
    let marker = work.path().join("filter-ran");
    let (head, second) = submodule(root);
    git_ok(root, &["checkout", "-q", "-b", "loom/s1", "main"]);
    let entry = format!("160000,{second},sub");
    git_ok(root, &["update-index", "--cacheinfo", &entry]);
    git_ok(root, &["commit", "-q", "-m", "move the gitlink"]);
    git_ok(root, &["checkout", "-q", "main"]);
    let before = rev(root, "main");
    dirty_submodule(root, &marker);

    let result = merge(root, &work);

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_ne!(rev(root, "main"), before);
    let entry = git_out(root, &["ls-tree", "main", "sub"]);
    assert!(
        entry.starts_with(&format!("160000 commit {second}")),
        "{entry}"
    );
    assert_submodule_intact(root, &head, &marker);
}
