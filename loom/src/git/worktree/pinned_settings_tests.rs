//! The operator's identity and signing survive a private git directory: an
//! `includeIf "gitdir:"` matches the real path, and only files a session cannot
//! write decide them.

use super::*;
use crate::verify::contracts::test_support::contract_worktree;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

struct Operator {
    _tmp: TempDir,
    repo: PathBuf,
    worktree: PathBuf,
    common: PathBuf,
    env: Vec<(OsString, OsString)>,
}

/// A repository with no identity of its own, and a global file that gives one
/// only through `includeIf "gitdir:<common>"`, the operator's usual setup.
fn operator(extension: bool) -> Operator {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let plain = |args: &[&str]| {
        let output = crate::git::runner::run_git(args, &repo).unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
    };
    if extension {
        plain(&["config", "extensions.worktreeConfig", "true"]);
    }
    for key in ["user.name", "user.email", "commit.gpgsign"] {
        let _ = crate::git::runner::run_git(&["config", "--unset", key], &repo);
    }
    let common = repo.join(".git").canonicalize().unwrap();
    let included = tmp.path().join("included.cfg");
    std::fs::write(
        &included,
        "[user]\n\tname = Included\n\temail = included@example.invalid\n\
         [commit]\n\tgpgsign = true\n",
    )
    .unwrap();
    let global = tmp.path().join("global.cfg");
    let body = format!(
        "[includeIf \"gitdir:{}\"]\n\tpath = {}\n[user]\n\tuseConfigOnly = true\n",
        common.display(),
        included.display()
    );
    std::fs::write(&global, body).unwrap();
    let env = vec![
        ("PATH".into(), std::env::var_os("PATH").unwrap_or_default()),
        ("HOME".into(), tmp.path().as_os_str().to_os_string()),
        (
            "GIT_CONFIG_GLOBAL".into(),
            global.as_os_str().to_os_string(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ];
    Operator {
        _tmp: tmp,
        repo,
        worktree,
        common,
        env,
    }
}

const WAIT: Duration = Duration::from_secs(15);

impl Operator {
    fn git_on(&self, git_dir: &Path, args: &[&str]) -> Output {
        let mut vars = self.env.clone();
        vars.push(("GIT_DIR".into(), git_dir.as_os_str().to_os_string()));
        vars.push((
            "GIT_COMMON_DIR".into(),
            self.common.as_os_str().to_os_string(),
        ));
        run_git_in_env_within(&self.repo, args, &vars, WAIT).unwrap()
    }
}

/// The positive control is git with a private `GIT_DIR` and no help: the include
/// does not match, so the operator's identity and signing are gone.
#[test]
fn an_include_on_the_real_git_directory_still_applies() {
    for extension in [true, false] {
        let operator = operator(extension);
        let pinned = crate::git::worktree::WorktreeGit::pinned_main(&operator.repo).unwrap();
        let read = ["config", "--type=bool", "commit.gpgsign"];

        let signing = pinned
            .run_in_env_within(&read, &operator.env, WAIT)
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&signing.stdout).trim(),
            "true",
            "{signing:?}"
        );

        let name = pinned
            .run_in_env_within(&["var", "GIT_COMMITTER_IDENT"], &operator.env, WAIT)
            .unwrap();
        assert!(name.status.success(), "{name:?}");
        assert!(
            String::from_utf8_lossy(&name.stdout).starts_with("Included "),
            "{name:?}"
        );

        let private = operator._tmp.path().join("private");
        std::fs::create_dir_all(private.join("refs")).unwrap();
        std::fs::copy(operator.common.join("HEAD"), private.join("HEAD")).unwrap();
        let mut commondir = operator.common.as_os_str().as_bytes().to_vec();
        commondir.push(b'\n');
        std::fs::write(private.join("commondir"), commondir).unwrap();
        let bare = operator.git_on(&private, &read);
        assert_eq!(String::from_utf8_lossy(&bare.stdout).trim(), "", "{bare:?}");
    }
}

