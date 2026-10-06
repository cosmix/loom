//! Git command runner abstraction
//!
//! Provides centralized functions for running git commands with consistent
//! error handling, reducing boilerplate across the codebase.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
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

mod pinned;
pub(crate) use pinned::run_git_pinned_within;
pub use pinned::{run_git_pinned, run_git_pinned_checked};

fn git_timeout(args: &[&str]) -> Duration {
    match args.first().copied() {
        Some("clone" | "fetch" | "pull" | "push") => GIT_NETWORK_TIMEOUT,
        Some(
            "checkout" | "commit" | "merge" | "rebase" | "reset" | "restore" | "switch"
            | "worktree" | "merge-tree" | "commit-tree" | "update-ref" | "stash",
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
/// capsule denies only `.git/hooks`, `.git/config` and `config.worktree`), and
/// git honors both in `diff`, `merge-base` and every ancestry test; loom uses
/// neither. A caller's `env` is applied last, so it still wins.
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

/// `args` behind [`NO_HOOKS_ARGS`] and [`NO_COMMIT_GRAPH_ARGS`], the argv
/// every runner call executes.
fn global_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let globals = NO_HOOKS_ARGS.len() + NO_COMMIT_GRAPH_ARGS.len();
    let mut exec_args = Vec::with_capacity(globals + args.len());
    exec_args.extend_from_slice(&NO_HOOKS_ARGS);
    exec_args.extend_from_slice(&NO_COMMIT_GRAPH_ARGS);
    exec_args.extend_from_slice(args);
    exec_args
}

fn git_label(args: &[&str]) -> String {
    format!("git {}", args.first().unwrap_or(&"command"))
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
    command
        .env_clear()
        .envs(env.iter().map(|pair| (&pair.0, &pair.1)));
    harden(&mut command, &exec_args, repo_root);
    run_bounded_git(command, &exec_args, &git_label(args), timeout)
}

/// Run a git command, check for success, and return stdout as a trimmed String.
///
/// On failure, bails with the full command + directory + exit code + stdout +
/// stderr context (conventions.md git error format).
pub fn run_git_checked(args: &[&str], repo_root: &Path) -> Result<String> {
    stdout_of_success(args, run_git(args, repo_root)?, repo_root)
}

fn stdout_of_success(args: &[&str], output: Output, repo_root: &Path) -> Result<String> {
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
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
mod tests {
    use super::*;

    #[test]
    fn git_deadlines_are_operation_specific() {
        assert_eq!(git_timeout(&["status"]), GIT_READ_TIMEOUT);
        assert_eq!(git_timeout(&["merge"]), GIT_MUTATION_TIMEOUT);
        for sub in ["merge-tree", "commit-tree", "update-ref", "stash"] {
            assert_eq!(git_timeout(&[sub]), GIT_MUTATION_TIMEOUT);
        }
        assert_eq!(git_timeout(&["fetch"]), GIT_NETWORK_TIMEOUT);
        assert!(GIT_READ_TIMEOUT < GIT_MUTATION_TIMEOUT);
        assert!(GIT_MUTATION_TIMEOUT < GIT_NETWORK_TIMEOUT);
    }

    #[test]
    fn git_runner_returns_structured_timeout() {
        let repo = tempfile::tempdir().unwrap();
        let error = run_git_program(
            "sh",
            &["-c", "sleep 60"],
            &[],
            "git -c",
            repo.path(),
            Duration::from_millis(100),
        )
        .expect_err("fake git command must time out");

        let timeout = error
            .downcast_ref::<crate::process::ProcessTimeoutError>()
            .expect("caller must be able to classify a timeout");
        assert_eq!(timeout.operation(), "git -c");
    }

    /// Every hook name this fixture wires up. Each script just touches a
    /// marker file under the repo's `markers` directory when it runs.
    const HOOK_NAMES: [&str; 5] = [
        "pre-commit",
        "commit-msg",
        "pre-merge-commit",
        "post-merge",
        "post-checkout",
    ];

    /// Set up isolated from ambient host git config (never mutates
    /// process-wide environment; each command carries its own overrides).
    pub(super) fn isolated_git(root: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
            .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap()
    }

    pub(super) fn isolated_git_ok(root: &Path, args: &[&str]) {
        let output = isolated_git(root, args);
        assert!(
            output.status.success(),
            "git {args:?} failed: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A repository whose `core.hooksPath` (set via local config, so it is
    /// live for any plain `git` call) points at a fixture directory where
    /// every hook in [`HOOK_NAMES`] touches a marker file under
    /// `<root>/markers` when it runs. Also carries a `feature` branch ready
    /// to merge or check out, and leaves `main` checked out.
    #[cfg(unix)]
    fn repo_with_marker_hooks() -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        isolated_git_ok(&root, &["init", "-b", "main"]);
        isolated_git_ok(&root, &["config", "user.email", "runner-test@example.com"]);
        isolated_git_ok(&root, &["config", "user.name", "Runner Test"]);
        std::fs::write(root.join("seed.txt"), "seed").unwrap();
        isolated_git_ok(&root, &["add", "seed.txt"]);
        isolated_git_ok(&root, &["commit", "-m", "seed"]);

        // A branch to merge and check out, created outside the runner and
        // BEFORE the hook fixture is installed below - otherwise these
        // plain setup commands would themselves trigger the fixture hooks
        // (pre-commit, commit-msg, post-checkout) and pre-populate the
        // markers the tests assert on.
        isolated_git_ok(&root, &["checkout", "-b", "feature"]);
        std::fs::write(root.join("feature.txt"), "feature").unwrap();
        isolated_git_ok(&root, &["add", "feature.txt"]);
        isolated_git_ok(&root, &["commit", "-m", "feature commit"]);
        isolated_git_ok(&root, &["checkout", "main"]);

        let hooks_dir = root.join("hooks-fixture");
        let markers_dir = root.join("markers");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::create_dir_all(&markers_dir).unwrap();
        for hook in HOOK_NAMES {
            let script_path = hooks_dir.join(hook);
            let marker_path = markers_dir.join(hook);
            std::fs::write(
                &script_path,
                format!("#!/bin/sh\ntouch \"{}\"\n", marker_path.display()),
            )
            .unwrap();
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }
        isolated_git_ok(
            &root,
            &["config", "core.hooksPath", hooks_dir.to_str().unwrap()],
        );

        (temp, root)
    }

    #[cfg(unix)]
    fn assert_no_hooks_fired(markers_dir: &Path) {
        for hook in HOOK_NAMES {
            assert!(
                !markers_dir.join(hook).exists(),
                "hook '{hook}' fired through a call routed via the runner"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn run_git_checked_disables_repository_hooks_for_commit_merge_and_checkout() {
        let (_temp, root) = repo_with_marker_hooks();
        let markers_dir = root.join("markers");

        run_git_checked(&["commit", "--allow-empty", "-m", "runner commit"], &root)
            .expect("runner commit must succeed with hooks disabled");
        assert_no_hooks_fired(&markers_dir);

        run_git_checked(
            &["merge", "--no-ff", "feature", "-m", "runner merge"],
            &root,
        )
        .expect("runner merge must succeed with hooks disabled");
        assert_no_hooks_fired(&markers_dir);

        run_git_checked(&["checkout", "feature"], &root)
            .expect("runner checkout must succeed with hooks disabled");
        assert_no_hooks_fired(&markers_dir);
    }

    /// Positive control: the same commit, run with plain `git` bypassing the
    /// runner entirely, must fire the fixture hooks. This proves the
    /// absence asserted above comes from the runner's
    /// `-c core.hooksPath=/dev/null`, not from a broken fixture.
    #[cfg(unix)]
    #[test]
    fn plain_git_commit_proves_the_fixture_hooks_are_live() {
        let (_temp, root) = repo_with_marker_hooks();
        let markers_dir = root.join("markers");

        isolated_git_ok(&root, &["commit", "--allow-empty", "-m", "control commit"]);

        assert!(
            markers_dir.join("pre-commit").exists(),
            "fixture pre-commit hook must be live"
        );
        assert!(
            markers_dir.join("commit-msg").exists(),
            "fixture commit-msg hook must be live"
        );
    }
}
