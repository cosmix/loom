//! `merge_stage` refuses while a quarantine of the stage exists, and only for
//! that stage's attempt directories.

use super::*;
use crate::git::merge::test_support::{init_repo, lock_dir, rev, snapshot, stage_branch};
use crate::git::merge::{merge_stage, MergeGate, MergeResult};

/// A repository with branch `loom/s1` adding `b.txt`.
fn repo_with_stage() -> tempfile::TempDir {
    let repo = init_repo();
    stage_branch(repo.path(), "s1", &[("b.txt", "from branch")]);
    repo
}

/// Create `<git dir>/loom-displaced/<name>/` holding one operator file.
fn quarantine(root: &Path, name: &str) -> PathBuf {
    let dir = git_dir_for_repo_path(root)
        .unwrap()
        .join(QUARANTINE_ROOT)
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.txt"), "operator bytes").unwrap();
    dir
}

fn merge(root: &Path) -> MergeResult {
    let work = lock_dir();
    merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap()
}

#[test]
fn a_held_quarantine_blocks_the_merge_and_changes_nothing() {
    let repo = repo_with_stage();
    let root = repo.path();
    let held = quarantine(root, "s1-123");
    let main = rev(root, "main");
    let before = snapshot(root);

    let result = merge(root);

    match result {
        MergeResult::Blocked(MergeBlock::CheckoutRestoreIncomplete {
            quarantine,
            failures,
            detail,
        }) => {
            assert_eq!(quarantine, held);
            assert!(failures.is_empty());
            assert!(detail.unwrap().contains("delete the quarantine directory"));
        }
        other => panic!("expected CheckoutRestoreIncomplete, got {other:?}"),
    }
    assert_eq!(rev(root, "main"), main);
    assert_eq!(snapshot(root), before);
    assert_eq!(
        std::fs::read_to_string(held.join("b.txt")).unwrap(),
        "operator bytes"
    );
}

#[test]
fn the_merge_lands_once_the_quarantine_is_deleted() {
    let repo = repo_with_stage();
    let root = repo.path();
    let held = quarantine(root, "s1-123");
    assert!(matches!(merge(root), MergeResult::Blocked(_)));

    std::fs::remove_dir_all(&held).unwrap();

    assert!(matches!(merge(root), MergeResult::Success { .. }));
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "from branch"
    );
}

#[test]
fn another_stage_whose_name_starts_with_the_id_is_ignored() {
    let repo = repo_with_stage();
    let root = repo.path();
    quarantine(root, "s1x-123");
    quarantine(root, "s10-5");

    assert!(matches!(merge(root), MergeResult::Success { .. }));
}

#[test]
fn a_suffix_that_is_not_all_digits_is_ignored() {
    let repo = repo_with_stage();
    let root = repo.path();
    quarantine(root, "s1-abc");
    quarantine(root, "s1-");
    quarantine(root, "s1-2-123");

    assert!(matches!(merge(root), MergeResult::Success { .. }));
}

#[test]
fn an_unreadable_root_fails_closed_without_a_block() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join(QUARANTINE_ROOT);
    std::fs::write(&root, "not a directory").unwrap();

    let message = format!("{:#}", held_quarantine(&root, "s1").unwrap_err());

    assert!(message.contains(&root.display().to_string()), "{message}");
    assert!(
        message.contains("could not be read as a directory"),
        "{message}"
    );
    assert!(message.contains("permissions or type"), "{message}");
    assert!(message.contains("do not delete it"), "{message}");
    assert!(
        !message.contains("delete the quarantine directory"),
        "{message}"
    );
    assert_eq!(
        held_quarantine(&temp.path().join("missing"), "s1").unwrap(),
        None
    );
}

#[test]
fn merge_stage_errors_on_an_unreadable_root_and_lands_once_it_is_fixed() {
    let repo = repo_with_stage();
    let root = repo.path();
    let quarantine_root = git_dir_for_repo_path(root).unwrap().join(QUARANTINE_ROOT);
    std::fs::write(&quarantine_root, "not a directory").unwrap();
    let main = rev(root, "main");
    let work = lock_dir();

    let error = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains(&quarantine_root.display().to_string()),
        "{message}"
    );
    assert!(
        !message.contains("delete the quarantine directory"),
        "{message}"
    );
    assert_eq!(rev(root, "main"), main);

    std::fs::remove_file(&quarantine_root).unwrap();

    assert!(matches!(merge(root), MergeResult::Success { .. }));
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "from branch"
    );
}

#[test]
fn a_named_entry_is_held_even_when_it_is_not_a_directory() {
    let repo = repo_with_stage();
    let root = repo.path();
    let entry = git_dir_for_repo_path(root)
        .unwrap()
        .join(QUARANTINE_ROOT)
        .join("s1-9");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::fs::write(&entry, "not a directory").unwrap();

    match merge(root) {
        MergeResult::Blocked(MergeBlock::CheckoutRestoreIncomplete { quarantine, .. }) => {
            assert_eq!(quarantine, entry);
        }
        other => panic!("expected CheckoutRestoreIncomplete, got {other:?}"),
    }
}

#[test]
fn attempt_names_need_the_exact_id_then_digits() {
    assert!(names_attempt("s1-123", "s1"));
    assert!(names_attempt("s-2-123", "s-2"));
    assert!(!names_attempt("s-2-123", "s"));
    assert!(!names_attempt("s1x-123", "s1"));
    assert!(!names_attempt("s10-5", "s1"));
    assert!(!names_attempt("s1-abc", "s1"));
    assert!(!names_attempt("s1-", "s1"));
    assert!(!names_attempt("s1", "s1"));
}
