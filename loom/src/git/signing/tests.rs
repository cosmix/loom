//! Signing tests, and the fixtures other modules' signing tests share.

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

use crate::git::merge::commit_merge;

/// `git` in `dir` with the global and system configuration pointed at
/// missing files; returns trimmed stdout and panics on a failure.
pub(crate) fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
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

/// A complete environment for the `*_in` helpers: this process's `PATH`,
/// `HOME` at `dir`, and the global and system configuration at missing files.
pub(crate) fn isolated_env(dir: &Path) -> Vec<(OsString, OsString)> {
    let mut env = vec![
        ("HOME".into(), dir.as_os_str().to_owned()),
        (
            "GIT_CONFIG_GLOBAL".into(),
            dir.join(".loom-test-no-global").into(),
        ),
        (
            "GIT_CONFIG_SYSTEM".into(),
            dir.join(".loom-test-no-system").into(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        env.push(("PATH".into(), path));
    }
    env
}

/// Install a fake gpg program repo-locally with `commit.gpgsign=true` and
/// `gpg.format=openpgp`. The script reads its stdin to EOF first (git writes
/// the commit payload to it); with `fail` it exits 1 with "fake signer
/// refused" on stderr, else it prints the `SIG_CREATED` status line on stderr
/// (git passes `--status-fd=2`) and an armored block on stdout. Returns its
/// log: one line per call, the argv then `GNUPGHOME=<value>`.
pub(crate) fn fake_signer(repo: &Path, fail: bool) -> PathBuf {
    let dir = PathBuf::from(git_in(repo, &["rev-parse", "--absolute-git-dir"]));
    let log = dir.join("fake-signer.log");
    let script = dir.join("fake-signer");
    let outcome = if fail {
        "echo 'fake signer refused' >&2\nexit 1\n"
    } else {
        "printf '\\n[GNUPG:] SIG_CREATED D 1 8 00 0 FAKE\\n' >&2\n\
         printf '%s\\n' '-----BEGIN PGP SIGNATURE-----' '' 'ZmFrZQ==' '-----END PGP SIGNATURE-----'\n"
    };
    let body = format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s GNUPGHOME=%s\\n' \"$*\" \"$GNUPGHOME\" >> '{}'\n{outcome}",
        log.display()
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    git_in(repo, &["config", "gpg.format", "openpgp"]);
    git_in(repo, &["config", "gpg.program", script.to_str().unwrap()]);
    git_in(repo, &["config", "commit.gpgsign", "true"]);
    log
}

/// A repository on `main` with one commit, signing off.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_in(root, &["init", "-b", "main"]);
    git_in(root, &["config", "user.email", "t@t.com"]);
    git_in(root, &["config", "user.name", "t"]);
    git_in(root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("a.txt"), "a\n").unwrap();
    git_in(root, &["add", "a.txt"]);
    git_in(root, &["commit", "-m", "init"]);
    temp
}

fn head_tree(root: &Path) -> (String, String) {
    (
        git_in(root, &["rev-parse", "HEAD"]),
        git_in(root, &["rev-parse", "HEAD^{tree}"]),
    )
}

fn is_signed(root: &Path, id: &str) -> bool {
    let object = git_in(root, &["cat-file", "commit", id]);
    object.lines().any(|line| line.starts_with("gpgsig "))
}

fn log_lines(log: &Path) -> Vec<String> {
    let text = fs::read_to_string(log).unwrap_or_default();
    text.lines().map(str::to_string).collect()
}

#[test]
fn signing_enabled_reads_the_bool_forms() {
    let temp = repo();
    let root = temp.path();
    for (value, expected) in [("yes", true), ("on", true), ("1", true), ("true", true)] {
        git_in(root, &["config", "commit.gpgsign", value]);
        assert_eq!(signing_enabled(root).unwrap(), expected, "{value}");
    }
    for (value, expected) in [
        ("no", false),
        ("off", false),
        ("0", false),
        ("false", false),
    ] {
        git_in(root, &["config", "commit.gpgsign", value]);
        assert_eq!(signing_enabled(root).unwrap(), expected, "{value}");
    }
    git_in(root, &["config", "--unset", "commit.gpgsign"]);
    assert!(!signing_enabled_in(root, &isolated_env(root)).unwrap());
}

#[test]
fn signing_enabled_rejects_a_malformed_value() {
    let temp = repo();
    git_in(temp.path(), &["config", "commit.gpgsign", "maybe"]);

    let error = signing_enabled(temp.path()).expect_err("a malformed value is an error");

    assert!(error.to_string().contains("commit.gpgsign"), "{error}");
}

#[test]
fn commit_tree_is_unsigned_when_gpgsign_is_off() {
    let temp = repo();
    let root = temp.path();
    let log = fake_signer(root, false);
    git_in(root, &["config", "commit.gpgsign", "false"]);
    let (head, tree) = head_tree(root);

    let id = commit_tree(root, &tree, &[&head], "unsigned").unwrap();

    assert!(!is_signed(root, &id));
    assert!(
        log_lines(&log).is_empty(),
        "the signer ran: {:?}",
        log_lines(&log)
    );
}

#[test]
fn commit_tree_signs_once_and_the_commit_has_a_gpgsig_header() {
    let temp = repo();
    let root = temp.path();
    let log = fake_signer(root, false);
    let (head, tree) = head_tree(root);

    let id = commit_tree(root, &tree, &[&head], "signed").unwrap();

    assert!(is_signed(root, &id));
    assert_eq!(log_lines(&log).len(), 1, "{:?}", log_lines(&log));
    assert_eq!(git_in(root, &["rev-parse", &format!("{id}^")]), head);
}

#[test]
fn signing_environment_reaches_only_the_signing_call() {
    let temp = repo();
    let root = temp.path();
    let log = fake_signer(root, false);
    let (head, tree) = head_tree(root);
    let before = std::env::var_os("GNUPGHOME");
    let carried = SigningEnv {
        gnupghome: Some("/x".into()),
        ssh_auth_sock: None,
    };

    commit_tree_with(root, &tree, &[&head], "carried", Some(&carried)).unwrap();
    commit_tree_with(
        root,
        &tree,
        &[&head],
        "default",
        Some(&SigningEnv::default()),
    )
    .unwrap();

    let lines = log_lines(&log);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].ends_with("GNUPGHOME=/x"), "{}", lines[0]);
    let own = before.clone().unwrap_or_default();
    let expected = format!("GNUPGHOME={}", own.to_string_lossy());
    assert!(lines[1].ends_with(&expected), "{} vs {expected}", lines[1]);
    assert_eq!(std::env::var_os("GNUPGHOME"), before);
}

