//! What a stopped rollback reports about the operator's changes, and a reset
//! that fails after the stash exists.

use super::*;

#[test]
fn a_rollback_before_any_stash_says_the_changes_stayed_in_the_checkout() {
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

    let MergeResult::Blocked(block) = merge(root) else {
        panic!("expected a block");
    };

    let text = block.to_string();
    assert_eq!(read(root), edited);
    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert!(!text.contains("remain"), "{text}");
    assert!(
        text.contains("were not stashed and stay in the checkout"),
        "{text}"
    );
    assert!(text.contains("refs/loom/autostash/s1-"), "{text}");
}

#[test]
fn a_reset_that_changed_nothing_says_the_changes_are_still_in_the_checkout() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let (head, edited) = (rev(root, "main"), read(root));
    BEFORE_STASH_RESET.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|repo| {
            std::fs::write(repo.join(".git/index.lock"), b"").unwrap();
        }));
    });

    let MergeResult::Blocked(block) = merge(root) else {
        panic!("expected a block");
    };

    let MergeBlock::StashResetRefused { backup_ref } = &block else {
        panic!("expected StashResetRefused, got {block:?}");
    };
    std::fs::remove_file(root.join(".git/index.lock")).unwrap();
    assert_eq!(rev(root, "main"), head);
    assert_eq!(read(root), edited, "the failed reset changed nothing");
    assert_eq!(
        git_out(root, &["rev-parse", "stash@{0}"]),
        git_out(root, &["rev-parse", backup_ref])
    );
    let text = block.to_string();
    assert!(text.contains("still in the checkout"), "{text}");
    assert!(
        text.contains(&format!("git update-ref -d {backup_ref}")),
        "{text}"
    );
    assert!(!text.contains("restore them"), "{text}");
    git_ok(root, &["stash", "drop", "--quiet"]);
    git_ok(root, &["update-ref", "-d", backup_ref]);
    assert_eq!(read(root), edited, "the named remedy keeps the changes");
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A reset that restores one file and then fails on the next leaves the changes
/// in neither place alone: the text must send the operator to the stash.
#[test]
fn a_reset_that_failed_partway_names_the_stash() {
    // SAFETY: geteuid() is always safe to call and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        return; // Root writes through the read-only directory that fails the reset.
    }
    let repo = init_repo();
    let root = repo.path();
    std::fs::create_dir(root.join("locked")).unwrap();
    commit_file(root, "locked/l.txt", "base\n", "locked");
    overlapping_checkout(root);
    std::fs::write(root.join("locked/l.txt"), "edited\n").unwrap();
    let locked = root.join("locked");
    let to_lock = locked.clone();
    BEFORE_STASH_RESET.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |_| set_mode(&to_lock, 0o555)));
    });

    let result = merge(root);

    set_mode(&locked, 0o755);
    let MergeResult::Blocked(MergeBlock::StashNotRestored { backup_ref }) = &result else {
        panic!("expected StashNotRestored, got {result:?}");
    };
    assert_eq!(read(root), LINES, "the reset restored this file");
    assert_eq!(
        git_out(root, &["rev-parse", "stash@{0}"]),
        git_out(root, &["rev-parse", backup_ref])
    );
    let saved = git_out(root, &["show", &format!("{backup_ref}:f.txt")]);
    assert!(
        saved.contains("l10b"),
        "the stash holds the edit the reset took"
    );
}

#[test]
fn a_rollback_after_a_reset_that_changed_nothing_says_where_the_changes_are() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "f.txt", LINES, "f");
    let branch_text = LINES.replace("l1\n", "l1b\n");
    stage_branch(
        root,
        "s1",
        &[("f.txt", &branch_text), ("new.txt", "branch")],
    );
    std::fs::write(root.join("f.txt"), LINES.replace("l10\n", "l10b\n")).unwrap();
    std::fs::write(root.join("new.txt"), "branch").unwrap();
    BEFORE_STASH_RESET.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|repo| {
            std::fs::write(repo.join(".git/index.lock"), b"").unwrap();
            std::fs::write(repo.join("new.txt"), "operator").unwrap();
        }));
    });

    let result = merge(root);

    std::fs::remove_file(root.join(".git/index.lock")).unwrap();
    let MergeResult::Blocked(block) = &result else {
        panic!("expected a block, got {result:?}");
    };
    assert!(
        matches!(
            block,
            MergeBlock::CheckoutRestoreIncomplete {
                changes: RollbackChanges::ResetRefused { .. },
                ..
            }
        ),
        "{block:?}"
    );
    assert!(
        block.to_string().contains("still in the checkout"),
        "{block}"
    );
    assert!(read(root).contains("l10b"));
}