/// A commit written with the pin carries the included identity.
#[test]
fn a_commit_tree_carries_the_included_identity() {
    let operator = operator(false);
    let pinned = crate::git::worktree::WorktreeGit::pinned_main(&operator.repo).unwrap();
    let mut env = operator.env.clone();
    // Signing needs a key this test has none of; the identity is what is tested.
    env.push(("GIT_CONFIG_COUNT".into(), "1".into()));
    env.push(("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into()));
    env.push(("GIT_CONFIG_VALUE_0".into(), "false".into()));

    let args = ["commit-tree", "HEAD^{tree}", "-m", "identity"];
    let commit = pinned.run_in_env_within(&args, &env, WAIT).unwrap();
    assert!(commit.status.success(), "{commit:?}");
    let id = String::from_utf8_lossy(&commit.stdout).trim().to_string();

    let author = pinned
        .run_in_env_within(&["log", "-1", "--format=%an <%ae>", &id], &env, WAIT)
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&author.stdout).trim(),
        "Included <included@example.invalid>"
    );
}

/// A session can write `config.worktree`. The control is a default read on the
/// real directory, which takes the session's value; the scoped reads never see
/// the file, even between validation and use.
#[test]
fn a_session_written_config_worktree_decides_no_setting() {
    let operator = operator(true);
    let admin = operator.common.join("worktrees/s1");
    let hostile = "[user]\n\tname = Session\n[gpg]\n\tprogram = /tmp/hostile\n";
    std::fs::write(admin.join("config.worktree"), hostile).unwrap();
    let pin_env = [
        ("GIT_DIR", admin.as_os_str()),
        ("GIT_COMMON_DIR", operator.common.as_os_str()),
        ("GIT_WORK_TREE", operator.worktree.as_os_str()),
    ];
    let source = SettingsSource {
        pin_env: &pin_env,
        work_tree: &operator.worktree,
        env: Some(&operator.env),
    };

    let args = identity_args(&["commit-tree", "x"], &source).unwrap();

    assert!(!args.iter().any(|arg| arg.contains("Session")), "{args:?}");
    assert!(!args.iter().any(|arg| arg.contains("hostile")), "{args:?}");

    let control = operator.git_on(&admin, &["config", "user.name"]);
    assert_eq!(
        String::from_utf8_lossy(&control.stdout).trim(),
        "Session",
        "{control:?}"
    );
}

/// A session can make the real directory unusable for the moment of the read,
/// and git then answers with success and no include. The commit must fail, never
/// go out without the operator's signature.
#[test]
fn a_git_directory_that_cannot_be_read_stops_the_command() {
    let operator = operator(false);
    let admin = operator.common.join("worktrees/s1");
    std::fs::write(admin.join("HEAD"), "not a head\n").unwrap();
    let pin_env = [
        ("GIT_DIR", admin.as_os_str()),
        ("GIT_COMMON_DIR", operator.common.as_os_str()),
        ("GIT_WORK_TREE", operator.worktree.as_os_str()),
    ];
    let source = SettingsSource {
        pin_env: &pin_env,
        work_tree: &operator.worktree,
        env: Some(&operator.env),
    };

    let error = identity_args(&["commit-tree", "x"], &source).unwrap_err();

    assert!(format!("{error:#}").contains("identity"), "{error:#}");
}

#[test]
fn a_command_that_writes_no_commit_reads_no_settings() {
    let operator = operator(false);
    let missing = operator._tmp.path().join("missing");
    let pin_env = [("GIT_DIR", missing.as_os_str())];
    let source = SettingsSource {
        pin_env: &pin_env,
        work_tree: &operator.worktree,
        env: Some(&operator.env),
    };

    assert!(identity_args(&["status"], &source).unwrap().is_empty());
}

#[test]
fn a_later_scope_replaces_an_earlier_value_and_other_keys_and_scopes_are_ignored() {
    let listing = "system\0user.name\nfirst\0global\0core.pager\nevil\0global\0commit.gpgsign\0\
                   worktree\0user.email\nsession@example.invalid\0command\0user.email\nc@c\0\
                   local\0user.name\nsecond=with-equals\0";
    let (settings, saw_local) = collect_identity(listing);
    assert_eq!(settings, ["commit.gpgsign", "user.name=second=with-equals"]);
    assert!(saw_local);
    assert!(!collect_identity("global\0user.name\nfirst\0").1);
}
