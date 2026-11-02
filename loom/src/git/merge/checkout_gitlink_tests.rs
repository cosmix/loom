//! A merge around uncommitted changes never opens the configuration of a
//! repository at a gitlink, keeps what an operator's real submodule holds, and
//! writes a stash `git stash` itself can pop.

use std::path::Path;

use super::checkout_stash::{reset_checkout, snapshot_changes, store_unchanged};
use super::test_support::{commit_file, git_ok, git_out, init_repo, lock_dir, stage_branch};
use super::{merge_stage, MergeGate, MergeResult};

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

/// A repository at `root/sub` whose clean filter appends to `marker` and whose
/// tracked file is modified: a `git status` inside it runs the filter.
fn plant_repository(root: &Path, marker: &Path) -> String {
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    git_ok(&sub, &["init", "-q", "-b", "main"]);
    git_ok(&sub, &["config", "user.email", "t@t.com"]);
    git_ok(&sub, &["config", "user.name", "t"]);
    std::fs::write(sub.join("a.bin"), "x\n").unwrap();
    std::fs::write(sub.join(".gitattributes"), "* filter=planted\n").unwrap();
    git_ok(&sub, &["add", "a.bin", ".gitattributes"]);
    git_ok(&sub, &["commit", "-q", "-m", "planted"]);
    let filter = format!("sh -c 'echo ran >> {}; cat'", marker.display());
    git_ok(&sub, &["config", "filter.planted.clean", &filter]);
    std::fs::write(sub.join("a.bin"), "changed\n").unwrap();
    git_out(&sub, &["rev-parse", "HEAD"])
}

fn gitlink_entry(root: &Path, commit: &str) {
    let entry = format!("160000,{commit},sub");
    git_ok(root, &["update-index", "--add", "--cacheinfo", &entry]);
}

/// `f.txt` edited at its end while the branch edits its start: the overlap
/// goes through the stash path.
fn overlapping_checkout(root: &Path) -> String {
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(root, "s1", &[("f.txt", branch_text.as_str())]);
    let edited = LINES.replace("l10\n", "l10b\n");
    std::fs::write(root.join("f.txt"), &edited).unwrap();
    LINES.replace("l1\n", "l1b\n").replace("l10\n", "l10b\n")
}

fn merge(root: &Path) -> MergeResult {
    let work = lock_dir();
    merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap()
}

#[test]
fn a_staged_planted_gitlink_runs_no_filter_through_the_stash_path() {
    let repo = init_repo();
    let root = repo.path();
    let marker = lock_dir();
    let marker = marker.path().join("filter-ran");
    let merged = overlapping_checkout(root);
    let commit = plant_repository(root, &marker);
    gitlink_entry(root, &commit);

    let result = merge(root);

    assert!(!marker.exists(), "the planted filter ran");
    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), merged);
}

#[test]
fn a_committed_planted_gitlink_runs_no_filter_through_the_stash_path() {
    let repo = init_repo();
    let root = repo.path();
    let marker = lock_dir();
    let marker = marker.path().join("filter-ran");
    let commit = plant_repository(root, &marker);
    gitlink_entry(root, &commit);
    git_ok(root, &["commit", "-q", "-m", "link"]);
    std::fs::remove_file(&marker).ok();
    let merged = overlapping_checkout(root);
    std::fs::remove_file(&marker).ok();

    let result = merge(root);

    assert!(!marker.exists(), "the planted filter ran");
    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), merged);
}

