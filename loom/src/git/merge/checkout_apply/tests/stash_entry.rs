//! The stash list is shared with every other user of `refs/stash`, so the pop
//! after a fast-forward must find the merge's own entry.

use super::*;
use crate::git::merge::StashReapply;

/// A stash entry made the way a stage session makes one: from a linked
/// worktree, whose `git stash` writes the same `refs/stash`. Returns its commit.
fn stash_from_a_session(repo: &Path) -> String {
    let session = lock_dir();
    let dir = session.path().join("session");
    let dir_arg = dir.to_str().unwrap();
    git_ok(
        repo,
        &["worktree", "add", "-q", "-b", "session", dir_arg, "main"],
    );
    std::fs::write(dir.join("a.txt"), "the session's work\n").unwrap();
    git_ok(&dir, &["stash", "push", "-q"]);
    git_ok(repo, &["worktree", "remove", "--force", dir_arg]);
    git_out(repo, &["rev-parse", "stash@{0}"])
}

fn before_the_reset(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_STASH_RESET.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// `f.txt` edited, its stash made, and `after` run before the fast-forward.
fn landed(root: &Path, after: impl FnOnce(&Path) + 'static) -> StashReapply {
    overlapping_checkout(root);
    before_the_reset(after);
    let result = merge(root);
    let MergeResult::Success {
        stash: Some(stash), ..
    } = result
    else {
        panic!("expected a landed merge with a stash outcome, got {result:?}");
    };
    stash
}

#[test]
fn another_entry_on_top_is_neither_applied_nor_dropped() {
    let repo = init_repo();
    let root = repo.path();
    let session_entry = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let recorded = session_entry.clone();

    let stash = landed(root, move |repo| {
        *recorded.borrow_mut() = stash_from_a_session(repo);
    });

    assert!(stash.restored);
    assert!(read(root).starts_with("l1b\n") && read(root).contains("l10b"));
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"seed");
    assert_eq!(
        git_out(root, &["stash", "list", "--format=%H"]),
        *session_entry.borrow(),
        "the merge's own entry is the one that went"
    );
}

#[test]
fn an_entry_that_is_gone_is_refused_and_the_other_entry_stays() {
    let repo = init_repo();
    let root = repo.path();

    let stash = landed(root, |repo| {
        git_ok(repo, &["stash", "clear"]);
        stash_from_a_session(repo);
    });

    assert!(!stash.restored);
    assert!(!read(root).contains("l10b"), "nothing was reapplied");
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"seed");
    assert_eq!(git_out(root, &["stash", "list"]).lines().count(), 1);
    let saved = git_out(root, &["show", &format!("{}:f.txt", stash.backup_ref)]);
    assert!(saved.contains("l10b"), "the backup ref keeps the work");
}

/// The remedy the operator text names works on what the block leaves.
#[test]
fn the_named_remedy_restores_the_work_from_the_backup_ref() {
    let repo = init_repo();
    let root = repo.path();

    let stash = landed(root, |repo| git_ok(repo, &["stash", "clear"]));

    assert!(stash
        .notice()
        .contains(&format!("git stash apply {}", stash.backup_ref)));
    git_ok(root, &["stash", "apply", "--quiet", &stash.backup_ref]);
    assert!(read(root).starts_with("l1b\n") && read(root).contains("l10b"));
}

#[test]
fn a_refused_fast_forward_pops_its_own_entry_under_another() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let edited = read(root);
    before_the_reset(|repo| {
        stash_from_a_session(repo);
    });
    let _guard = inject(Failures {
        fast_forward: true,
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
    assert_eq!(read(root), edited);
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"seed");
    assert_eq!(git_out(root, &["stash", "list"]).lines().count(), 1);
}
