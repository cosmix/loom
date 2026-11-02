use super::*;
use crate::git::merge::test_support::{commit_file, git_ok, init_repo, rev, stage_branch};
use std::os::unix::fs::symlink;

fn overlap(paths: &[&str]) -> MergeBlock {
    MergeBlock::UncommittedOverlap {
        paths: paths.iter().map(|p| p.to_string()).collect(),
    }
}

#[test]
fn git_messages_that_list_paths_become_an_overlap() {
    let cases = [
        (
            "error: Your local changes to the following files would be overwritten by merge:\n\ta\n\tb c\nPlease commit your changes or stash them before you merge.\nAborting\n",
            overlap(&["a", "b c"]),
        ),
        (
            "error: The following untracked working tree files would be overwritten by merge:\n\tb\nPlease move or remove them before you merge.\nAborting\n",
            overlap(&["b"]),
        ),
        (
            "error: The following untracked working tree files would be removed by merge:\n\tt\nPlease move or remove them before you merge.\nAborting\n",
            overlap(&["t"]),
        ),
        (
            "error: Updating the following directories would lose untracked files in them:\n\td\n\nAborting\n",
            overlap(&["d"]),
        ),
    ];
    for (stderr, expected) in cases {
        assert_eq!(refused_block(stderr), expected, "{stderr}");
    }
}

#[test]
fn a_message_without_paths_becomes_a_one_line_detail() {
    let block =
        refused_block("hint: Diverging\nhint:\nfatal: Not possible to fast-forward, aborting.\n");
    assert_eq!(
        block,
        MergeBlock::FastForwardRefused {
            detail: "hint: Diverging hint: fatal: Not possible to fast-forward, aborting."
                .to_string()
        }
    );

    let MergeBlock::FastForwardRefused { detail } = refused_block(&"word ".repeat(200)) else {
        panic!("expected FastForwardRefused");
    };
    assert_eq!(detail.chars().count(), DETAIL_LIMIT);
}

#[test]
fn a_refusal_naming_paths_is_an_overlap_and_changes_nothing() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join("b.txt"), "mine").unwrap();

    let advance = fast_forward(root, (&old, &new), &[], None).unwrap();

    assert_eq!(advance, Advance::Blocked(overlap(&["b.txt"])));
    assert_eq!(rev(root, "main"), old);
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "mine");
}

#[test]
fn a_refusal_after_the_target_moved_is_target_moved() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    commit_file(root, "c.txt", "moved", "main moves");

    let advance = fast_forward(root, (&old, &new), &[], None).unwrap();

    assert_eq!(advance, Advance::Blocked(MergeBlock::TargetMoved));
}

/// The refusal comes before git touches anything. Under `WorktreeGit::pinned_main`
/// git would lock a private index, so it would move the target and update the
/// files, and only publishing the index afterwards would find the operator's lock.
#[test]
fn a_held_index_lock_refuses_and_moves_nothing() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join(".git/index.lock"), "").unwrap();

    let advance = fast_forward(root, (&old, &new), &[], None).unwrap();

    assert_eq!(rev(root, "main"), old);
    assert!(!root.join("b.txt").exists());
    let Advance::Blocked(MergeBlock::FastForwardRefused { detail }) = advance else {
        panic!("expected FastForwardRefused, got {advance:?}");
    };
    assert!(detail.contains("index.lock"), "{detail}");
}

/// Git matches `includeIf "gitdir:"` against the git directory's path, so a
/// private `GIT_DIR` would lose this setting and write other bytes to the checkout.
#[test]
fn an_include_for_this_git_directory_shapes_the_files_the_fast_forward_writes() {
    let repo = init_repo();
    let root = repo.path().canonicalize().unwrap();
    stage_branch(&root, "s1", &[("b.txt", "line\n")]);
    let (old, new) = (rev(&root, "main"), rev(&root, "loom/s1"));
    let include = root.join(".git/operator-include");
    std::fs::write(&include, "[core]\n\tautocrlf = true\n").unwrap();
    let key = format!("includeIf.gitdir:{}/.git.path", root.display());
    git_ok(&root, &["config", &key, include.to_str().unwrap()]);

    let advance = fast_forward(&root, (&old, &new), &[], None).unwrap();

    assert_eq!(advance, Advance::Advanced { stash: None });
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "line\r\n"
    );
}

