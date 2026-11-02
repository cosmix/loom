//! Shared test helpers for dependency inheritance integration tests

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

#[path = "../test_support/mod.rs"]
pub mod test_support;

// Several test targets load this file through `#[path]` without using this name.
#[allow(unused_imports)]
pub use test_support::write_main_checkout_identity;

pub fn git_checked(dir: &Path, args: &[&str]) -> std::process::Output {
    test_support::git_checked(dir, args)
}

pub fn git_unchecked(dir: &Path, args: &[&str]) -> std::process::Output {
    test_support::git_unchecked(dir, args)
}

use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::StageDefinition;

/// Test helper: Create a temporary git repository with initial commit
pub fn init_test_repo() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let repo_root = temp_dir.path();

    test_support::init_git_repo(repo_root);
    test_support::git_checked(repo_root, &["branch", "-M", "main"]);

    temp_dir
}

/// Test helper: Create a branch with a commit adding a file
pub fn create_branch_with_file(name: &str, filename: &str, content: &str, repo_root: &Path) {
    test_support::git_checked(repo_root, &["checkout", "-b", name]);
    fs::write(repo_root.join(filename), content).expect("Failed to write file");
    test_support::git_checked(repo_root, &["add", filename]);
    test_support::git_checked(repo_root, &["commit", "-m", &format!("Add {filename}")]);
    test_support::git_checked(repo_root, &["checkout", "main"]);
}

/// Test helper: Build execution graph from stage definitions
pub fn build_test_graph(stages: Vec<(&str, Vec<&str>)>) -> ExecutionGraph {
    let stage_defs: Vec<StageDefinition> = stages
        .into_iter()
        .map(|(id, deps)| StageDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: Some(format!("Test stage {id}")),
            dependencies: deps.into_iter().map(String::from).collect(),
            working_dir: ".".to_string(),
            ..Default::default()
        })
        .collect();

    ExecutionGraph::build(stage_defs).expect("Failed to build graph")
}

/// Test helper: Mark stage as completed in graph
pub fn complete_stage(graph: &mut ExecutionGraph, stage_id: &str) {
    graph
        .mark_executing(stage_id)
        .expect("Failed to mark executing");
    graph
        .mark_completed(stage_id)
        .expect("Failed to mark completed");
}

/// Test helper: Verify worktree contains file from dependency branch
pub fn verify_worktree_has_file(worktree_path: &Path, filename: &str) -> bool {
    worktree_path.join(filename).exists()
}

/// Test helper: Merge a branch into main
pub fn merge_into_main(branch: &str, repo_root: &Path) {
    test_support::git_checked(repo_root, &["checkout", "main"]);
    test_support::git_checked(
        repo_root,
        &["merge", "--no-ff", "-m", &format!("Merge {branch}"), branch],
    );
}

/// Test helper: Delete a branch
pub fn delete_branch(branch: &str, repo_root: &Path) {
    test_support::git_checked(repo_root, &["branch", "-D", branch]);
}

/// Path to the `loom` binary this test run built.
const LOOM: &str = env!("CARGO_BIN_EXE_loom");

/// The path [`LOOM`] resolves to, for a caller that must hand it to a
/// subprocess as an env var (e.g. `LOOM_BIN` for a hook script) rather than
/// spawn it directly — a direct spawn outside this module is what
/// `binary_spawn_guard` forbids.
pub fn loom_bin_path() -> &'static str {
    LOOM
}

/// A `loom` command whose user directory is a scratch `LOOM_HOME` with the
/// update check switched off.
///
/// These tests spawn the real binary, and every loom invocation that is not on
/// `main.rs`'s update-check exclusion list reads the user's `~/.loom` and, when
/// that record is stale, spawns a detached child that makes a real network
/// request (see `loom::update_check`). Left alone, the suite would refresh the
/// developer's own `update-state.json`, leak an orphaned fetcher, and print an
/// update notice into the stderr these tests assert on. `LOOM_HOME` is the same
/// seam `tests/e2e/daemon_config/mod.rs` uses to stay off the real user config.
///
/// The binary under test is not built with `cfg(test)`, so it reads its
/// `LOOM_*` session variables from the real process environment — the same
/// place [`clear_relay_env`] scrubs before every spawn here. Without this, a
/// `cargo test` process that happens to run inside a real loom session (this
/// suite's own integration-verify stage, for one) would leak that session's
/// identity into every test's supposedly clean CLI invocation.
///
/// The `TempDir` lives in a `static OnceLock` so every test in this binary
/// shares one scratch home; since statics are never destructed at process
/// exit, Rust does not remove this directory. The host-load runner owns the
/// outer temporary root and must remove it after the test binary and its
/// children retire. Direct Cargo invocations leave this scratch home behind.
pub fn loom_cmd() -> Command {
    static SCRATCH_HOME: OnceLock<TempDir> = OnceLock::new();
    let home = SCRATCH_HOME.get_or_init(|| {
        let dir = TempDir::new().expect("create scratch LOOM_HOME");
        fs::write(dir.path().join("config.toml"), "[update]\ncheck = false\n")
            .expect("write scratch user config");
        dir
    });
    let mut command = Command::new(LOOM);
    clear_relay_env(&mut command);
    command.env("LOOM_HOME", home.path());
    command
}

