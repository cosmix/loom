//! The stash path of the fast-forward, with git failures injected at the
//! points git does not fail at by itself.

use super::*;
use crate::git::merge::checkout_files::inject_before_restore;
use crate::git::merge::fast_forward::failpoint::{inject, Failures};
use crate::git::merge::test_support::{
    commit_file, git_ok, git_out, init_repo, lock_dir, rev, stage_branch,
};
use crate::git::merge::{merge_stage, MergeGate, MergeResult};
use std::cell::RefCell;

type BeforeStashPush = Box<dyn FnOnce(&Path)>;

thread_local! {
    static BEFORE_STASH_PUSH: RefCell<Option<BeforeStashPush>> = RefCell::new(None);
}

pub(super) fn before_stash_push(repo: &Path) -> bool {
    BEFORE_STASH_PUSH.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook(repo);
            true
        } else {
            false
        }
    })
}

thread_local! {
    static BEFORE_STASH_RESET: RefCell<Option<BeforeStashPush>> = RefCell::new(None);
}

pub(super) fn before_stash_reset(repo: &Path) {
    BEFORE_STASH_RESET.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook(repo);
        }
    });
}

mod rollback;
mod stash_entry;

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

/// `f.txt` has a local edit at its end and the branch edits its
/// start: the overlap is clean and goes through the stash.
fn overlapping_checkout(root: &Path) {
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(root, "s1", &[("f.txt", branch_text.as_str())]);
    std::fs::write(root.join("f.txt"), LINES.replace("l10\n", "l10b\n")).unwrap();
}

fn merge(root: &Path) -> MergeResult {
    let work = lock_dir();
    merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap()
}

fn read(root: &Path) -> String {
    std::fs::read_to_string(root.join("f.txt")).unwrap()
}

#[test]
fn a_refused_fast_forward_whose_pop_fails_names_the_stash() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let head = rev(root, "main");
    let _guard = inject(Failures {
        fast_forward: true,
        pop: true,
        ..Failures::default()
    });

    let MergeResult::Blocked(MergeBlock::StashNotRestored { backup_ref }) = merge(root) else {
        panic!("expected StashNotRestored");
    };

    assert_eq!(rev(root, "main"), head);
    assert!(git_out(root, &["stash", "list"]).contains("stash@{0}"));
    git_out(root, &["rev-parse", "--verify", &backup_ref]);
    assert!(backup_ref.starts_with("refs/loom/autostash/s1-"));
    assert_eq!(read(root), LINES, "the changes are only in the stash");
}

#[test]
fn a_refused_fast_forward_with_a_working_pop_puts_everything_back() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let (head, edited) = (rev(root, "main"), read(root));
    let _guard = inject(Failures {
        fast_forward: true,
        pop: false,
        ..Failures::default()
    });

    let result = merge(root);

    assert!(
        matches!(
            &result,
            MergeResult::Blocked(MergeBlock::FastForwardRefused { .. })
        ),
        "{result:?}"
    );
    assert_eq!(rev(root, "main"), head);
    assert_eq!(read(root), edited);
    assert_eq!(git_out(root, &["stash", "list"]), "");
}

#[test]
fn stash_failure_reports_a_recreated_untracked_file_and_keeps_the_backup() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(
        root,
        "s1",
        &[("f.txt", &branch_text), ("new.txt", "branch")],
    );
    let edited = LINES.replace("l10\n", "l10b\n");
    std::fs::write(root.join("f.txt"), &edited).unwrap();
    std::fs::write(root.join("new.txt"), "branch").unwrap();
    BEFORE_STASH_PUSH.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|repo| {
            std::fs::write(repo.join("new.txt"), "operator").unwrap();
        }));
    });

    let result = merge(root);

    let MergeResult::Blocked(MergeBlock::CheckoutRestoreIncomplete {
        failures,
        changes: RollbackChanges::InCheckout { backup_ref },
        ..
    }) = result
    else {
        panic!("expected incomplete rollback, got {result:?}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "new.txt");
    assert_eq!(std::fs::read(root.join("new.txt")).unwrap(), b"operator");
    assert_eq!(read(root), edited);
    git_out(root, &["rev-parse", "--verify", &backup_ref]);
}

#[test]
fn partial_removal_reports_an_incomplete_rollback() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("first.txt", "original")]);
    let old = rev(root, "main");
    let branch = rev(root, "loom/s1");
    let tree = git_out(root, &["rev-parse", "loom/s1^{tree}"]);
    std::fs::write(root.join("first.txt"), "original").unwrap();
    std::fs::create_dir(root.join("cannot-remove")).unwrap();
    inject_before_restore(|repo| {
        std::fs::write(repo.join("first.txt"), "operator").unwrap();
    });
    let mut pending = PendingMerge::new(root, &tree, [&old, &branch], "merge");

    let advance = land_removing(
        root,
        &mut pending,
        vec!["first.txt".into(), "cannot-remove".into()],
    )
    .unwrap();

    let Advance::Blocked(MergeBlock::CheckoutRestoreIncomplete { failures, .. }) = advance else {
        panic!("expected incomplete rollback, got {advance:?}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "first.txt");
    assert_eq!(std::fs::read(root.join("first.txt")).unwrap(), b"operator");
    assert_eq!(rev(root, "main"), old);
}

#[test]
fn a_fast_forward_that_errors_leaves_the_stash_unpopped() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let head = rev(root, "main");
    // The pop would work: it must not be tried on a tree of unknown state.
    let _guard = inject(Failures {
        fast_forward_error: true,
        ..Failures::default()
    });

    let MergeResult::Blocked(MergeBlock::StashNotRestored { backup_ref }) = merge(root) else {
        panic!("expected StashNotRestored");
    };

    assert_eq!(rev(root, "main"), head);
    git_out(root, &["rev-parse", "--verify", &backup_ref]);
    assert!(git_out(root, &["stash", "list"]).contains("stash@{0}"));
    assert_eq!(read(root), LINES, "the changes are only in the stash");
}

