//! Shared test helpers for dependency inheritance integration tests

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::{Implementers, StageDefinition};

/// Test helper: Create a temporary git repository with initial commit
pub fn init_test_repo() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let repo_root = temp_dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to init git repo");

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to set git user.email");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to set git user.name");

    fs::write(repo_root.join("README.md"), "# Test Repository\n")
        .expect("Failed to write README.md");

    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git add");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git commit");

    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to rename branch to main");

    temp_dir
}

/// Test helper: Create a branch with a commit adding a file
pub fn create_branch_with_file(name: &str, filename: &str, content: &str, repo_root: &Path) {
    Command::new("git")
        .args(["checkout", "-b", name])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout new branch");

    fs::write(repo_root.join(filename), content).expect("Failed to write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git add");

    Command::new("git")
        .args(["commit", "-m", &format!("Add {filename}")])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git commit");

    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");
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
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            parallel_group: None,
            auto_merge: None,
            working_dir: ".".to_string(),
            sandbox: Default::default(),
            stage_type: None,
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_ceiling_tokens: None,
            removed_context_budget: None,
            plan_overview: None,
            execution_mode: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            code_review: None,
            ultracode: false,
            implementers: Implementers::default(),
            subagent_timeout_secs: None,
            skills: vec![],
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
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to checkout main");

    Command::new("git")
        .args(["merge", "--no-ff", "-m", &format!("Merge {branch}"), branch])
        .current_dir(repo_root)
        .output()
        .expect("Failed to merge");
}

/// Test helper: Delete a branch
pub fn delete_branch(branch: &str, repo_root: &Path) {
    Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_root)
        .output()
        .expect("Failed to delete branch");
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
/// exit, that directory is never cleaned up and is left behind in the system
/// temp dir after each test run — an accepted trade for a process-wide shared
/// home, not a leak to fix.
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

/// Every `LOOM_*` variable a spawned loom session may export
/// (`orchestrator/terminal/native/wrapper.rs` and its `host_env` submodule),
/// beyond what a bare `loom` invocation ever needs to see. A test that spawns
/// the real binary — or a hook script under test — must never let one of
/// these leak in from the process actually running `cargo test`: it would
/// silently make the child believe it is a stage/knowledge/merge/adjudication
/// session, or (via `LOOM_HOOK_PATH`/`LOOM_BIN`) resolve a hook script's PATH
/// and binary to something other than the test's own stub.
pub const RELAY_ENV_VARS_TO_CLEAR: &[&str] = &[
    "LOOM_SESSION_ID",
    "LOOM_STAGE_ID",
    "LOOM_WORK_DIR",
    "LOOM_WORKTREE_PATH",
    "LOOM_MAIN_AGENT_PID",
    "LOOM_SESSION_TYPE",
    "LOOM_MERGE_SESSION",
    "LOOM_SCRATCH_DIR",
    "LOOM_BIN",
    "LOOM_HOOK_PATH",
    "LOOM_HOOK_CONTEXT",
    "LOOM_CONTROL_BROKER",
];

/// Remove every variable in [`RELAY_ENV_VARS_TO_CLEAR`] from `command`. Callers
/// that need one of them for the scenario under test set it back afterward
/// with an explicit `.env(...)` — that always wins, since it runs after this
/// blanket clear.
pub fn clear_relay_env(command: &mut Command) -> &mut Command {
    for var in RELAY_ENV_VARS_TO_CLEAR {
        command.env_remove(var);
    }
    command
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
