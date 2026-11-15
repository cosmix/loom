//! Contracts for the `reference-transaction` hook: it refuses a sandboxed
//! session's move of a guarded target, lets a knowledge-only fast-forward
//! through, and attests a host move with the ref's real old value.
//!
//! Every git command here runs with the user's and system's config shut out
//! and sets or removes `LOOM_SESSION_ID` explicitly.

use loom::git::hooks::{install_reference_transaction_hook, HookInstall};
use loom::git::target_guard::{
    attestation_mode, check, AttestationMode, GuardState, LEDGER_FILE, REFS_FILE,
};
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

#[path = "test_support/mod.rs"]
mod test_support;

use test_support::git_repo::{git, git_command, TestRepo};

/// Run git in `dir`; `session` sets `LOOM_SESSION_ID`, `None` leaves it removed.
fn run(dir: &Path, args: &[&str], session: Option<&str>) -> Output {
    let mut cmd = git_command(dir);
    cmd.args(args);
    if let Some(id) = session {
        cmd.env("LOOM_SESSION_ID", id);
    }
    cmd.output().unwrap()
}

fn git_as(dir: &Path, args: &[&str], session: Option<&str>) -> String {
    let out = run(dir, args, session);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Commit `path` in the checkout at `dir` as `session`.
fn commit(dir: &Path, path: &str, session: Option<&str>) -> String {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, path).unwrap();
    git_as(dir, &["add", path], session);
    git_as(dir, &["commit", "-m", path], session);
    git(dir, &["rev-parse", "HEAD"])
}

fn tip(root: &Path) -> String {
    git(root, &["rev-parse", "refs/heads/main"])
}

/// A repository on `main` with one commit, `.loom/` and `.worktrees/`
/// excluded, `root/.loom/work` created, the hook installed with attestation
/// on, and `main` recorded by `check`.
fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .init_in(&root);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit(&root, "README.md", None);
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    (dir, root, work)
}

fn install_and_record(root: &Path, work: &Path) -> String {
    let installed = install_reference_transaction_hook(root).unwrap();
    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(
        attestation_mode(root, work),
        AttestationMode::Active,
        "attestation must be on (is core.hooksPath set at global or system scope?)"
    );
    let main = tip(root);
    let state = check(root, work, "main")
        .unwrap()
        .expect("no other merge lock holder");
    assert_eq!(
        state,
        GuardState::Clear {
            accepted: main.clone()
        }
    );
    let refs = std::fs::read_to_string(work.join(REFS_FILE)).unwrap();
    assert!(refs.lines().any(|l| l == "ref refs/heads/main"), "{refs}");
    main
}

/// `.worktrees/s` on `loom/s` with one commit of `a.txt`; returns its tip.
fn stage_branch(root: &Path) -> (PathBuf, String) {
    let wt = root.join(".worktrees/s");
    git(
        root,
        &["worktree", "add", "-b", "loom/s", wt.to_str().unwrap()],
    );
    let stage_tip = commit(&wt, "a.txt", None);
    (wt, stage_tip)
}

/// Replace the ledger with a directory, so no one can append to it.
fn make_ledger_unwritable(work: &Path) {
    let ledger = work.join(LEDGER_FILE);
    let _ = std::fs::remove_file(&ledger);
    std::fs::create_dir_all(&ledger).unwrap();
}

#[test]
fn sandboxed_session_update_of_the_target_is_refused() {
    let (_dir, root, work) = fixture();
    let (wt, stage_tip) = stage_branch(&root);
    let main = install_and_record(&root, &work);
    make_ledger_unwritable(&work);
    let args = ["update-ref", "refs/heads/main", stage_tip.as_str()];
    let out = run(&wt, &args, Some("s1"));
    assert!(!out.status.success(), "the hook let a session move main");
    assert_eq!(tip(&root), main);
}

#[test]
fn sandboxed_session_knowledge_only_commit_is_allowed() {
    let (_dir, root, work) = fixture();
    let main = install_and_record(&root, &work);
    make_ledger_unwritable(&work);
    let new_main = commit(&root, "doc/loom/knowledge/x.md", Some("k1"));
    assert_eq!(tip(&root), new_main);
    assert_ne!(new_main, main);
    assert_eq!(git(&root, &["rev-parse", "refs/heads/main~1"]), main);
}

#[test]
fn host_update_is_attested_with_the_real_old_value() {
    let (_dir, root, work) = fixture();
    let (_wt, stage_tip) = stage_branch(&root);
    let main = install_and_record(&root, &work);
    git(&root, &["checkout", "--detach"]);
    git(&root, &["branch", "-f", "main", &stage_tip]);
    assert_eq!(tip(&root), stage_tip);
    let ledger = std::fs::read_to_string(work.join(LEDGER_FILE)).unwrap();
    let expected = format!("attest {main} {stage_tip} refs/heads/main");
    assert!(ledger.lines().any(|l| l == expected), "{ledger}");
}
