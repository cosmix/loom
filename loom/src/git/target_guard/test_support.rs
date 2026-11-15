//! Repository fixtures for tests of the target guard and its callers:
//! scratch repositories on the shared git fixtures, the stand-in hook
//! that turns attestation on, and a recorded target held by an operator move.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::{attestation_mode, check, AttestationMode, GuardState, HOOK_MARKER};
use crate::test_git::{git_with_env, TestRepo};

pub(crate) use crate::test_git::git;

/// A scratch repository on `main` and its state directory `root/.loom/work`.
pub(crate) struct Repo {
    _dir: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) work: PathBuf,
}

/// Trimmed stdout of a git command that must succeed, with ambient config and
/// `LOOM_SESSION_ID` shut out and `envs` added.
pub(super) fn git_env(dir: &Path, args: &[&str], envs: &[(&str, &Path)]) -> String {
    git_with_env(dir, args, envs)
}

/// Write `path` (relative to `dir`) with `text`, commit it with the path as
/// the message, and return the new commit.
pub(crate) fn commit_file(dir: &Path, path: &str, text: &str) -> String {
    crate::test_git::commit_file(dir, path, text, path)
}

/// A repository on `main` with `README.md` committed, `.loom/` and
/// `.worktrees/` excluded, and the state directory created. The identity is
/// set in the repository's own config, because the code under test runs git
/// without this module's environment.
pub(crate) fn repo() -> Repo {
    let repo = TestRepo::builder()
        .identity("t", "t@t.com")
        .seed("README.md", "readme\n", "README.md")
        .create();
    let root = repo.path().to_path_buf();
    let dir = repo.into_temp_dir();
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
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