#[test]
fn a_failing_signer_is_a_signing_error_with_its_stderr_tail() {
    let temp = repo();
    let root = temp.path();
    fake_signer(root, true);
    let (head, tree) = head_tree(root);

    let error = commit_tree(root, &tree, &[&head], "refused").unwrap_err();

    assert!(error.signing, "{error:?}");
    assert!(
        error.detail.contains("fake signer refused"),
        "{}",
        error.detail
    );
    assert!(error.detail.len() <= TAIL_BYTES);
}

#[test]
fn probe_passes_with_a_working_signer() {
    let temp = repo();
    let log = fake_signer(temp.path(), false);

    probe(temp.path(), &SigningEnv::default()).unwrap();

    assert_eq!(log_lines(&log).len(), 1);
}

#[test]
fn probe_names_the_signer_failure() {
    let temp = repo();
    fake_signer(temp.path(), true);

    let error = probe(temp.path(), &SigningEnv::default()).unwrap_err();

    let text = error.to_string();
    assert!(text.contains("a test signature failed"), "{text}");
    assert!(text.contains("fake signer refused"), "{text}");
    assert!(text.contains("git commit-tree -S"), "{text}");
}

#[test]
fn merge_commit_is_signed_through_commit_merge() {
    let temp = repo();
    let root = temp.path();
    let (first, tree) = head_tree(root);
    let second = git_in(root, &["commit-tree", &tree, "-m", "side"]);
    let log = fake_signer(root, false);

    let id = commit_merge(root, &tree, [&first, &second], "Merge").unwrap();

    let object = git_in(root, &["cat-file", "commit", &id]);
    assert!(object.contains(&format!("parent {first}")), "{object}");
    assert!(object.contains(&format!("parent {second}")), "{object}");
    assert!(is_signed(root, &id));
    assert_eq!(log_lines(&log).len(), 1);
}

#[test]
fn a_failed_merge_signature_stays_downcastable() {
    let temp = repo();
    let root = temp.path();
    let (first, tree) = head_tree(root);
    let second = git_in(root, &["commit-tree", &tree, "-m", "side"]);
    fake_signer(root, true);

    let error = commit_merge(root, &tree, [&first, &second], "Merge").unwrap_err();

    let found = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CommitTreeError>())
        .expect("the CommitTreeError survives in the chain");
    assert!(found.signing);
}

#[test]
fn the_installed_signing_environment_wins_over_the_process() {
    let installed = SigningEnv {
        gnupghome: Some("/installed".into()),
        ssh_auth_sock: Some("/installed.sock".into()),
    };
    let process = || SigningEnv {
        gnupghome: Some("/process".into()),
        ssh_auth_sock: None,
    };

    let chosen = current_from(Some(&installed), || panic!("captured despite an install"));
    let captured = current_from(None, process);

    assert_eq!(chosen.gnupghome.as_deref(), Some(OsStr::new("/installed")));
    assert_eq!(
        chosen.ssh_auth_sock.as_deref(),
        Some(OsStr::new("/installed.sock"))
    );
    assert_eq!(captured.gnupghome.as_deref(), Some(OsStr::new("/process")));
    assert_eq!(
        captured.env_pairs(),
        vec![(GNUPGHOME, OsStr::new("/process"))]
    );
}

#[test]
fn a_signer_that_never_finishes_times_out_naming_the_bound() {
    let error = anyhow::Error::new(ProcessTimeoutError::new("git commit-tree", SIGN_TIMEOUT));

    let detail = run_failure(&error.context("Failed to execute: git commit-tree"));

    assert!(detail.contains("30 s"), "{detail}");
    assert!(detail.contains("passphrase"), "{detail}");
}
