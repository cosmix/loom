//! Shared git fixtures: one isolated runner and one repository builder for every test.
//!
//! Every git child started here ignores the host's git configuration, its default global
//! excludes and attributes files, and any inherited `GIT_*` variable, so a developer's global
//! hooks, default branch name, ignore rules or `GIT_DIR` cannot change what a fixture builds. The file depends only on `std` and `tempfile`: the library
//! compiles it as `test_git` and each integration crate reaches it through `test_support`.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub const DEFAULT_BRANCH: &str = "main";
pub const DEFAULT_NAME: &str = "Test User";
pub const DEFAULT_EMAIL: &str = "test@test.com";

const DEFAULT_SEED_PATH: &str = "README.md";
const DEFAULT_SEED_CONTENT: &str = "# Test Repository\n";
const DEFAULT_SEED_MESSAGE: &str = "Initial commit";

/// Strip every inherited GIT_* variable, then GIT_CONFIG_GLOBAL=/dev/null, GIT_CONFIG_NOSYSTEM=1
/// and an XDG_CONFIG_HOME that cannot be a directory. The last one shuts out git's default
/// `core.excludesFile` and `core.attributesFile` (`$XDG_CONFIG_HOME/git/ignore` and
/// `git/attributes`, or under `$HOME/.config` only when the variable is unset or empty), which
/// the config variables do not reach. Call before adding the caller's own variables
/// (env_remove also drops explicitly set ones).
pub fn isolate(command: &mut Command) -> &mut Command {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("XDG_CONFIG_HOME", "/dev/null/loom-no-xdg-config")
}

/// `program` in `dir` under the fixture isolation: the one place the policy is written.
fn command_for(program: &Path, dir: &Path) -> Command {
    let mut command = Command::new(program);
    isolate(&mut command);
    // Deliberately repeats `clear_loom_env` (tests/test_support/mod.rs): the lib includes
    // this file as `test_git` and cannot reach `super::`.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command.env("LC_ALL", "C").current_dir(dir);
    command
}

/// `git` in `dir`: isolate(), every inherited LOOM_* variable removed, LC_ALL=C, current_dir(dir).
pub fn git_command(dir: &Path) -> Command {
    command_for(Path::new("git"), dir)
}

fn run_command(mut command: Command, dir: &Path, args: &[&str]) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("git {args:?} in {}: {error}", dir.display()))
}

fn git_with_program(program: &Path, dir: &Path, args: &[&str]) -> Output {
    let mut command = command_for(program, dir);
    command.args(args);
    run_command(command, dir, args)
}

/// Run git; return the Output whatever the status.
pub fn git_unchecked(dir: &Path, args: &[&str]) -> Output {
    git_with_program(Path::new("git"), dir, args)
}

/// Run git; panic naming args, dir, exit status, stdout and stderr unless it succeeded.
pub fn git_checked(dir: &Path, args: &[&str]) -> Output {
    let output = git_unchecked(dir, args);
    check_git(output, dir, args)
}