/// Remove inherited Loom variables; scenarios may add explicit values afterward.
pub fn clear_relay_env(command: &mut Command) -> &mut Command {
    test_support::clear_loom_env(command);
    command
}

/// Feed a finite payload from a file and capture both streams under a deadline.
pub fn run_hook_process(command: &mut Command, input: &[u8]) -> std::process::Output {
    run_hook_process_with_timeout(command, input, std::time::Duration::from_secs(15))
        .unwrap_or_else(|error| panic!("hook process {command:?}: {error:#}"))
}

fn run_hook_process_with_timeout(
    command: &mut Command,
    input: &[u8],
    timeout: std::time::Duration,
) -> anyhow::Result<std::process::Output> {
    use std::io::{Seek, Write};
    let mut payload = tempfile::tempfile().expect("create hook input");
    payload.write_all(input).expect("write hook input");
    payload.rewind().expect("rewind hook input");
    command.stdin(Stdio::from(payload));
    loom::process::run_bounded_output(command, timeout, "hook test")
}

#[test]
fn hook_runner_drains_large_output_and_times_out() {
    let mut noisy = Command::new("bash");
    noisy.args(["-c", "for ((i=0; i<10000; i++)); do printf '0123456789abcdef\\n'; printf '0123456789abcdef\\n' >&2; done"]);
    let output = run_hook_process_with_timeout(&mut noisy, b"", std::time::Duration::from_secs(5))
        .expect("drain output");
    assert!(output.status.success());
    assert!(output.stdout.len() > 65536);
    assert!(output.stderr.len() > 65536);

    let mut overflowing = Command::new("dd");
    overflowing.args(["if=/dev/zero", "bs=1024", "count=17000"]);
    let error =
        run_hook_process_with_timeout(&mut overflowing, b"", std::time::Duration::from_secs(10))
            .expect_err("output above the capture limit must fail");
    assert!(error.to_string().contains("stdout"), "{error:#}");

    let dir = TempDir::new().unwrap();
    let pid_file = dir.path().join("descendant.pid");
    let mut hung = Command::new("bash");
    hung.arg("-c")
        .arg("sleep 300 & echo $! > \"$1\"; while :; do :; done")
        .arg("hung")
        .arg(&pid_file);
    let error = run_hook_process_with_timeout(&mut hung, b"", std::time::Duration::from_secs(2))
        .expect_err("hung child must time out");
    assert!(error.to_string().contains("hook test"), "{error:#}");
    let pid: u32 = fs::read_to_string(&pid_file)
        .expect("hung child records its descendant")
        .trim()
        .parse()
        .expect("descendant pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while loom::process::is_process_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        !loom::process::is_process_alive(pid),
        "descendant {pid} outlived the timed-out hook"
    );
}

/// Whether a `name --version` child spawns successfully, i.e. `name` is on
/// `PATH`. Used to skip a test that needs an external tool rather than fail
/// it on a host that lacks one.
pub fn tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Restores a process env var to its previous value on drop, on EVERY exit
/// path including a panic — so a test that pins one for its own duration can
/// never leak a stale value into whichever test the harness runs next.
pub struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

/// `loom::git::create_worktree` under a scratch `HOME`, so its internal
/// `trust_worktree` call — which resolves its write target through
/// `dirs::home_dir()` with no injectable override — lands in a throwaway
/// directory instead of the developer's real `~/.claude.json`. Tests that
/// spawn the binary instead use `loom_cmd().env("HOME", ...)` for the same
/// purpose.
pub fn create_worktree_isolated(
    stage_id: &str,
    repo_root: &Path,
    base_branch: Option<&str>,
) -> anyhow::Result<loom::models::worktree::Worktree> {
    let home = TempDir::new().expect("create scratch HOME");
    let _guard = EnvVarGuard::set("HOME", home.path());
    loom::git::create_worktree(stage_id, repo_root, base_branch, None)
}
