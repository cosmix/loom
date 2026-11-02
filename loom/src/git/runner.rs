//! Git command runner abstraction
//!
//! Provides centralized functions for running git commands with consistent
//! error handling, reducing boilerplate across the codebase.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

pub(crate) const GIT_READ_TIMEOUT: Duration = Duration::from_secs(15);
const GIT_MUTATION_TIMEOUT: Duration = Duration::from_secs(120);
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(300);

/// Global git args that switch off the two config keys through which a
/// checked-out tree makes git run its code, on every git command loom runs
/// itself. Must precede the subcommand in argv.
///
/// - `core.hooksPath` here is a TRACKED directory (`loom/.githooks`), so a
///   stage or branch can plant an executable hook there that loom's own
///   merges, commits, and worktree operations would run, unsandboxed.
/// - `core.fsmonitor` names a command git runs on every index read (`status`,
///   `ls-files`, `diff`), and any `.git/config` can set it, so a directory
///   loom merely inspects could run code as the user.
pub const NO_HOOKS_ARGS: [&str; 4] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
];

/// Global git args that make git read commits from their objects. A stage
/// session can write `<common git dir>/objects/info/commit-graph`, a cache of
/// each commit's parents and tree that git trusts without reading the
/// commit, so a forged graph would change every ancestry test and diff loom
/// runs. Must precede the subcommand in argv.
const NO_COMMIT_GRAPH_ARGS: [&str; 2] = ["-c", "core.commitGraph=false"];

/// Global git args that keep loom's own commands from starting automatic
/// maintenance. A detached `gc` runs past every deadline here, opens `gc.pid`
/// and `gc.log` by path, and replaces `packed-refs`, which drops the read-only
/// bind a running session's capsule holds on that file.
const NO_MAINTENANCE_ARGS: [&str; 4] = ["-c", "gc.auto=0", "-c", "maintenance.auto=false"];

/// Global options whose value is the next argument. Every other argument
/// before the subcommand that starts with `-` stands alone.
const GLOBAL_OPTIONS_WITH_VALUE: [&str; 7] = [
    "-c",
    "-C",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--attr-source",
];

/// The position of the subcommand in `args`, past every global option; the
/// length of `args` when there is none (`--version`).
fn subcommand_index(args: &[&str]) -> usize {
    let mut index = 0;
    while let Some(arg) = args.get(index).filter(|arg| arg.starts_with('-')) {
        index += if GLOBAL_OPTIONS_WITH_VALUE.contains(arg) {
            2
        } else {
            1
        };
    }
    index.min(args.len())
}

/// The subcommand of `args`, past every global option.
pub(crate) fn subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    args.get(subcommand_index(args)).copied()
}

mod pinned;
mod planted_links;
mod stream;
pub use pinned::{run_git_pinned, run_git_pinned_checked};
pub(crate) use pinned::{run_git_pinned_within, REPO_REDIRECT_ENV};
use planted_links::refuse_links_in_discovered_git_dir;
pub(crate) use planted_links::{refuse_planted_links, refuse_ref_path_links};
pub(crate) use stream::run_git_stream_with_env;

fn git_timeout(args: &[&str]) -> Duration {
    match subcommand(args) {
        Some("clone" | "fetch" | "pull" | "push") => GIT_NETWORK_TIMEOUT,
        Some(
            "checkout" | "commit" | "merge" | "rebase" | "reset" | "restore" | "switch"
            | "worktree" | "merge-tree" | "commit-tree" | "update-ref" | "stash" | "update-index"
            | "read-tree" | "write-tree",
        ) => GIT_MUTATION_TIMEOUT,
        _ => GIT_READ_TIMEOUT,
    }
}

/// The graft file every runner call names: it can never exist (`/dev/null`
/// is not a directory), so git reads no grafts. `/dev/null` itself also
/// disables them, but git opens it and prints its grafts deprecation advice
/// on stderr, which loom parses.
const NO_GRAFT_FILE: &str = "/dev/null/loom-no-grafts";