fn check_git(output: Output, dir: &Path, args: &[&str]) -> Output {
    assert!(
        output.status.success(),
        "git {args:?} in {} exited {}\nstdout: {}\nstderr: {}",
        dir.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn trimmed_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn failed_git_mutation_reports_command_and_output() {
    // The stand-in runs from a shell string: a script written here and executed
    // could fail with ETXTBSY when another test thread forks while the write
    // descriptor is open.
    const FAILING_GIT: &str = "echo fixture-error >&2; exit 42";
    let temp = tempfile::tempdir().unwrap();
    for action in ["commit", "merge"] {
        let args = ["-c", FAILING_GIT, action, "fixture"];
        let result = std::panic::catch_unwind(|| {
            check_git(
                git_with_program(Path::new("/bin/sh"), temp.path(), &args),
                temp.path(),
                &args,
            )
        });
        let panic = result.expect_err("failed git must reject fixture setup");
        let message = panic.downcast_ref::<String>().expect("panic message");
        assert!(
            message.contains(action) && message.contains("42") && message.contains("fixture-error"),
            "{message}"
        );
    }
}

/// git_checked, then stdout (lossy UTF-8) trimmed.
pub fn git(dir: &Path, args: &[&str]) -> String {
    trimmed_stdout(&git_checked(dir, args))
}

/// git with extra variables applied after the isolation; checked; trimmed stdout.
pub fn git_with_env<V: AsRef<OsStr>>(dir: &Path, args: &[&str], env: &[(&str, V)]) -> String {
    let mut command = git_command(dir);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = check_git(run_command(command, dir, args), dir, args);
    trimmed_stdout(&output)
}

/// Create parent dirs, write `path` (relative to dir), `git add -- path`, `git commit -q -m message`;
/// return the new HEAD sha.
pub fn commit_file(dir: &Path, path: &str, content: &str, message: &str) -> String {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("create {} for {path}: {error}", parent.display()));
    }
    std::fs::write(&file, content)
        .unwrap_or_else(|error| panic!("write {}: {error}", file.display()));
    git_checked(dir, &["add", "--", path]);
    git_checked(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A temporary repository: the `TempDir` plus its canonical root.
pub struct TestRepo {
    temp: TempDir,
    root: PathBuf,
}

impl TestRepo {
    pub fn new() -> TestRepo {
        TestRepo::builder().create()
    }

    pub fn builder() -> RepoBuilder {
        RepoBuilder::default()
    }

    /// Canonicalized TempDir path.
    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn git(&self, args: &[&str]) -> String {
        git(&self.root, args)
    }

    pub fn commit_file(&self, path: &str, content: &str, message: &str) -> String {
        commit_file(&self.root, path, content, message)
    }

    pub fn into_temp_dir(self) -> TempDir {
        self.temp
    }
}

impl Default for TestRepo {
    fn default() -> Self {
        TestRepo::new()
    }
}

/// Options for a fixture repository; the defaults are documented on `init_in`.
pub struct RepoBuilder {
    branch: String,
    name: String,
    email: String,
    seed: Option<(String, String, String)>,
}

impl Default for RepoBuilder {
    fn default() -> Self {
        RepoBuilder {
            branch: DEFAULT_BRANCH.to_string(),
            name: DEFAULT_NAME.to_string(),
            email: DEFAULT_EMAIL.to_string(),
            seed: Some((
                DEFAULT_SEED_PATH.to_string(),
                DEFAULT_SEED_CONTENT.to_string(),
                DEFAULT_SEED_MESSAGE.to_string(),
            )),
        }
    }
}

impl RepoBuilder {
    pub fn branch(mut self, name: &str) -> Self {
        self.branch = name.to_string();
        self
    }

    pub fn identity(mut self, name: &str, email: &str) -> Self {
        self.name = name.to_string();
        self.email = email.to_string();
        self
    }

    pub fn seed(mut self, path: &str, content: &str, message: &str) -> Self {
        self.seed = Some((path.to_string(), content.to_string(), message.to_string()));
        self
    }

    pub fn no_seed(mut self) -> Self {
        self.seed = None;
        self
    }

    /// In an existing directory: git init -q -b <branch>; config user.name, user.email,
    /// commit.gpgsign false; then the seed commit (commit_file) unless no_seed().
    pub fn init_in(self, dir: &Path) {
        git_checked(dir, &["init", "-q", "-b", &self.branch]);
        git_checked(dir, &["config", "user.name", &self.name]);
        git_checked(dir, &["config", "user.email", &self.email]);
        git_checked(dir, &["config", "commit.gpgsign", "false"]);
        if let Some((path, content, message)) = &self.seed {
            commit_file(dir, path, content, message);
        }
    }

    /// TempDir::new(), canonicalize, init_in(root).
    pub fn create(self) -> TestRepo {
        let temp = TempDir::new().expect("create temp directory for repository");
        let root = temp.path().canonicalize().expect("canonicalize temp root");
        self.init_in(&root);
        TestRepo { temp, root }
    }
}
