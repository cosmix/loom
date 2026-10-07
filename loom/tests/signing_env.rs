//! The signing environment `take_from_process` captures must reach the signer
//! of every commit the daemon writes. Startup moves `GNUPGHOME` and
//! `SSH_AUTH_SOCK` out of the process environment, so a signed commit that
//! passed git an empty environment would fail only where a real agent needs
//! them. One test in its own process: the installed environment is a
//! process-wide `OnceLock`, and the environment itself is global state.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::git::merge::commit_merge;
use loom::git::signing;
use loom::git::stage_commit::{commit_staged, CommitRequest, CommitScope};
use tempfile::TempDir;

/// Run git in `root`; the test process already points the global and system
/// configuration at missing files. Returns trimmed stdout.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository with one commit on `main`, checked out on `loom/s1`, with
/// every setting the code under test could inherit pinned repo-locally.
fn repo(root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "gpg.format", "openpgp"]);
    let hooks = root.join(".git").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    git(root, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-m", "init"]);
    git(root, &["checkout", "-b", "loom/s1"]);
}

/// Install a fake signer repo-locally and turn signing on. It reads the commit
/// payload to EOF, logs the two signing variables it sees, and answers with a
/// status line and an armored block. Returns its log, one line per call.
fn enable_fake_signer(root: &Path, dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let log = dir.join("signer.log");
    let script = dir.join("fake-gpg");
    let body = format!(
        r#"#!/bin/sh
cat >/dev/null
printf 'GNUPGHOME=%s SSH_AUTH_SOCK=%s\n' "$GNUPGHOME" "$SSH_AUTH_SOCK" >> '{log}'
printf '\n[GNUPG:] SIG_CREATED D 1 8 00 0 FAKE\n' >&2
printf '%s\n' '-----BEGIN PGP SIGNATURE-----' 'ZmFrZSBzaWduYXR1cmUgZm9yIGxvb20=' '-----END PGP SIGNATURE-----'
"#,
        log = log.display()
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    git(root, &["config", "gpg.program", script.to_str().unwrap()]);
    git(root, &["config", "commit.gpgsign", "true"]);
    log
}

fn signer_calls(log: &Path) -> Vec<String> {
    let text = fs::read_to_string(log).unwrap_or_default();
    text.lines().map(str::to_string).collect()
}

fn assert_signed(root: &Path, id: &str) {
    let object = git(root, &["cat-file", "commit", id]);
    assert!(
        object.lines().any(|line| line.starts_with("gpgsig ")),
        "commit {id} carries no gpgsig header:\n{object}"
    );
}

/// Point git at missing global and system configuration, put the two signing
/// variables in the environment, and move them into the installed signing
/// environment. Returns the log line a signer that saw them writes.
fn capture_signing_environment(dir: &Path) -> String {
    let gnupghome = dir.join("gnupg-home");
    let agent_sock = dir.join("agent.sock");
    std::env::set_var("GIT_CONFIG_GLOBAL", dir.join("no-global"));
    std::env::set_var("GIT_CONFIG_SYSTEM", dir.join("no-system"));
    std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    std::env::set_var("GNUPGHOME", &gnupghome);
    std::env::set_var("SSH_AUTH_SOCK", &agent_sock);

    signing::take_from_process();

    assert!(std::env::var_os("GNUPGHOME").is_none());
    assert!(std::env::var_os("SSH_AUTH_SOCK").is_none());
    let installed = signing::installed();
    assert_eq!(installed.gnupghome.as_deref(), Some(gnupghome.as_os_str()));
    assert_eq!(
        installed.ssh_auth_sock.as_deref(),
        Some(agent_sock.as_os_str())
    );
    format!(
        "GNUPGHOME={} SSH_AUTH_SOCK={}",
        gnupghome.display(),
        agent_sock.display()
    )
}

/// Commit a newly staged file on `loom/s1` as the daemon does for a session.
fn commit_a_staged_file(root: &Path) -> String {
    fs::write(root.join("a.txt"), "a\n").unwrap();
    git(root, &["add", "a.txt"]);
    let request = CommitRequest {
        message: "feat(s1): add a.txt".to_string(),
        expected_head: git(root, &["rev-parse", "HEAD"]),
        expected_tree: git(root, &["write-tree"]),
    };
    let scope = CommitScope::StageBranch {
        stage_id: "s1".to_string(),
    };
    commit_staged(root, &scope, &request).expect("commit_staged refused")
}

#[test]
fn the_captured_signing_environment_reaches_the_signer_of_each_daemon_commit() {
    let temp = TempDir::new().unwrap();
    let expected = capture_signing_environment(temp.path());
    let root = temp.path().join("repo");
    repo(&root);
    let base_tree = git(&root, &["rev-parse", "main^{tree}"]);
    let side_args: [&str; 6] = ["commit-tree", &base_tree, "-p", "main", "-m", "side"];
    let side = git(&root, &side_args);
    let log = enable_fake_signer(&root, &temp.path().join("signer"));

    let staged = commit_a_staged_file(&root);

    assert_signed(&root, &staged);
    assert_eq!(signer_calls(&log), std::slice::from_ref(&expected));

    let tree = git(&root, &["rev-parse", &format!("{staged}^{{tree}}")]);
    let merged = commit_merge(&root, &tree, [&staged, &side], "Merge loom/s1").unwrap();

    assert_signed(&root, &merged);
    assert_eq!(signer_calls(&log), [expected.clone(), expected]);
}