/// The git command every runner call executes: English output, and replace
/// refs and grafts ignored. A stage session can write
/// `<common git dir>/refs/replace/*` and `<common git dir>/info/grafts` (its
/// capsule denies `<common git dir>/hooks`, `<common git dir>/config`, every
/// sibling's `<common git dir>/worktrees/<name>` and the session's own
/// `config.worktree` and the reflogs; `refs/` and `info/` stay writable), and git honors both
/// in `diff`, `merge-base` and every ancestry test; loom uses neither. A
/// caller's `env` is applied last, so it still wins.
fn git_command(
    program: &str,
    exec_args: &[&str],
    env: &[(&str, &OsStr)],
    repo_root: &Path,
) -> Command {
    let mut command = Command::new(program);
    harden(&mut command, exec_args, repo_root);
    command.envs(env.iter().copied());
    command
}

/// The argv, variables and directory of [`git_command`], set on `command`.
fn harden(command: &mut Command, exec_args: &[&str], repo_root: &Path) {
    for variable in REPO_REDIRECT_ENV {
        command.env_remove(variable);
    }
    command
        .args(exec_args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", NO_GRAFT_FILE)
        .current_dir(repo_root);
}

fn run_bounded_git(
    mut command: Command,
    exec_args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<Output> {
    refuse_links_in_discovered_git_dir(&command, exec_args)?;
    crate::process::run_bounded_output(&mut command, timeout, label.to_string())
        .with_context(|| format!("Failed to execute: git {}", exec_args.join(" ")))
}

fn run_git_program(
    program: &str,
    exec_args: &[&str],
    env: &[(&str, &OsStr)],
    label: &str,
    repo_root: &Path,
    timeout: Duration,
) -> Result<Output> {
    let command = git_command(program, exec_args, env, repo_root);
    run_bounded_git(command, exec_args, label, timeout)
}

/// Subcommands that compare the work tree and, for a gitlink, run `git status`
/// in its directory. That child opens the directory's `.git`, which a checkout's
/// author controls, and runs what its configuration names.
const WORKTREE_COMPARISONS: [&str; 4] = ["status", "diff", "diff-index", "diff-files"];

/// `args` with gitlink recursion off: `--ignore-submodules=dirty` still reports
/// a gitlink whose commit moved but starts no process inside it. It is passed as
/// an option because an `ignore` in the checkout's `.gitmodules` overrides
/// configuration. A caller's weaker `--ignore-submodules` value is dropped;
/// `all`, stricter, is kept and wins by coming later.
fn without_gitlink_recursion<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let position = subcommand_index(args);
    let compares = args
        .get(position)
        .is_some_and(|name| WORKTREE_COMPARISONS.contains(name));
    if !compares {
        return args.to_vec();
    }
    let (head, rest) = args.split_at(position + 1);
    let options = rest
        .iter()
        .position(|arg| *arg == "--")
        .unwrap_or(rest.len());
    let weaker =
        |arg: &&str| arg.starts_with("--ignore-submodules=") && *arg != "--ignore-submodules=all";
    let mut hardened = head.to_vec();
    hardened.push("--ignore-submodules=dirty");
    hardened.extend(rest[..options].iter().copied().filter(|arg| !weaker(arg)));
    hardened.extend_from_slice(&rest[options..]);
    hardened
}

/// `args` behind [`NO_HOOKS_ARGS`], [`NO_COMMIT_GRAPH_ARGS`] and
/// [`NO_MAINTENANCE_ARGS`], the argv every runner call executes.
fn global_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let args = without_gitlink_recursion(args);
    let globals = NO_HOOKS_ARGS.len() + NO_COMMIT_GRAPH_ARGS.len() + NO_MAINTENANCE_ARGS.len();
    let mut exec_args = Vec::with_capacity(globals + args.len());
    exec_args.extend_from_slice(&NO_HOOKS_ARGS);
    exec_args.extend_from_slice(&NO_COMMIT_GRAPH_ARGS);
    exec_args.extend_from_slice(&NO_MAINTENANCE_ARGS);
    exec_args.extend_from_slice(&args);
    exec_args
}

fn git_label(args: &[&str]) -> String {
    format!("git {}", subcommand(args).unwrap_or("command"))
}

/// Run a git command in `repo_root` and return the raw Output.
///
/// `git_command` sets `LC_ALL=C` and `LANG=C`, so stdout/stderr parsing is
/// locale-independent. Use this when you need both stdout and stderr, or
/// custom error handling.
pub fn run_git(args: &[&str], repo_root: &Path) -> Result<Output> {
    run_git_with_env(args, &[], repo_root)
}