#[test]
fn a_real_submodule_keeps_its_moved_commit_and_dirty_files_through_the_stash_path() {
    let repo = init_repo();
    let root = repo.path();
    let sub = root.join("sub");
    std::fs::create_dir(&sub).unwrap();
    git_ok(&sub, &["init", "-q", "-b", "main"]);
    git_ok(&sub, &["config", "user.email", "t@t.com"]);
    git_ok(&sub, &["config", "user.name", "t"]);
    commit_file(&sub, "a.txt", "first\n", "first");
    let first = git_out(&sub, &["rev-parse", "HEAD"]);
    gitlink_entry(root, &first);
    std::fs::write(
        root.join(".gitmodules"),
        "[submodule \"sub\"]\n\tpath = sub\n\turl = ./sub\n",
    )
    .unwrap();
    git_ok(root, &["add", ".gitmodules"]);
    git_ok(root, &["commit", "-q", "-m", "submodule"]);
    let merged = overlapping_checkout(root);
    commit_file(&sub, "a.txt", "second\n", "second");
    let second = git_out(&sub, &["rev-parse", "HEAD"]);
    std::fs::write(sub.join("a.txt"), "dirty\n").unwrap();

    let result = merge(root);

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), merged);
    assert_eq!(git_out(&sub, &["rev-parse", "HEAD"]), second);
    assert_eq!(
        std::fs::read_to_string(sub.join("a.txt")).unwrap(),
        "dirty\n"
    );
    let moved = git_out(root, &["diff", "--name-only", "--ignore-submodules=dirty"]);
    assert!(moved.lines().any(|path| path == "sub"), "{moved}");
    assert_eq!(git_out(root, &["stash", "list"]), "");
}

#[test]
fn a_stash_made_for_a_merge_is_a_normal_stash_entry() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let _guard = super::fast_forward::failpoint::inject(super::fast_forward::failpoint::Failures {
        pop: true,
        ..Default::default()
    });

    let MergeResult::Success {
        stash: Some(stash), ..
    } = merge(root)
    else {
        panic!("expected a landed merge with a stash outcome");
    };

    let list = git_out(root, &["stash", "list"]);
    assert!(list.starts_with("stash@{0}: WIP on main: "), "{list}");
    assert_eq!(
        git_out(root, &["rev-parse", "stash@{0}"]),
        git_out(root, &["rev-parse", &stash.backup_ref]),
        "the backup names the stash entry"
    );
    drop(_guard);
    git_ok(root, &["stash", "pop", "--index"]);
    assert!(std::fs::read_to_string(root.join("f.txt"))
        .unwrap()
        .contains("l10b"));
}

#[test]
fn a_staged_and_a_worktree_edit_both_come_back_through_the_stash_path() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "staged.txt", "base\n", "staged");
    commit_file(root, "gone.txt", "base\n", "gone");
    overlapping_checkout(root);
    std::fs::write(root.join("staged.txt"), "staged\n").unwrap();
    git_ok(root, &["add", "staged.txt"]);
    std::fs::remove_file(root.join("gone.txt")).unwrap();

    let result = merge(root);

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert_eq!(
        std::fs::read_to_string(root.join("staged.txt")).unwrap(),
        "staged\n"
    );
    assert!(!root.join("gone.txt").exists());
    let staged = git_out(root, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged, "staged.txt", "the index comes back too");
}

#[test]
fn a_change_after_the_snapshot_stops_the_stash_and_stays_in_the_checkout() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    std::fs::write(root.join("f.txt"), "first edit\n").unwrap();
    let snapshot = snapshot_changes(root).unwrap().unwrap();
    std::fs::write(root.join("f.txt"), "second edit\n").unwrap();

    assert!(store_unchanged(root, &snapshot).is_err());

    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert_eq!(
        std::fs::read_to_string(root.join("f.txt")).unwrap(),
        "second edit\n"
    );
}

#[test]
fn a_file_rewritten_with_its_own_bytes_is_no_change() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    std::fs::write(root.join("f.txt"), LINES).unwrap();

    assert!(snapshot_changes(root).unwrap().is_none());
}

#[test]
fn a_snapshot_of_a_non_utf8_path_comes_back_through_the_stash() {
    use std::os::unix::ffi::OsStrExt;
    let repo = init_repo();
    let root = repo.path();
    let path = root.join(std::ffi::OsStr::from_bytes(b"caf\xff.txt"));
    std::fs::write(&path, "base\n").unwrap();
    git_ok(root, &["add", "--", "caf*"]);
    git_ok(root, &["commit", "-q", "-m", "named"]);
    std::fs::write(&path, "edited\n").unwrap();

    let snapshot = snapshot_changes(root).unwrap().unwrap();
    store_unchanged(root, &snapshot).unwrap();
    reset_checkout(root).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "base\n");
    git_ok(root, &["stash", "pop", "--index", "--quiet"]);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited\n");
}
