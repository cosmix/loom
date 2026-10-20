use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use loom::models::stage::{Stage, StageStatus, StageType};
use loom::verify::transitions::save_stage;

/// Build a real git repo on branch `main` with an initial commit. Returns the
/// TempDir (callers keep it alive for the duration of the test).
pub(crate) fn init_repo() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    run_git(&["init", "-b", "main"], root);
    run_git(&["config", "user.email", "test@test.com"], root);
    run_git(&["config", "user.name", "Test"], root);
    fs::write(root.join("README.md"), "initial\n").expect("write README");
    run_git(&["add", "README.md"], root);
    run_git(&["commit", "-m", "initial"], root);
    run_git(&["branch", "-M", "main"], root);

    tmp
}

pub(crate) fn run_git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Create a branch, add one commit, return the commit SHA. Leaves caller on `main`.
pub(crate) fn create_loom_branch_with_commit(
    stage_id: &str,
    filename: &str,
    content: &str,
    repo_root: &Path,
) -> String {
    let branch = format!("loom/{stage_id}");
    run_git(&["checkout", "-b", &branch], repo_root);
    fs::write(repo_root.join(filename), content).expect("write file");
    run_git(&["add", filename], repo_root);
    run_git(&["commit", "-m", "stage work"], repo_root);

    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();

    run_git(&["checkout", "main"], repo_root);
    sha
}

/// Create an empty `.loom/work/` with a minimal `config.toml` pointing at `main`.
pub(crate) fn init_work_dir(repo_root: &Path) -> PathBuf {
    let work_dir = repo_root.join(".loom").join("work");
    fs::create_dir_all(work_dir.join("stages")).expect("mkdir .loom/work/stages");
    fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n").expect("write config.toml");
    work_dir
}

/// Build a Stage in the given Completed/merged state and write it to disk.
pub(crate) fn write_phantom_stage(
    stage_id: &str,
    merged: bool,
    completed_commit: Option<String>,
    work_dir: &Path,
) {
    let mut stage = Stage::new(stage_id.to_string(), Some(format!("test {stage_id}")));
    stage.id = stage_id.to_string();
    stage.stage_type = StageType::Standard;
    stage.status = StageStatus::Completed;
    stage.completed_at = Some(chrono::Utc::now());
    stage.merged = merged;
    stage.completed_commit = completed_commit;
    save_stage(&stage, work_dir).expect("save stage");
}

/// Guarded cwd change: restores the prior cwd even if the closure panics.
/// We use #[serial] on tests that enter this helper to avoid races.
pub(crate) fn with_cwd<F: FnOnce()>(dir: &Path, f: F) {
    let prior = std::env::current_dir().expect("getcwd");
    std::env::set_current_dir(dir).expect("set cwd to test repo");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::set_current_dir(&prior).expect("restore cwd");
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Restores a process env var to its previous value on drop, on EVERY exit
/// path including a panic. Modeled on the identically-named guard in
/// `loom/tests/e2e/daemon_config/mod.rs` — that one lives in a different test
/// binary and can't be imported here, so `phantom_merge.rs` keeps its own.
struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
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

/// Redirects `HOME` and `LOOM_HOME` to a scratch directory for the life of
/// the guard, restoring both on drop (including on panic).
///
/// `repair::execute` resolves `dirs::home_dir()` to find hook scripts, codex
/// hooks, `settings.json`, and other home-relative assets
/// (`commands/repair/settings_checks.rs::hook_scripts_issue`,
/// `commands/repair/hooks.rs::check`, `home_assets::check`). Without this
/// redirect, `repair::execute(true)` in-process installs THIS test binary's
/// embedded hook scripts into the developer's real `~/.claude/hooks/loom`
/// and `~/.codex/hooks/loom`, silently overwriting whatever the running
/// daemon expects.
///
/// Mutating `HOME`/`LOOM_HOME` process-wide is safe ONLY here: this file is
/// its own standalone integration-test binary, and every test in it that
/// calls `repair::execute` is already `#[serial]`, so no other test in this
/// process can observe the redirected value concurrently. The lib test
/// binary (`cargo test --lib`) has non-serial tests and must NEVER do this —
/// use the injectable `_to(dir)` variants (e.g. `install_loom_hooks_to`)
/// there instead.
pub(crate) struct HomeGuard {
    _temp: TempDir,
    _home_env: EnvVarGuard,
    _loom_home_env: EnvVarGuard,
    home: PathBuf,
}

impl HomeGuard {
    /// The scratch directory `HOME` now points at, e.g. to assert on
    /// `<home>/.claude/hooks/loom/commit-guard.sh` after `repair --fix`.
    pub(crate) fn home(&self) -> &Path {
        &self.home
    }
}

pub(crate) fn isolate_home() -> HomeGuard {
    let temp = TempDir::new().expect("tempdir for scratch home");
    let home = temp.path().to_path_buf();
    let loom_home = home.join(".loom");
    let home_str = home.to_str().expect("temp path is UTF-8");
    let loom_home_str = loom_home.to_str().expect("temp path is UTF-8");

    let _home_env = EnvVarGuard::set("HOME", home_str);
    let _loom_home_env = EnvVarGuard::set("LOOM_HOME", loom_home_str);

    HomeGuard {
        _temp: temp,
        _home_env,
        _loom_home_env,
        home,
    }
}

/// Asserts `repair --fix` installed hook scripts under the given scratch
/// `HOME`, proving the HOME redirect actually took effect rather than
/// leaking into the developer's real home.
pub(crate) fn assert_hooks_installed_under(home: &Path) {
    let installed_hook = home.join(".claude/hooks/loom/commit-guard.sh");
    assert!(
        installed_hook.exists(),
        "repair --fix must install hook scripts into the isolated scratch \
         HOME ({}), not the real developer home",
        installed_hook.display()
    );
}
