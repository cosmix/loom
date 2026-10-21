//! Git calls that inspect one checkout whatever the caller's environment
//! exports.

use anyhow::Result;
use std::fs::File;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use super::{git_command, git_label, git_timeout, global_args, run_bounded_git, stdout_of_success};

/// Variables that point git at a repository, work tree or index other than the
/// ones under its working directory.
pub(crate) const REPO_REDIRECT_ENV: [&str; 4] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
];

/// [`super::run_git`] for inspecting the checkout at `repo_root` whatever the
/// caller's environment exports: `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`
/// and `GIT_COMMON_DIR` are dropped, so none can point git at another
/// repository. `stdin` is the file git reads as standard input, which needs no
/// writer thread beside the deadline.
pub fn run_git_pinned(args: &[&str], stdin: Option<File>, repo_root: &Path) -> Result<Output> {
    run_git_pinned_within(args, stdin, repo_root, git_timeout(args))
}

/// [`run_git_pinned`] with an explicit `timeout`.
pub(crate) fn run_git_pinned_within(
    args: &[&str],
    stdin: Option<File>,
    repo_root: &Path,
    timeout: Duration,
) -> Result<Output> {
    let exec_args = global_args(args);
    let command = pinned_command("git", &exec_args, stdin, repo_root);
    run_bounded_git(command, &exec_args, &git_label(args), timeout)
}

/// [`super::run_git_checked`] through [`run_git_pinned`].
pub fn run_git_pinned_checked(args: &[&str], repo_root: &Path) -> Result<String> {
    stdout_of_success(args, run_git_pinned(args, None, repo_root)?, repo_root)
}

fn pinned_command(
    program: &str,
    exec_args: &[&str],
    stdin: Option<File>,
    repo_root: &Path,
) -> Command {
    let mut command = git_command(program, exec_args, &[], repo_root);
    for variable in REPO_REDIRECT_ENV {
        command.env_remove(variable);
    }
    if let Some(input) = stdin {
        command.stdin(input);
    }
    command
}

#[cfg(test)]
mod tests;
