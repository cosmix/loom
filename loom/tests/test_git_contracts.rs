//! Contracts for the shared git fixture module `tests/test_support/git_repo.rs`.
//!
//! Each test pins one isolation property or API behaviour the fixture sweep copies into
//! every repository-building test. Tests that change the process environment restore it
//! through `EnvGuard` before asserting, so a failed assertion never leaks a variable.

#[path = "test_support/mod.rs"]
mod test_support;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serial_test::serial;
use test_support::git_repo::{
    commit_file, git, git_command, git_unchecked, git_with_env, isolate, TestRepo,
};

/// Sets process environment variables and puts back their previous values on drop.
struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    fn new() -> Self {
        Self { saved: Vec::new() }
    }

    fn set(&mut self, key: &str, value: impl AsRef<OsStr>) {
        self.saved.push((key.to_string(), std::env::var_os(key)));
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..).rev() {
            match previous {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}

/// Reads repository state with a plain `git`, independent of the module under test.
/// Only the variables that would redirect it to another repository are removed.
fn plain_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} in {} exited {}\nstderr: {}",
        dir.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The value `command` will set for `key`: `None` when the command leaves it inherited,
/// `Some(None)` when it removes it, `Some(Some(v))` when it sets it to `v`.
fn env_of(command: &Command, key: &str) -> Option<Option<OsString>> {
    command
        .get_envs()
        .find(|(name, _)| *name == OsStr::new(key))
        .map(|(_, value)| value.map(OsStr::to_os_string))
}

fn set_to(value: &str) -> Option<Option<OsString>> {
    Some(Some(OsString::from(value)))
}

/// Writes a git config that renames the default branch and points hooks at a directory
/// whose pre-commit hook always fails.
fn write_hostile_config(file: &Path, hooks: &Path) {
    fs::create_dir_all(file.parent().expect("config parent")).expect("create config dir");
    let body = format!(
        "[init]\n\tdefaultBranch = master\n[core]\n\thooksPath = \"{}\"\n",
        hooks.display()
    );
    fs::write(file, body).expect("write config");
}