#[test]
fn a_fast_forward_that_errors_without_a_stash_is_a_refusal() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("g.txt", "new\n")]);
    let head = rev(root, "main");
    let _guard = inject(Failures {
        fast_forward_error: true,
        ..Failures::default()
    });

    let result = merge(root);

    let MergeResult::Blocked(MergeBlock::FastForwardRefused { detail }) = &result else {
        panic!("expected FastForwardRefused, got {result:?}");
    };
    assert!(detail.contains("simulated spawn failure"), "{detail}");
    assert_eq!(rev(root, "main"), head);
}

#[test]
fn backup_ref_names_differ_for_different_stashes_in_one_second() {
    let a = backup_ref_name("s1", "aaaaaaaaaaaaaaaaaaaaaaaa");
    let b = backup_ref_name("s1", "bbbbbbbbbbbbbbbbbbbbbbbb");
    assert_ne!(a, b);
    assert!(a.starts_with("refs/loom/autostash/s1-") && a.ends_with("-aaaaaaaaaaaa"));
}

#[test]
fn an_existing_backup_ref_is_not_overwritten() {
    let repo = init_repo();
    let root = repo.path();
    let first = rev(root, "main");
    commit_file(root, "x.txt", "x", "second");
    let second = rev(root, "main");
    let name = "refs/loom/autostash/s1-1-aaaaaaaaaaaa";

    create_backup_ref(root, name, &first).unwrap();

    assert!(create_backup_ref(root, name, &second).is_err());
    assert_eq!(git_out(root, &["rev-parse", name]), first);
}

#[test]
fn a_landed_merge_whose_pops_fail_is_a_success_with_the_stash_kept() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let head = rev(root, "main");
    let _guard = inject(Failures {
        fast_forward: false,
        pop: true,
        ..Failures::default()
    });

    let MergeResult::Success {
        stash: Some(stash), ..
    } = merge(root)
    else {
        panic!("expected a landed merge with a stash outcome");
    };

    assert!(!stash.restored);
    assert_ne!(rev(root, "main"), head);
    assert_eq!(rev(root, "main^1"), head);
    git_out(root, &["rev-parse", "--verify", &stash.backup_ref]);
    assert!(git_out(root, &["stash", "list"]).contains("stash@{0}"));
    assert!(read(root).starts_with("l1b\n") && !read(root).contains("l10b"));
}

#[test]
fn a_failing_stash_snapshot_blocks_and_leaves_the_checkout_alone() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("n.txt", "branch")]);
    // An intent-to-add entry on a path the branch adds makes the stash fail.
    std::fs::write(root.join("n.txt"), "mine").unwrap();
    git_ok(root, &["add", "-N", "n.txt"]);
    let head = rev(root, "main");

    let result = merge(root);

    let MergeResult::Blocked(MergeBlock::UncommittedOverlap { paths }) = result else {
        panic!("expected UncommittedOverlap, got {result:?}");
    };
    assert_eq!(paths, vec!["n.txt".to_string()]);
    assert_eq!(rev(root, "main"), head);
    assert_eq!(std::fs::read_to_string(root.join("n.txt")).unwrap(), "mine");
    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert_eq!(git_out(root, &["for-each-ref", "refs/loom"]), "");
}

#[test]
fn an_errored_fast_forward_writes_removed_files_back_only_where_absent() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(
        root,
        "s1",
        &[("a.txt", "branch a\n"), ("b.txt", "branch b\n")],
    );
    // `a.txt` came back while the fast-forward ran; `b.txt` is still gone.
    std::fs::write(root.join("a.txt"), "operator's a\n").unwrap();
    let removed = vec!["a.txt".to_string(), "b.txt".to_string()];
    let advance = errored_fast_forward(root, &removed);

    let Advance::Blocked(MergeBlock::CheckoutRestoreIncomplete {
        failures, changes, ..
    }) = advance
    else {
        panic!("expected incomplete rollback, got {advance:?}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, "a.txt");
    assert_eq!(
        changes,
        RollbackChanges::Stashed {
            backup_ref: "refs/loom/autostash/s1-1-abc".to_string()
        }
    );
    assert_eq!(
        std::fs::read(root.join("a.txt")).unwrap(),
        b"operator's a\n",
        "a file already present is untouched"
    );
    assert_eq!(
        std::fs::read(root.join("b.txt")).unwrap(),
        b"branch b\n",
        "an absent file is restored"
    );
}

/// `main` fast-forwarded to `loom/s1` with the fast-forward itself failing.
fn errored_fast_forward(root: &Path, removed: &[String]) -> Advance {
    let _guard = inject(Failures {
        fast_forward_error: true,
        ..Failures::default()
    });
    fast_forward(
        root,
        (&rev(root, "main"), &rev(root, "loom/s1")),
        removed,
        Some(StashedChanges {
            commit: "unused: the errored fast-forward pops nothing".to_string(),
            backup_ref: "refs/loom/autostash/s1-1-abc".to_string(),
        }),
    )
    .unwrap()
}
