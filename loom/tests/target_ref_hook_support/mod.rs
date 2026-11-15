//! Fixtures the `reference-transaction` hook tests share: git run with the
//! user's and system's config shut out and `LOOM_SESSION_ID` set or removed
//! explicitly, and a repository on `main` that the target guard records, with a
//! linked worktree standing in for a stage.

use loom::git::hooks::{install_reference_transaction_hook, HookInstall};
use loom::git::target_guard::{attestation_mode, check, AttestationMode, GuardState, LEDGER_FILE};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub use crate::test_support::git_repo::git;
use crate::test_support::git_repo::{git_command, TestRepo};

/// git in `dir` through the shared runner; `session` sets `LOOM_SESSION_ID`, `None` leaves it
/// removed. Replace refs and grafts stay enabled unless a test turns them off: the hook must
/// do that itself. The including crate root declares `test_support` (`crate::test_support`).
pub fn command(dir: &Path, args: &[&str], session: Option<&str>) -> Command {
    let mut cmd = git_command(dir);
    cmd.args(args);
    if let Some(id) = session {
        cmd.env("LOOM_SESSION_ID", id);
    }
    cmd
}

pub fn run(dir: &Path, args: &[&str], session: Option<&str>) -> Output {
    command(dir, args, session).output().unwrap()
}

/// git in `dir` outside any session, with `input` on its stdin.
pub fn run_with_input(dir: &Path, args: &[&str], input: &str) -> Output {
    let mut cmd = command(dir, args, None);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// One commit of every one of `paths` (each file holds its own path) in the
/// checkout at `dir` outside any session.
pub fn commit_paths(dir: &Path, paths: &[&str]) -> String {
    for &path in paths {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, path).unwrap();
        git(dir, &["add", path]);
    }
    git(dir, &["commit", "-m", &paths.join(" ")]);
    git(dir, &["rev-parse", "HEAD"])
}

/// Commit `path` in the checkout at `dir` outside any session.
pub fn commit(dir: &Path, path: &str) -> String {
    commit_paths(dir, &[path])
}

pub fn tip(root: &Path) -> String {
    git(root, &["rev-parse", "refs/heads/main"])
}

pub fn ledger(work: &Path) -> String {
    std::fs::read_to_string(work.join(LEDGER_FILE)).unwrap()
}

/// Replace the ledger with a directory, so no one can append to it.
pub fn make_ledger_unwritable(work: &Path) {
    let ledger = work.join(LEDGER_FILE);
    std::fs::remove_file(&ledger).unwrap();
    std::fs::create_dir_all(&ledger).unwrap();
}

pub fn ledger_dir_is_empty(work: &Path) -> bool {
    std::fs::read_dir(work.join(LEDGER_FILE))
        .unwrap()
        .next()
        .is_none()
}

pub struct Repo {
    _dir: TempDir,
    pub root: PathBuf,
    pub work: PathBuf,
    /// The linked worktree `.worktrees/s` on `loom/s`.
    pub wt: PathBuf,
    /// `main`'s tip when the guard recorded it.
    pub main: String,
    /// `loom/s`'s tip: a child of `main` adding `a.txt`.
    pub stage_tip: String,
}

/// A repository on `main` with one commit, a linked worktree `.worktrees/s`
/// on `loom/s` with one more, `.loom/work` created, the hook installed with
/// attestation on, and `main` recorded by `check` (which writes the refs
/// file and the empty ledger).
pub fn repo() -> Repo {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    TestRepo::builder()
        .identity("t", "t@t.com")
        .no_seed()
        .init_in(&root);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit(&root, "README.md");
    let wt = root.join(".worktrees/s");
    git(
        &root,
        &["worktree", "add", "-b", "loom/s", wt.to_str().unwrap()],
    );
    let stage_tip = commit(&wt, "a.txt");
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    let installed = install_reference_transaction_hook(&root).unwrap();
    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(
        attestation_mode(&root, &work),
        AttestationMode::Active,
        "attestation must be on (is core.hooksPath set at global or system scope?)"
    );
    let main = tip(&root);
    let state = check(&root, &work, "main")
        .unwrap()
        .expect("no other merge lock holder");
    assert_eq!(
        state,
        GuardState::Clear {
            accepted: main.clone()
        }
    );
    assert_eq!(ledger(&work), "");
    Repo {
        _dir: dir,
        root,
        work,
        wt,
        main,
        stage_tip,
    }
}

/// `git update-ref refs/heads/main <stage tip>` in `dir` as `session`.
pub fn move_main_to_stage_tip(repo: &Repo, dir: &Path, session: Option<&str>) -> Output {
    let args = ["update-ref", "refs/heads/main", repo.stage_tip.as_str()];
    run(dir, &args, session)
}
