//! Frozen contracts for stage daemon-owned-commits: the daemon commits a
//! session's staged index with plumbing only, refuses state paths and a moved
//! HEAD, never runs repository hooks, signs stage and merge commits when
//! `commit.gpgsign` is true, and relays `commit` as a control kind.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::git::merge::commit_merge;
use loom::git::stage_commit::{commit_staged, CommitRequest, CommitScope};
use loom::models::session::SessionType;
use loom::relay::{verdict, MatrixVerdict, RequestKind};
use tempfile::TempDir;

const BRANCH: &str = "refs/heads/loom/s1";

/// Run git in `root` with ambient config neutralised; returns trimmed stdout.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A repository with one commit on `main`, checked out on `loom/s1`. Every
/// setting the code under test could inherit from the host's global config is
/// pinned repo-locally: signing off, openpgp format, hooks in `.git/hooks`.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "gpg.format", "openpgp"]);
    let hooks = root.join(".git").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    git(root, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    write(root, "README.md", "base\n");
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-m", "init"]);
    git(root, &["checkout", "-b", "loom/s1"]);
    temp
}

/// The request a session that saw the current HEAD and index would send.
fn request_for_current_index(root: &Path, message: &str) -> CommitRequest {
    CommitRequest {
        message: message.to_string(),
        expected_head: git(root, &["rev-parse", "HEAD"]),
        expected_tree: git(root, &["write-tree"]),
    }
}

fn stage_scope() -> CommitScope {
    CommitScope::StageBranch {
        stage_id: "s1".to_string(),
    }
}

fn branch_tip(root: &Path) -> String {
    git(root, &["rev-parse", BRANCH])
}

/// Install the fake signer repo-locally and turn signing on. Returns the
/// signer's argv log path; the signer lives in `dir`.
fn enable_fake_signer(root: &Path, dir: &Path) -> PathBuf {
    let log = dir.join("signer-argv.log");
    let script = dir.join("fake-gpg");
    let body = format!(
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' "$*" >> '{log}'
fd=2
for arg in "$@"; do
  case "$arg" in --status-fd=*) fd="${{arg#--status-fd=}}" ;; esac
done
eval "printf '\n[GNUPG:] SIG_CREATED D 1 8 00 0 FAKE\n' >&$fd"
printf '%s\n' '-----BEGIN PGP SIGNATURE-----' 'ZmFrZSBzaWduYXR1cmUgZm9yIGxvb20gY29udHJhY3Q=' '-----END PGP SIGNATURE-----'
"#,
        log = log.display()
    );
    write_executable(&script, &body);
    git(root, &["config", "gpg.format", "openpgp"]);
    git(root, &["config", "gpg.program", script.to_str().unwrap()]);
    git(root, &["config", "commit.gpgsign", "true"]);
    log
}

fn assert_signed(root: &Path, id: &str, log: &Path) {
    let object = git(root, &["cat-file", "commit", id]);
    assert!(
        object.lines().any(|line| line.starts_with("gpgsig ")),
        "commit {id} carries no gpgsig header:\n{object}"
    );
    let argv = fs::read_to_string(log).unwrap_or_default();
    assert!(!argv.trim().is_empty(), "the fake signer was never invoked");
}

#[test]
fn staged_state_path_is_refused() {
    let temp = repo();
    let root = temp.path();
    let before = branch_tip(root);
    write(root, ".loom/work/x", "state\n");
    git(root, &["add", "-f", ".loom/work/x"]);
    let request = request_for_current_index(root, "feat(s1): commit state");

    let result = commit_staged(root, &stage_scope(), &request);

    assert!(
        result.is_err(),
        "a staged .loom/work/x was committed: {result:?}"
    );
    assert_eq!(branch_tip(root), before, "a refusal moved {BRANCH}");
}

#[test]
fn stage_commit_is_signed_when_gpgsign() {
    let temp = repo();
    let root = temp.path();
    let signer = TempDir::new().unwrap();
    let log = enable_fake_signer(root, signer.path());
    write(root, "a.txt", "a\n");
    git(root, &["add", "a.txt"]);
    let request = request_for_current_index(root, "feat(s1): add a.txt");

    let id = commit_staged(root, &stage_scope(), &request).expect("commit_staged refused");

    assert_eq!(branch_tip(root), id);
    assert_signed(root, &id, &log);
}

#[test]
fn merge_commit_is_signed_when_gpgsign() {
    let temp = repo();
    let root = temp.path();
    write(root, "a.txt", "a\n");
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "feat(s1): add a.txt"]);
    let first = git(root, &["rev-parse", "HEAD"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    let base_tree = git(root, &["rev-parse", "main^{tree}"]);
    let second = git(
        root,
        &["commit-tree", &base_tree, "-p", "main", "-m", "side"],
    );
    let signer = TempDir::new().unwrap();
    let log = enable_fake_signer(root, signer.path());

    let id = commit_merge(root, &tree, [&first, &second], "Merge loom/s1").unwrap();

    let object = git(root, &["cat-file", "commit", &id]);
    assert!(object.contains(&format!("parent {first}")));
    assert!(object.contains(&format!("parent {second}")));
    assert_signed(root, &id, &log);
}

#[test]
fn moved_head_is_refused() {
    let temp = repo();
    let root = temp.path();
    let seen_head = git(root, &["rev-parse", "HEAD"]);
    write(root, "b.txt", "b\n");
    git(root, &["add", "b.txt"]);
    git(root, &["commit", "-m", "feat(s1): moved"]);
    let moved = branch_tip(root);
    write(root, "c.txt", "c\n");
    git(root, &["add", "c.txt"]);
    let request = CommitRequest {
        message: "feat(s1): add c.txt".to_string(),
        expected_head: seen_head,
        expected_tree: git(root, &["write-tree"]),
    };

    let result = commit_staged(root, &stage_scope(), &request);

    assert!(
        result.is_err(),
        "a commit with a stale expected_head was applied: {result:?}"
    );
    assert_eq!(branch_tip(root), moved, "a refusal moved {BRANCH}");
}

#[test]
fn planted_hooks_never_run() {
    let temp = repo();
    let root = temp.path();
    let markers = TempDir::new().unwrap();
    let hooks = root.join(".git").join("hooks");
    let names = [
        "pre-commit",
        "commit-msg",
        "post-commit",
        "reference-transaction",
    ];
    for name in names {
        let marker = markers.path().join(name);
        let body = format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display());
        write_executable(&hooks.join(name), &body);
    }
    write(root, "a.txt", "a\n");
    git(root, &["add", "a.txt"]);
    let request = request_for_current_index(root, "feat(s1): add a.txt");

    let id = commit_staged(root, &stage_scope(), &request).expect("commit_staged refused");

    assert_eq!(branch_tip(root), id);
    for name in names {
        assert!(
            !markers.path().join(name).exists(),
            "repository hook {name} ran during commit_staged"
        );
    }
}

#[test]
fn commit_kind_is_control_and_refused_for_contract() {
    assert!(RequestKind::Commit.is_control());
    assert_eq!(
        verdict(SessionType::Contract, RequestKind::Commit),
        MatrixVerdict::Refuse
    );
}
