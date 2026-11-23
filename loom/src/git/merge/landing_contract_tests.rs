//! Contracts for landing a merge into the operator's checkout: local bytes
//! the merge would replace are never lost, and a fast-forward that moved some
//! other branch is never recorded as a merge into the target.

use super::test_support::{commit_file, git_ok, git_out, init_repo, lock_dir, rev, stage_branch};
use super::*;
use crate::test_git::git_command;

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

fn merge(root: &Path, work: &tempfile::TempDir) -> MergeResult {
    merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap()
}

#[test]
fn a_tracked_overlap_is_refused_with_its_bytes_intact() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(root, "s1", &[("f.txt", branch_text.as_str())]);
    let local = LINES.replace("l10\n", "l10b\n");
    std::fs::write(root.join("f.txt"), &local).unwrap();
    let old = rev(root, "main");
    let work = lock_dir();

    let result = merge(root, &work);

    match result {
        MergeResult::Blocked(MergeBlock::UncommittedOverlap { paths }) => {
            assert_eq!(paths, vec!["f.txt".to_string()]);
        }
        other => panic!("expected UncommittedOverlap, got {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), local);
    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert_eq!(git_out(root, &["for-each-ref", "refs/loom"]), "");
    assert_eq!(rev(root, "main"), old);
}

#[test]
fn an_untracked_file_replaced_before_it_is_moved_survives() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("u.txt", "incoming")]);
    std::fs::write(root.join("u.txt"), "incoming").unwrap();
    let old = rev(root, "main");
    super::checkout_files::inject_before_displace(|checkout: &Path| {
        std::fs::write(checkout.join("u.txt"), "operator rewrite").unwrap();
    });
    let work = lock_dir();

    let result = merge(root, &work);

    let (quarantine, failures) = match result {
        MergeResult::Blocked(MergeBlock::CheckoutRestoreIncomplete {
            quarantine,
            failures,
            ..
        }) => (quarantine, failures),
        other => panic!("expected CheckoutRestoreIncomplete, got {other:?}"),
    };
    assert!(
        failures.iter().any(|failure| failure.path == "u.txt"),
        "{failures:?}"
    );
    assert_eq!(
        std::fs::read(quarantine.join("u.txt")).unwrap(),
        b"operator rewrite"
    );
    assert_eq!(rev(root, "main"), old);
}

/// Commit the raw, non-UTF-8 name on `loom/s1` with `git add -f` (main
/// ignores it), then return to `main`. `false` when the filesystem rejects
/// the name.
fn commit_raw_name_on_branch(root: &Path, name: &std::ffi::OsStr) -> bool {
    git_ok(root, &["checkout", "-q", "-b", "loom/s1", "main"]);
    if std::fs::write(root.join(name), "incoming").is_err() {
        git_ok(root, &["checkout", "-q", "main"]);
        return false;
    }
    let added = git_command(root)
        .args(["add", "-f", "--"])
        .arg(name)
        .output()
        .unwrap();
    assert!(added.status.success(), "{added:?}");
    git_ok(root, &["commit", "-q", "-m", "raw name"]);
    git_ok(root, &["checkout", "-q", "main"]);
    true
}

#[test]
fn a_non_utf8_path_the_merge_adds_is_refused() {
    use std::os::unix::ffi::OsStrExt;
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, ".gitignore", "x*\n", "ignore x");
    let name = std::ffi::OsStr::from_bytes(b"x\xff");
    if !commit_raw_name_on_branch(root, name) {
        return;
    }
    std::fs::write(root.join(name), "local").unwrap();
    let old = rev(root, "main");
    let work = lock_dir();

    let result = merge(root, &work);

    assert!(
        matches!(result, MergeResult::Blocked(_)),
        "expected Blocked, got {result:?}"
    );
    assert_eq!(std::fs::read(root.join(name)).unwrap(), b"local");
    assert_eq!(rev(root, "main"), old);
}

#[test]
fn a_branch_switch_before_the_fast_forward_records_no_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "from branch")]);
    let old = rev(root, "main");
    super::fast_forward::inject_before_fast_forward(|checkout: &Path| {
        git_ok(checkout, &["checkout", "-q", "-b", "other"]);
    });
    let lock = lock_dir();

    let result = merge(root, &lock);

    assert!(
        !matches!(result, MergeResult::Success { .. }),
        "expected no Success, got {result:?}"
    );
    assert_eq!(rev(root, "main"), old);
    let accepted = crate::git::target_guard::accepted_tip(lock.path(), "main").unwrap();
    assert!(
        accepted.is_none() || accepted.as_deref() == Some(old.as_str()),
        "the guard recorded {accepted:?}, main is still {old}"
    );
}
