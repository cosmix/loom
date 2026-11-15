//! Loom's `reference-transaction` hook under real git: which moves of the
//! guarded target it attests, which it refuses, and the `core.hooksPath`
//! settings that make git skip it (and turn attestation off).
//!
//! Every git command here runs with the user's and system's config shut out
//! and sets or removes `LOOM_SESSION_ID` explicitly.

use loom::git::configured_hooks_path;
use loom::git::target_guard::{attestation_mode, AttestationMode, REFS_FILE};

#[path = "target_ref_hook_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "test_support/mod.rs"]
mod test_support;
use support::{
    commit, git, ledger, ledger_dir_is_empty, make_ledger_unwritable, move_main_to_stage_tip, repo,
    run, run_with_input, stderr, tip, Repo,
};

#[test]
fn stage_branch_commit_writes_no_ledger_line() {
    let repo = repo();
    commit(&repo.wt, "b.txt");
    assert_eq!(ledger(&repo.work), "");
}

#[test]
fn aborted_transaction_leaves_an_attest_and_a_matching_abort() {
    let repo = repo();
    let (main, to) = (&repo.main, &repo.stage_tip);
    let input = format!("start\nupdate refs/heads/main {to}\nprepare\nabort\n");
    let out = run_with_input(&repo.root, &["update-ref", "--stdin"], &input);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(tip(&repo.root), repo.main);
    let attest = format!("attest {main} {to} refs/heads/main\n");
    let abort = format!("abort {main} {to} refs/heads/main\n");
    assert_eq!(ledger(&repo.work), attest + &abort);
}

#[test]
fn without_a_refs_file_the_hook_does_nothing() {
    let repo = repo();
    std::fs::remove_file(repo.work.join(REFS_FILE)).unwrap();
    make_ledger_unwritable(&repo.work);
    let out = move_main_to_stage_tip(&repo, &repo.wt, Some("s1"));
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    assert_eq!(tip(&repo.root), repo.stage_tip);
    assert!(ledger_dir_is_empty(&repo.work));
}

#[test]
fn worktree_move_is_attested_in_the_main_checkout_ledger() {
    let repo = repo();
    let out = move_main_to_stage_tip(&repo, &repo.wt, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stderr(&out), "", "the hook must be silent on success");
    assert_eq!(tip(&repo.root), repo.stage_tip);
    let expected = format!("attest {} {} refs/heads/main\n", repo.main, repo.stage_tip);
    assert_eq!(ledger(&repo.work), expected);
}

#[test]
fn unwritable_ledger_outside_a_session_allows_the_move() {
    let repo = repo();
    make_ledger_unwritable(&repo.work);
    let out = move_main_to_stage_tip(&repo, &repo.wt, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(tip(&repo.root), repo.stage_tip);
    assert!(ledger_dir_is_empty(&repo.work));
}

#[test]
fn session_knowledge_commit_with_a_quoted_path_is_refused() {
    let repo = repo();
    make_ledger_unwritable(&repo.work);
    let path = "doc/loom/knowledge/é.md";
    let file = repo.root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, path).unwrap();
    git(&repo.root, &["add", path]);
    let out = run(&repo.root, &["commit", "-m", path], Some("k1"));
    assert!(
        !out.status.success(),
        "the hook let a session commit {path}"
    );
    let refusal = "loom: refusing to move refs/heads/main from loom session k1";
    assert!(stderr(&out).contains(refusal), "{}", stderr(&out));
    assert_eq!(tip(&repo.root), repo.main);
}

#[test]
fn session_push_from_the_worktree_is_refused() {
    let repo = repo();
    // With `main` checked out, git itself refuses the push.
    git(&repo.root, &["checkout", "--detach"]);
    make_ledger_unwritable(&repo.work);
    let out = run(&repo.wt, &["push", ".", "HEAD:main"], Some("s1"));
    assert!(!out.status.success(), "the hook let a session push main");
    let refusal = "loom: refusing to move refs/heads/main from loom session s1";
    assert!(stderr(&out).contains(refusal), "{}", stderr(&out));
    assert_eq!(tip(&repo.root), repo.main);
}

#[test]
fn hooks_path_dev_null_moves_the_target_without_a_line() {
    let repo = repo();
    let to = repo.stage_tip.as_str();
    let args = [
        "-c",
        "core.hooksPath=/dev/null",
        "update-ref",
        "refs/heads/main",
        to,
    ];
    git(&repo.root, &args);
    assert_eq!(tip(&repo.root), repo.stage_tip);
    assert_eq!(ledger(&repo.work), "");
}

/// `core.hooksPath` is `/dev/null` for git and for loom's reads, attestation
/// is off, and a host commit on `main` writes no ledger line.
fn assert_hooks_skipped(repo: &Repo) {
    let configured = configured_hooks_path(&repo.root);
    assert_eq!(configured.as_deref(), Some("/dev/null"));
    let mode = attestation_mode(&repo.root, &repo.work);
    assert!(matches!(mode, AttestationMode::Off { .. }), "{mode:?}");
    commit(&repo.root, "src/y.rs");
    assert_eq!(ledger(&repo.work), "");
}

#[test]
fn included_hooks_path_turns_attestation_off() {
    let repo = repo();
    let include = repo.root.join(".git/loom-test-include");
    std::fs::write(&include, "[core]\n\thooksPath = /dev/null\n").unwrap();
    git(
        &repo.root,
        &["config", "include.path", include.to_str().unwrap()],
    );
    assert_hooks_skipped(&repo);
}

#[test]
fn worktree_config_hooks_path_turns_attestation_off() {
    let repo = repo();
    git(&repo.root, &["config", "extensions.worktreeConfig", "true"]);
    git(
        &repo.root,
        &["config", "--worktree", "core.hooksPath", "/dev/null"],
    );
    assert_hooks_skipped(&repo);
}
