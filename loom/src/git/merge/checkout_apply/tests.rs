//! Landing in the operator's checkout, with git failures injected at the
//! points git does not fail at by itself.

use super::*;
use crate::git::merge::fast_forward::failpoint::{inject, Failures};
use crate::git::merge::fast_forward::inject_before_fast_forward;
use crate::git::merge::test_support::{
    commit_file, git_ok, git_out, init_repo, lock_dir, rev, stage_branch,
};
use crate::git::merge::{merge_stage, MergeGate, MergeResult};

const LINES: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n";

/// `f.txt` has a local edit at its end and the branch edits its start: a
/// tracked overlap, which blocks even though git could merge it.
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
fn a_fast_forward_that_fails_to_start_is_a_refusal() {
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
fn an_intent_to_add_entry_on_a_path_the_branch_adds_blocks_and_keeps_it() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("n.txt", "branch")]);
    // The checkout's intent-to-add entry sits on a path the branch adds.
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
fn a_tracked_overlap_blocks_and_keeps_the_local_bytes() {
    let repo = init_repo();
    let root = repo.path();
    overlapping_checkout(root);
    let (head, edited) = (rev(root, "main"), read(root));

    let result = merge(root);

    let MergeResult::Blocked(MergeBlock::UncommittedOverlap { paths }) = result else {
        panic!("expected UncommittedOverlap, got {result:?}");
    };
    assert_eq!(paths, vec!["f.txt".to_string()]);
    assert_eq!(read(root), edited);
    assert_eq!(git_out(root, &["stash", "list"]), "");
    assert_eq!(git_out(root, &["for-each-ref", "refs/loom"]), "");
    assert_eq!(rev(root, "main"), head);
}

/// `x*` is ignored on `main`, so the raw-named file the branch adds with
/// `add -f` is an ignored file in the checkout that git would overwrite. The
/// control-path gate is bypassed: enforced, it blocks the name with
/// `FastForwardRefused` first, so the landing under test never runs.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_non_utf8_path_the_merge_adds_blocks_and_keeps_the_ignored_file() {
    use crate::test_git::git_command;
    use std::os::unix::ffi::OsStrExt;
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, ".gitignore", "x*\n", "ignore x");
    let name = std::ffi::OsStr::from_bytes(b"x\xff");
    git_ok(root, &["checkout", "-q", "-b", "loom/s1", "main"]);
    if std::fs::write(root.join(name), "incoming").is_err() {
        return;
    }
    let added = git_command(root)
        .args(["add", "-f", "--"])
        .arg(name)
        .output()
        .unwrap();
    assert!(added.status.success(), "{added:?}");
    git_ok(root, &["commit", "-q", "-m", "raw name"]);
    git_ok(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join(name), "local").unwrap();
    let head = rev(root, "main");

    let work = lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Bypass).unwrap();

    assert!(
        matches!(
            result,
            MergeResult::Blocked(MergeBlock::FastForwardRefused { .. })
        ),
        "expected FastForwardRefused, got {result:?}"
    );
    assert_eq!(std::fs::read(root.join(name)).unwrap(), b"local");
    assert_eq!(rev(root, "main"), head);
}

#[test]
fn a_branch_switch_before_the_fast_forward_records_no_merge() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("b.txt", "b")]);
    let head = rev(root, "main");
    inject_before_fast_forward(|repo| {
        git_ok(repo, &["checkout", "-q", "-b", "other"]);
    });
    let work = lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    assert!(!matches!(result, MergeResult::Success { .. }), "{result:?}");
    let MergeResult::Blocked(MergeBlock::FastForwardRefused { detail }) = &result else {
        panic!("expected FastForwardRefused, got {result:?}");
    };
    assert!(detail.contains("refs/heads/other"), "{detail}");
    assert_eq!(rev(root, "main"), head);
    assert_ne!(rev(root, "other"), head);
    let accepted = crate::git::target_guard::accepted_tip(work.path(), "main").unwrap();
    assert!(accepted.is_none() || accepted.as_deref() == Some(head.as_str()));
}

#[test]
fn a_branch_switch_that_strands_displaced_files_names_the_quarantine_and_the_ref() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("u.txt", "incoming")]);
    // Identical to the incoming blob, so the landing moves it aside; the
    // fast-forward on `other` then writes the tracked file over its path.
    std::fs::write(root.join("u.txt"), "incoming").unwrap();
    let head = rev(root, "main");
    inject_before_fast_forward(|repo| {
        git_ok(repo, &["checkout", "-q", "-b", "other"]);
    });

    let result = merge(root);

    let MergeResult::Blocked(
        block @ MergeBlock::CheckoutRestoreIncomplete {
            quarantine, detail, ..
        },
    ) = &result
    else {
        panic!("expected CheckoutRestoreIncomplete, got {result:?}");
    };
    assert!(quarantine.exists(), "{}", quarantine.display());
    assert!(detail
        .as_deref()
        .unwrap_or_default()
        .contains("refs/heads/other"));
    let text = block.to_string();
    assert!(text.contains(&quarantine.display().to_string()), "{text}");
    assert!(text.contains("refs/heads/other"), "{text}");
    assert_eq!(rev(root, "main"), head);
}