#[test]
#[serial]
fn ambient_global_config_does_not_reach_fixture_git() {
    let hooks = tempfile::tempdir().unwrap();
    let hook = hooks.path().join("pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let global = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let global_file = global.path().join("gitconfig");
    write_hostile_config(&global_file, hooks.path());
    write_hostile_config(&home.path().join(".gitconfig"), hooks.path());
    write_hostile_config(&xdg.path().join("git").join("config"), hooks.path());

    let mut env = EnvGuard::new();
    env.set("GIT_CONFIG_GLOBAL", &global_file);
    env.set("HOME", home.path());
    env.set("XDG_CONFIG_HOME", xdg.path());
    let repo = std::panic::catch_unwind(TestRepo::new);
    drop(env);

    let repo = repo.expect("ambient config must not block the fixture's seed commit");
    assert_eq!(
        plain_git(repo.path(), &["rev-parse", "--abbrev-ref", "HEAD"]),
        "main"
    );
    assert_eq!(
        plain_git(repo.path(), &["log", "--format=%s"]),
        "Initial commit"
    );
}

/// Writes `<root>/<rel>` with `body`, creating parent directories.
fn write_ambient(root: &Path, rel: &str, body: &str) {
    let file = root.join(rel);
    fs::create_dir_all(file.parent().expect("ambient parent")).expect("create ambient dir");
    fs::write(file, body).expect("write ambient file");
}

/// Runs `status`, `check-ignore`, `check-attr` and `add` on the two planted
/// files in `repo`, with an ambient `$XDG_CONFIG_HOME/git/{ignore,attributes}`
/// and `$HOME/.config/git/{ignore,attributes}` that list them.
fn git_under_ambient_defaults(repo: &Path) -> [std::process::Output; 4] {
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    write_ambient(xdg.path(), "git/ignore", "xdg-ignored.txt\n");
    write_ambient(xdg.path(), "git/attributes", "xdg-ignored.txt text\n");
    write_ambient(home.path(), ".config/git/ignore", "home-ignored.txt\n");
    write_ambient(
        home.path(),
        ".config/git/attributes",
        "home-ignored.txt text\n",
    );
    for name in ["xdg-ignored.txt", "home-ignored.txt"] {
        fs::write(repo.join(name), "x\n").unwrap();
    }

    let mut env = EnvGuard::new();
    env.set("XDG_CONFIG_HOME", xdg.path());
    env.set("HOME", home.path());
    let files = ["xdg-ignored.txt", "home-ignored.txt"];
    let outputs = [
        git_unchecked(repo, &["status", "--porcelain"]),
        git_unchecked(repo, &["check-ignore", files[0], files[1]]),
        git_unchecked(repo, &["check-attr", "text", files[0], files[1]]),
        git_unchecked(repo, &["add", "--", files[0], files[1]]),
    ];
    drop(env);
    outputs
}

#[test]
#[serial]
fn ambient_default_excludes_and_attributes_do_not_reach_fixture_git() {
    let repo = TestRepo::new();
    let [status, ignored, attrs, added] = git_under_ambient_defaults(repo.path());

    let stdout =
        |output: &std::process::Output| String::from_utf8_lossy(&output.stdout).to_string();
    let status_text = stdout(&status);
    assert!(
        status_text.contains("?? xdg-ignored.txt") && status_text.contains("?? home-ignored.txt"),
        "ambient default excludes must not hide files: {status_text}"
    );
    assert_eq!(
        ignored.status.code(),
        Some(1),
        "check-ignore must match nothing: {}",
        stdout(&ignored)
    );
    let attr_text = stdout(&attrs);
    assert_eq!(
        attr_text.matches("text: unspecified").count(),
        2,
        "{attr_text}"
    );
    assert!(
        added.status.success() && added.stderr.is_empty(),
        "git add must succeed without warnings: {}",
        String::from_utf8_lossy(&added.stderr)
    );
    for output in [&status, &ignored, &attrs] {
        assert!(
            output.stderr.is_empty(),
            "isolation must not make git warn: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[serial]
fn git_command_environment_is_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let mut env = EnvGuard::new();
    env.set("GIT_FOO", "1");
    env.set("LOOM_BAR", "1");
    let runner = git_command(dir.path());
    let mut bare = Command::new("git");
    isolate(&mut bare);
    drop(env);

    for (label, command) in [("git_command", &runner), ("isolate", &bare)] {
        assert_eq!(
            env_of(command, "GIT_CONFIG_GLOBAL"),
            set_to("/dev/null"),
            "{label}"
        );
        assert_eq!(
            env_of(command, "GIT_CONFIG_NOSYSTEM"),
            set_to("1"),
            "{label}"
        );
        assert_eq!(
            env_of(command, "GIT_FOO"),
            Some(None),
            "{label} must remove GIT_FOO"
        );
    }
    assert_eq!(
        env_of(&runner, "LOOM_BAR"),
        Some(None),
        "git_command must remove LOOM_BAR"
    );
    assert_eq!(env_of(&runner, "LC_ALL"), set_to("C"));
    assert_eq!(runner.get_current_dir(), Some(dir.path()));
}

#[test]
#[serial]
fn no_seed_leaves_unborn_main_and_unchecked_returns_failure() {
    let repo = TestRepo::builder().no_seed().create();
    assert_eq!(
        plain_git(repo.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/main"
    );
    assert_eq!(
        plain_git(repo.path(), &["config", "--local", "--get", "user.email"]),
        "test@test.com"
    );
    let output = git_unchecked(repo.path(), &["rev-parse", "--verify", "HEAD"]);
    assert!(
        !output.status.success(),
        "no_seed() must leave HEAD unborn, got {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
#[serial]
fn git_with_env_applies_after_isolation_and_commit_file_returns_head() {
    let repo = TestRepo::new();
    let path = repo.path();
    let mut env = EnvGuard::new();
    env.set("GIT_AUTHOR_NAME", "Ambient");
    let ident = std::panic::catch_unwind(|| {
        git_with_env(
            path,
            &["var", "GIT_AUTHOR_IDENT"],
            &[("GIT_AUTHOR_NAME", "Zed")],
        )
    });
    drop(env);

    let ident = ident.expect("git var GIT_AUTHOR_IDENT");
    assert!(ident.starts_with("Zed"), "{ident}");
    let sha = commit_file(path, "sub/x.txt", "x\n", "x");
    assert_eq!(sha, plain_git(path, &["rev-parse", "HEAD"]));
    assert_eq!(fs::read_to_string(path.join("sub/x.txt")).unwrap(), "x\n");
}

#[test]
#[serial]
fn fixture_identity_lives_in_repository_config() {
    let default = TestRepo::new();
    let custom = TestRepo::builder()
        .identity("Ann", "ann@example.invalid")
        .create();
    let cases = [
        (default.path(), "test@test.com", "Test User"),
        (custom.path(), "ann@example.invalid", "Ann"),
    ];
    for (path, email, name) in cases {
        let read = |key: &str| plain_git(path, &["config", "--local", "--get", key]);
        assert_eq!(read("user.email"), email, "{}", path.display());
        assert_eq!(read("user.name"), name, "{}", path.display());
        assert_eq!(read("commit.gpgsign"), "false", "{}", path.display());
    }
}

#[test]
#[serial]
fn inherited_git_dir_does_not_redirect_fixture_git() {
    let a = TestRepo::new();
    let mut env = EnvGuard::new();
    env.set("GIT_DIR", a.path().join(".git"));
    env.set("GIT_WORK_TREE", a.path());
    let b = std::panic::catch_unwind(|| TestRepo::builder().seed("b.txt", "b\n", "b").create());
    drop(env);

    let b = b.expect("inherited GIT_DIR must not break the fixture");
    assert_eq!(plain_git(b.path(), &["log", "--format=%s"]), "b");
    assert_eq!(
        plain_git(a.path(), &["log", "--format=%s"]),
        "Initial commit"
    );
}

#[test]
#[serial]
fn checked_git_failure_names_command_and_stderr() {
    let repo = TestRepo::new();
    let path = repo.path();
    let result =
        std::panic::catch_unwind(|| git(path, &["rev-parse", "--verify", "refs/heads/absent"]));
    let panic = result.expect_err("a failed git command must panic");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .expect("panic message");
    assert!(
        message.contains("rev-parse") && message.contains("fatal:"),
        "{message}"
    );
}

#[test]
#[serial]
fn builder_branch_and_seed_overrides_apply() {
    let repo = TestRepo::builder()
        .branch("trunk")
        .seed("dir/a.txt", "seed\n", "seed")
        .create();
    assert_eq!(
        plain_git(repo.path(), &["rev-parse", "--abbrev-ref", "HEAD"]),
        "trunk"
    );
    assert_eq!(plain_git(repo.path(), &["ls-files"]), "dir/a.txt");
    assert_eq!(plain_git(repo.path(), &["log", "--format=%s"]), "seed");

    let recorded = repo.path().to_path_buf();
    let temp = repo.into_temp_dir();
    assert_eq!(fs::canonicalize(temp.path()).unwrap(), recorded);
    assert_eq!(git(&recorded, &["log", "--format=%s"]), "seed");
    drop(temp);
}