#[test]
fn a_refusal_without_paths_is_fast_forward_refused() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "branch")]);
    let new = rev(root, "loom/s1");
    commit_file(root, "c.txt", "diverges", "main diverges");
    let old = rev(root, "main");

    let advance = fast_forward(root, (&old, &new), &[], None).unwrap();

    let Advance::Blocked(MergeBlock::FastForwardRefused { detail }) = advance else {
        panic!("expected FastForwardRefused, got {advance:?}");
    };
    assert!(detail.contains("fast-forward"), "{detail}");
    git_ok(root, &["status", "--porcelain"]);
}

#[test]
fn a_refused_fast_forward_reports_occupied_rollback_paths() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("a.txt", "branch a"), ("b.txt", "branch b")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join("a.txt"), "operator").unwrap();
    std::fs::write(root.join("outside.txt"), "outside").unwrap();
    symlink("outside.txt", root.join("b.txt")).unwrap();
    let _guard = failpoint::inject(failpoint::Failures {
        fast_forward: true,
        ..Default::default()
    });

    let advance =
        fast_forward(root, (&old, &new), &["a.txt".into(), "b.txt".into()], None).unwrap();

    let Advance::Blocked(MergeBlock::CheckoutRestoreIncomplete {
        source_rev,
        failures,
        changes,
    }) = advance
    else {
        panic!("expected incomplete rollback, got {advance:?}");
    };
    assert_eq!(source_rev, new);
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.txt", "b.txt"]
    );
    assert_eq!(changes, RollbackChanges::NoBackup);
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"operator");
    assert_eq!(std::fs::read(root.join("outside.txt")).unwrap(), b"outside");
    assert_eq!(rev(root, "main"), old);
}

/// A refused fast-forward with `f.txt` edited and stashed, and `a.txt`, which
/// the merge replaces, occupied so that rollback cannot restore it.
fn refused_with_a_stash_and_a_blocked_rollback(pop_fails: bool) -> (tempfile::TempDir, Advance) {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", "base\n", "f");
    stage_branch(root, "s1", &[("new.txt", "branch")]);
    let (old, new) = (rev(root, "main"), rev(root, "loom/s1"));
    std::fs::write(root.join("f.txt"), "edited\n").unwrap();
    git_ok(root, &["stash", "push", "-q"]);
    std::fs::write(root.join("new.txt"), "operator").unwrap();
    let _guard = failpoint::inject(failpoint::Failures {
        fast_forward: true,
        pop: pop_fails,
        ..Default::default()
    });
    let backup = Some(StashedChanges {
        commit: rev(root, "stash@{0}"),
        backup_ref: "refs/loom/autostash/s1-1-abc".to_string(),
    });
    let advance = fast_forward(root, (&old, &new), &["new.txt".into()], backup).unwrap();
    (repo, advance)
}

#[test]
fn a_popped_stash_is_not_reported_as_still_stashed() {
    let (repo, advance) = refused_with_a_stash_and_a_blocked_rollback(false);
    let Advance::Blocked(block) = advance else {
        panic!("expected a block, got {advance:?}");
    };

    let text = block.to_string();

    assert_eq!(
        std::fs::read(repo.path().join("f.txt")).unwrap(),
        b"edited\n"
    );
    assert!(!text.contains("remain"), "{text}");
    assert!(text.contains("put back in the checkout"), "{text}");
    assert!(
        matches!(
            block,
            MergeBlock::CheckoutRestoreIncomplete {
                changes: RollbackChanges::Restored { .. },
                ..
            }
        ),
        "{block:?}"
    );
    assert!(text.contains("refs/loom/autostash/s1-1-abc"), "{text}");
}

#[test]
fn a_stash_whose_pop_failed_is_reported_as_stashed() {
    let (repo, advance) = refused_with_a_stash_and_a_blocked_rollback(true);
    let Advance::Blocked(block) = advance else {
        panic!("expected a block, got {advance:?}");
    };

    let text = block.to_string();

    assert_eq!(std::fs::read(repo.path().join("f.txt")).unwrap(), b"base\n");
    assert!(
        text.contains("remain in refs/loom/autostash/s1-1-abc"),
        "{text}"
    );
    assert!(text.contains("`git stash list` entry"), "{text}");
    assert!(!text.contains("top"), "{text}");
    assert!(text.contains("refs/loom/autostash/s1-1-abc"), "{text}");
}