/// [`run_git`] with `env` added to this one command's environment.
pub fn run_git_with_env(args: &[&str], env: &[(&str, &OsStr)], repo_root: &Path) -> Result<Output> {
    run_git_with_env_within(repo_root, args, env, git_timeout(args))
}

/// [`run_git_with_env`] with git reading `stdin`.
pub(crate) fn run_git_with_stdin(
    args: &[&str],
    env: &[(&str, &OsStr)],
    stdin: File,
    repo_root: &Path,
) -> Result<Output> {
    let exec_args = global_args(args);
    let mut command = git_command("git", &exec_args, env, repo_root);
    command.stdin(stdin);
    run_bounded_git(command, &exec_args, &git_label(args), git_timeout(args))
}

/// [`run_git_with_env`] bounded by `timeout` instead of the subcommand's
/// default deadline (a signing call waits on an agent, not on git).
pub(crate) fn run_git_with_env_within(
    repo_root: &Path,
    args: &[&str],
    env: &[(&str, &OsStr)],
    timeout: Duration,
) -> Result<Output> {
    let exec_args = global_args(args);
    run_git_program("git", &exec_args, env, &git_label(args), repo_root, timeout)
}

/// [`run_git_with_env_within`] from an empty environment: git sees `env` and
/// the runner's own variables (which win), nothing this process exports.
pub(crate) fn run_git_in_env_within(
    repo_root: &Path,
    args: &[&str],
    env: &[(OsString, OsString)],
    timeout: Duration,
) -> Result<Output> {
    let exec_args = global_args(args);
    let mut command = Command::new("git");
    command.env_clear();
    harden(&mut command, &exec_args, repo_root);
    command.envs(env.iter().map(|pair| (&pair.0, &pair.1)));
    command
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_GRAFT_FILE", NO_GRAFT_FILE);
    run_bounded_git(command, &exec_args, &git_label(args), timeout)
}

/// Run a git command, check for success, and return stdout as a trimmed String.
///
/// On failure, bails with the full command + directory + exit code + stdout +
/// stderr context (conventions.md git error format).
pub fn run_git_checked(args: &[&str], repo_root: &Path) -> Result<String> {
    stdout_of_success(args, run_git(args, repo_root)?, repo_root)
}

/// Checked binary output, preserving whitespace and arbitrary blob bytes.
pub fn run_git_checked_bytes(args: &[&str], repo_root: &Path) -> Result<Vec<u8>> {
    bytes_of_success(args, run_git(args, repo_root)?, repo_root)
}

/// Checked UTF-8 records without trimming; invalid path encodings are explicit errors.
pub(crate) fn run_git_records(args: &[&str], repo_root: &Path) -> Result<String> {
    String::from_utf8(run_git_checked_bytes(args, repo_root)?)
        .context("git records contain a non-UTF-8 path")
}

fn stdout_of_success(args: &[&str], output: Output, repo_root: &Path) -> Result<String> {
    Ok(
        String::from_utf8_lossy(&bytes_of_success(args, output, repo_root)?)
            .trim()
            .to_string(),
    )
}

fn bytes_of_success(args: &[&str], output: Output, repo_root: &Path) -> Result<Vec<u8>> {
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        bail!(
            "git {} failed (exit code {exit_code}):\n\
             Command: git {}\n\
             Directory: {}\n\
             Stdout: {}\n\
             Stderr: {}",
            args.first().unwrap_or(&""),
            args.join(" "),
            repo_root.display(),
            if stdout.trim().is_empty() {
                "(empty)"
            } else {
                stdout.trim()
            },
            if stderr.trim().is_empty() {
                "(empty)"
            } else {
                stderr.trim()
            },
        );
    }
    Ok(output.stdout)
}

/// Run a git command and return true if exit code is 0.
///
/// Silently swallows errors (both spawn failures and non-zero exits).
/// Use this for status checks like `branch_exists`, `rev-parse --verify`, etc.
pub fn run_git_bool(args: &[&str], repo_root: &Path) -> bool {
    run_git(args, repo_root)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod commit_graph_tests;

#[cfg(test)]
mod global_option_tests;

#[cfg(test)]
mod tests;
