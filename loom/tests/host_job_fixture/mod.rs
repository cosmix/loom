//! Shared repository, worktree and session fixture for host jobs.

use crate::helpers;
use loom::fs::session_files::save_session;
use loom::models::session::{Session, SessionStatus, SessionType};
use loom::models::stage::{Stage, StageStatus};
use loom::plan::schema::AcceptanceCriterion;
use loom::verify::transitions::save_stage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
#[path = "../integration/relay_scratch.rs"]
mod relay_scratch;
use crate::helpers::test_support;

pub const STAGE: &str = "stage-a";
pub const SESSION: &str = "session-a";

/// Everything one scenario needs; the temp dirs live as long as the fixture.
pub struct Fixture {
    _dirs: Vec<TempDir>,
    pub work_dir: PathBuf,
    pub worktree: PathBuf,
    pub home: PathBuf,
    pub xdg: PathBuf,
    pub scratch_dir: PathBuf,
}

/// Point a command's git at missing global and system config.
pub fn isolate_git(command: &mut Command, home: &Path) {
    command
        .env("GIT_CONFIG_GLOBAL", home.join("no-global-gitconfig"))
        .env("GIT_CONFIG_SYSTEM", home.join("no-system-gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
}

/// Run git in `dir` with ambient config neutralised; returns trimmed stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git_checked(dir, args);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Capture the temporary main checkout before stage sessions use it.
pub fn capture_checkout_identity(repo: &Path, work_dir: &Path) {
    test_support::write_main_checkout_identity(repo, work_dir);
}

/// A repository whose one commit ignores `.loom/`, and the stage worktree.
fn init_repo_with_worktree() -> (TempDir, PathBuf) {
    let repo = TempDir::new().expect("create repo dir");
    let root = repo.path().to_path_buf();
    git(&root, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.email", "test@test.com"),
        ("user.name", "Test User"),
        ("commit.gpgsign", "false"),
    ] {
        git(&root, &["config", key, value]);
    }
    fs::write(root.join(".gitignore"), ".loom/\n").expect("write .gitignore");
    git(&root, &["add", ".gitignore"]);
    git(&root, &["commit", "-m", "initial"]);
    let worktree = root.join(".worktrees").join(STAGE);
    let branch = format!("loom/{STAGE}");
    git(
        &root,
        &["worktree", "add", "-b", &branch, worktree.to_str().unwrap()],
    );
    (repo, worktree)
}

/// stage-a: executing, plan version 1, owned by session-a, worktree stage-a.
pub fn write_stage(work_dir: &Path, acceptance: &[&str]) {
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::Executing,
        session: Some(SESSION.to_string()),
        worktree: Some(STAGE.to_string()),
        working_dir: Some(".".to_string()),
        plan_version: 1,
        acceptance: acceptance
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect(),
        ..Default::default()
    };
    save_stage(&stage, work_dir).expect("save stage record");
}

/// A running stage session-a whose worktree is the stage worktree.
fn write_session(work_dir: &Path, worktree: &Path) {
    let mut session = Session::new();
    session.id = SESSION.to_string();
    session.session_type = SessionType::Stage;
    session.stage_id = Some(STAGE.to_string());
    session.tracking_key = Session::derive_tracking_key(STAGE, SessionType::Stage);
    session.status = SessionStatus::Running;
    session.worktree_path = Some(worktree.to_path_buf());
    save_session(&session, work_dir).expect("save session record");

    let pid = std::process::id();
    let start_time = loom::process::process_start_time(pid).expect("own process start time");
    let pids_dir = work_dir.join("pids");
    fs::create_dir_all(&pids_dir).expect("create pids dir");
    let pid_file = format!("{}-{SESSION}.pid", session.tracking_key);
    fs::write(pids_dir.join(pid_file), format!("{pid}\n{start_time}\n")).expect("write pid entry");
}

/// A plan defining stage-a with `acceptance` and no goal checks, named by
/// `<work_dir>/config.toml`.
fn write_plan(work_dir: &Path, acceptance: &[&str]) {
    let plan = work_dir.join("PLAN-host.md");
    let mut yaml = format!(
        "loom:\n  version: 1\n  stages:\n    - id: {STAGE}\n      name: {STAGE}\n      working_dir: .\n      acceptance:\n"
    );
    for command in acceptance {
        let quoted = serde_json::to_string(command).expect("quote criterion");
        yaml.push_str(&format!("        - {quoted}\n"));
    }
    let body = format!(
        "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n<!-- END loom METADATA -->\n"
    );
    fs::write(&plan, body).expect("write plan");
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"host\"\nplan_name = \"host\"\nbase_branch = \"main\"\n",
        plan.display()
    );
    fs::write(work_dir.join("config.toml"), config).expect("write config.toml");
}

pub fn build_fixture(acceptance: &[&str]) -> Fixture {
    let home = relay_scratch::tempdir_outside_tmp("home-");
    let xdg = relay_scratch::tempdir_outside_tmp("xdg-");
    let (repo, worktree) = init_repo_with_worktree();
    let work_dir = repo.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");
    capture_checkout_identity(repo.path(), &work_dir);
    fs::create_dir_all(worktree.join(".loom")).expect("create worktree .loom dir");
    std::os::unix::fs::symlink(&work_dir, worktree.join(".loom/work"))
        .expect("link worktree .loom/work to the shared state");
    write_stage(&work_dir, acceptance);
    write_session(&work_dir, &worktree);
    write_plan(&work_dir, acceptance);
    let scratch_dir = relay_scratch::create_scratch_dir(xdg.path(), home.path(), SESSION);
    let (home_path, xdg_path) = (home.path().to_path_buf(), xdg.path().to_path_buf());
    Fixture {
        _dirs: vec![repo, home, xdg],
        work_dir,
        worktree,
        home: home_path,
        xdg: xdg_path,
        scratch_dir,
    }
}

impl Fixture {
    /// A `loom` command (via `helpers::loom_cmd`) as the stage session.
    pub fn session_loom(&self) -> Command {
        let mut command = helpers::loom_cmd();
        command
            .current_dir(&self.worktree)
            .env("LOOM_SESSION_ID", SESSION)
            .env("LOOM_STAGE_ID", STAGE)
            .env("LOOM_SESSION_TYPE", "stage")
            .env("LOOM_SCRATCH_DIR", &self.scratch_dir)
            .env("LOOM_WORK_DIR", &self.work_dir)
            .env("LOOM_WORKTREE_PATH", &self.worktree)
            .env("XDG_RUNTIME_DIR", &self.xdg)
            .env("HOME", &self.home);
        isolate_git(&mut command, &self.home);
        command
    }

    pub fn head(&self) -> String {
        git(&self.worktree, &["rev-parse", "HEAD"])
    }
}

/// A bare state root `<tmp>/.loom/work`.
pub fn state_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create state root parent");
    let work_dir = dir.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).expect("create state root");
    (dir, work_dir)
}
