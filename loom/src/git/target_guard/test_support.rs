//! Repository fixtures for tests of the target guard and its callers:
//! scratch repositories with ambient git config shut out, the stand-in hook
//! that turns attestation on, and a recorded target held by an operator move.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use super::{attestation_mode, check, AttestationMode, GuardState, HOOK_MARKER};

/// A scratch repository on `main` and its state directory `root/.loom/work`.
pub(crate) struct Repo {
    _dir: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) work: PathBuf,
}

/// Trimmed stdout of a git command that must succeed, with ambient config and
/// `LOOM_SESSION_ID` shut out and `envs` added.
pub(super) fn git_env(dir: &Path, args: &[&str], envs: &[(&str, &Path)]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("LOOM_SESSION_ID")
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .envs(envs.iter().copied())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Trimmed stdout of a git command that must succeed, with ambient config
/// shut out.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    git_env(dir, args, &[])
}

/// Write `path` (relative to `dir`) with `text`, commit it, and return the
/// new commit.
pub(crate) fn commit_file(dir: &Path, path: &str, text: &str) -> String {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, text).unwrap();
    git(dir, &["add", path]);
    git(dir, &["commit", "-m", path]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A repository on `main` with `README.md` committed, `.loom/` and
/// `.worktrees/` excluded, and the state directory created. The identity is
/// set in the repository's own config, because the code under test runs git
/// without this module's environment.
pub(crate) fn repo() -> Repo {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t.com"]);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit_file(&root, "README.md", "readme\n");
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    crate::git::worktree::capture_main_identity(&root, &work).unwrap();
    Repo {
        _dir: dir,
        root,
        work,
    }
}

/// Install a stand-in for loom's hook, executable so `attestation_mode` is
/// `Active`. It exits 0 and writes nothing: tests write the ledger.
pub(crate) fn activate(root: &Path) {
    let hook = root.join(".git/hooks/reference-transaction");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, format!("#!/bin/sh\n# {HOOK_MARKER}\nexit 0\n")).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        attestation_mode(root, &root.join(".loom/work")),
        AttestationMode::Active,
        "attestation must be on (is core.hooksPath set at global or system scope?)"
    );
}

/// The state directory with `main` recorded at its current tip.
pub(crate) fn recorded() -> (Repo, String) {
    let repo = repo();
    let first = check(&repo.root, &repo.work, "main").unwrap();
    assert!(matches!(first, Some(GuardState::Clear { .. })));
    let accepted = git(&repo.root, &["rev-parse", "main"]);
    (repo, accepted)
}

/// A recorded `main` moved by an operator commit that changes a control path;
/// `evaluate` records the hold. Returns the repository, the accepted tip and
/// the held tip.
pub(crate) fn held_repo(evaluate: bool) -> (Repo, String, String) {
    let (repo, accepted) = recorded();
    let moved = commit_file(&repo.root, ".claude/settings.json", "{}\n");
    if evaluate {
        let held = check(&repo.root, &repo.work, "main").unwrap();
        assert!(matches!(held, Some(GuardState::Held(_))), "{held:?}");
    }
    (repo, accepted, moved)
}
